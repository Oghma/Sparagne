import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Who a row may be for (`AppStore+People.swift`): the members of the vault
/// when the sync engine has them, the names the vault knows otherwise, and
/// how a typed name resolves to one of them. The logged-in half, with a
/// server behind it, is in `SyncEngineTests`.
@MainActor
struct PeopleTests {
    /// Vault `Casa` with wallet `Conto`, written by matteo while logged out.
    private static func onboarded() async throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.people.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 10_000)
        #expect(store.presentedError == nil)
        return store
    }

    /// A monthly expense owned by `owner`, written straight to the core.
    private static func template(owned owner: String, in store: AppStore) async throws {
        let vault = try #require(store.currentVault)
        try await store.core.execute(
            vaultId: vault.id,
            .createRecurring(
                transactionKind: .expense,
                amount: 50_000,
                walletId: nil,
                flowId: nil,
                category: "Casa",
                note: "mutuo",
                schedule: Schedule(frequency: .monthly(day: 1), interval: 1, startDate: "2030-01-01", endDate: nil),
                owner: owner
            )
        )
        await store.loadRecurringTemplates()
        await store.reload()
    }

    @Test("Logged out, a row may be for the author, the persons of the rows and the owners of the templates, each once")
    func fallbackWithoutMembers() async throws {
        let store = try await Self.onboarded()
        await store.setAuthor("elisa")
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 1_000)
        await store.setAuthor("matteo")
        try await Self.template(owned: "paolo", in: store)
        #expect(store.presentedError == nil)

        // The rows are matteo's opening balance and elisa's coop.
        #expect(store.peopleInRows == ["elisa", "matteo"])
        // The author first, then the rows' persons, then the owners: matteo
        // is in the rows too, and is listed once.
        #expect(store.assignablePeople == ["matteo", "elisa", "paolo"])
    }

    @Test("A vault with no rows yet still offers its author")
    func emptyVaultOffersTheAuthor() async throws {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.people.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Vuoto", walletName: "", openingBalance: 0)
        #expect(store.peopleInRows.isEmpty)
        #expect(store.assignablePeople == ["matteo"])
    }

    @Test("The members heard from the server replace the names the vault knows, for their own vault only")
    func membersWinForTheirVault() async throws {
        let store = try await Self.onboarded()
        let casa = try #require(store.currentVault)
        store.setVaultMembers([casa.id: ["matteo", "elisa", "carol"]])
        // carol has no row, and is offered all the same.
        #expect(store.assignablePeople == ["matteo", "elisa", "carol"])
        #expect(!store.peopleInRows.contains("carol"))

        await store.createVault(name: "Viaggi", walletName: "Carta", openingBalance: 0)
        #expect(store.assignablePeople == ["matteo"])

        // Logged out again: the engine hands over no members at all.
        await store.select(casa)
        store.setVaultMembers([:])
        #expect(store.assignablePeople == ["matteo"])
    }

    @Test("A typed name resolves exactly, then by a unique prefix, then by a unique substring")
    func resolution() async throws {
        let store = try await Self.onboarded()
        let vault = try #require(store.currentVault)
        store.setVaultMembers([vault.id: ["matteo", "elisa", "elena"]])

        #expect(try store.resolvePerson(named: "") == nil)
        #expect(try store.resolvePerson(named: "  ") == nil)
        #expect(try store.resolvePerson(named: "Elisa") == "elisa")
        #expect(try store.resolvePerson(named: "mat") == "matteo")
        #expect(try store.resolvePerson(named: "isa") == "elisa")

        // Two names start with "el": the cell is refused, not guessed.
        #expect(throws: DomainError.self) { try store.resolvePerson(named: "el") }
        // Nobody of that name may be named here.
        do {
            _ = try store.resolvePerson(named: "paolo")
            Issue.record("an unknown person resolved")
        } catch let error as DomainError {
            #expect(error.code == "not_found")
            #expect(error.message == ErrorMessages.unknownPerson("paolo"))
        }
    }

    @Test("One person under two spellings never ties with itself, in the PERSONA cell or behind a !")
    func twoSpellingsOfOnePerson() async throws {
        let store = try await Self.onboarded()
        // elisa's rows from before she had a username, and from after.
        await store.setAuthor("Elisa")
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 1_000)
        await store.setAuthor("elisa")
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "conad", amount: 1_000)
        await store.setAuthor("matteo")
        #expect(store.presentedError == nil)
        #expect(store.assignablePeople.contains("Elisa") && store.assignablePeople.contains("elisa"))
        #expect(AppStore.folded(store.assignablePeople) == ["matteo", "elisa"])

        // The cell: the name as written, then one person per spelling.
        #expect(try store.resolvePerson(named: "Elisa") == "Elisa")
        #expect(try store.resolvePerson(named: "ELISA") == "elisa")
        #expect(try store.resolvePerson(named: "eli") == "elisa")

        // The line: a prefix and a shout both find the username.
        await store.submit(quickAdd: "-24 cena !eli")
        await store.submit(quickAdd: "-3 caffè !ELISA")
        #expect(store.presentedError == nil)
        #expect(store.rows.first { $0.note == "cena" }?.person == "elisa")
        #expect(store.rows.first { $0.note == "caffè" }?.person == "elisa")
    }

    @Test("Logged out under the local name Matteo, with rows by the username matteo, !matteo is not ambiguous")
    func localNameBesideItsUsername() async throws {
        let store = try await Self.onboarded()
        await store.setAuthor("Matteo")
        #expect(store.assignablePeople.contains("Matteo") && store.assignablePeople.contains("matteo"))

        await store.submit(quickAdd: "-24 cena !matteo")
        #expect(store.presentedError == nil)
        #expect(store.rows.contains { $0.note == "cena" })
    }

    @Test("Spellings that differ only in case are folded into the lowercase one, at the first one's place")
    func folding() {
        #expect(AppStore.folded(["Matteo", "elisa", "matteo", "ELISA", " ", "Elisa"]) == ["matteo", "elisa"])
        // With no lowercase spelling, the first one stays.
        #expect(AppStore.folded(["Elisa", "ELISA", "bob"]) == ["Elisa", "bob"])
    }

    // MARK: - Quick add

    @Test("A quick-add !name puts the row on that member, recorded by the author")
    func quickAddPerson() async throws {
        let store = try await Self.onboarded()
        let vault = try #require(store.currentVault)
        store.setVaultMembers([vault.id: ["matteo", "elisa"]])

        guard case .success(let parsed) = store.preview(quickAdd: "-24 cena !eli") else {
            Issue.record("expected the line to parse")
            return
        }
        #expect(QuickAddSummary.describe(parsed, currency: .eur).contains("!eli"))

        await store.submit(quickAdd: "-24 cena !eli")
        #expect(store.presentedError == nil)
        let row = try #require(store.rows.first { $0.note == "cena" })
        #expect(row.person == "elisa")
        #expect(row.recordedBy == "matteo")
        #expect(store.transactions.first { $0.id == row.id }?.createdBy == "matteo")
    }

    @Test("A !name nobody here goes by is refused with its own headline, and nothing is written")
    func quickAddUnknownPerson() async throws {
        let store = try await Self.onboarded()
        let vault = try #require(store.currentVault)
        store.setVaultMembers([vault.id: ["matteo", "elisa"]])

        await store.submit(quickAdd: "-24 cena !paolo")
        let error = try #require(store.presentedError)
        #expect(error.code == "unknown_name")
        #expect(error.summary == ErrorMessages.unknownPerson("paolo"))
        #expect(!store.rows.contains { $0.note == "cena" })
    }

    @Test("An ambiguous !name asks which person, and the choice is written behind the !")
    func quickAddAmbiguousPerson() async throws {
        let store = try await Self.onboarded()
        let vault = try #require(store.currentVault)
        store.setVaultMembers([vault.id: ["matteo", "elisa", "elena"]])

        // `#elettricità` starts with the fragment too: only the whole token
        // behind the `!` is the person.
        store.quickAddText = "-24 cena #elettricità !el"
        await store.submit(quickAdd: store.quickAddText)
        let error = try #require(store.presentedError)
        #expect(error.code == "ambiguous_name")
        #expect(error.candidates.sorted() == ["elena", "elisa"])
        #expect(error.summary == ErrorMessages.ambiguousPerson)

        await store.resolveAmbiguous(choosing: "elisa")
        #expect(store.presentedError == nil)
        #expect(store.rows.first { $0.note == "cena" }?.person == "elisa")
        #expect(store.categories.contains { $0.name == "elettricità" })
    }

    @Test("Rewriting a choice keeps each marker and finds the whole token")
    func rewrite() {
        #expect(AppStore.rewrite("-5 pizza !el", fragment: "el", with: "elisa") == "-5 pizza !elisa")
        #expect(AppStore.rewrite("-5 pizza #elisir !el", fragment: "el", with: "elisa") == "-5 pizza #elisir !elisa")
        #expect(AppStore.rewrite("-5 hotel @ban", fragment: "ban", with: "Bank") == "-5 hotel @Bank")
        #expect(AppStore.rewrite("-5 hotel", fragment: "ban", with: "Bank") == "-5 hotel")
        #expect(AppStore.marker(carrying: "el", in: "-5 #ele !el")?.marker == "!")
        #expect(AppStore.marker(carrying: "food", in: "-5 #food")?.marker == "#")
    }

    @Test("Blanks and repeats are dropped, the first order kept")
    func distinct() {
        #expect(AppStore.distinct(["b", "a", "b", " ", "", "c", "a"]) == ["b", "a", "c"])
    }
}
