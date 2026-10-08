import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The PERSONA cell of the grid (`RowDraft.person`): who a row is for, typed
/// on the empty line or into an open row, sent only when it changed, undone
/// like any other cell.
@MainActor
struct PersonCellTests {
    /// Vault `Casa` with wallet `Conto`, written by matteo, whose members are
    /// matteo, elisa and bob; an undo manager attached once it is set up.
    private static func ledger() async throws -> (store: AppStore, manager: UndoManager) {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.person.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults, undoWindow: .seconds(60))
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        let vault = try #require(store.currentVault)
        store.setVaultMembers([vault.id: ["matteo", "elisa", "bob"]])
        return (store, try LedgerHistoryTests.attachManager(to: store))
    }

    /// Saves the empty line the way the grid's ↩ does.
    private static func save(_ draft: RowDraft, in store: AppStore) async throws {
        let entry = try #require(try draft.entry(store: store))
        await store.addRow(
            day: entry.day,
            flowId: entry.flowId,
            category: entry.category,
            note: entry.note,
            amount: entry.amount,
            walletId: entry.walletId,
            kind: entry.kind,
            person: entry.person
        )
        #expect(store.presentedError == nil)
    }

    private static func line(note: String, amount: String, person: String? = nil, in store: AppStore) -> RowDraft {
        var draft = RowDraft.blank(in: store)
        draft.note = note
        draft.amount = amount
        if let person { draft.person = person }
        return draft
    }

    @Test("The empty line is the author's until its PERSONA cell names someone else, by a prefix if need be")
    func newRowPerson() async throws {
        let (store, _) = try await Self.ledger()
        #expect(RowDraft.blank(in: store).person == "matteo")

        try await Self.save(Self.line(note: "pizza", amount: "12", in: store), in: store)
        let pizza = try #require(store.rows.first { $0.note == "pizza" })
        #expect(pizza.person == "matteo")
        #expect(pizza.recordedBy == "matteo")
        #expect(!pizza.isOnBehalf)

        try await Self.save(Self.line(note: "cena", amount: "24", person: "eli", in: store), in: store)
        let dinner = try #require(store.rows.first { $0.note == "cena" })
        #expect(dinner.person == "elisa")
        #expect(dinner.recordedBy == "matteo")
        #expect(dinner.isOnBehalf)

        // The author is sent as no person, so the command reads as one from
        // before rows could be for someone else; anyone else by name.
        #expect(store.explicitPerson("matteo") == nil)
        #expect(store.explicitPerson(" ") == nil)
        #expect(store.explicitPerson(nil) == nil)
        #expect(store.explicitPerson("elisa") == "elisa")
    }

    @Test("Editing the PERSONA cell sends the person alone, and Undo gives the row back to whoever it was for")
    func editAndUndo() async throws {
        let (store, manager) = try await Self.ledger()
        try await Self.save(Self.line(note: "pizza", amount: "12", in: store), in: store)
        let row = try #require(store.rows.first { $0.note == "pizza" })

        var draft = RowDraft(row: row, store: store)
        #expect(draft.person == "matteo")
        // Untouched, the cell sends nothing.
        #expect(try draft.patch(against: row, store: store).isEmpty)
        draft.person = "Elisa"
        let patch = try draft.patch(against: row, store: store)
        #expect(patch == TransactionPatch(person: "elisa"))

        await store.update(transactionId: row.id, patch: patch)
        #expect(store.presentedError == nil)
        let edited = try #require(store.rows.first { $0.id == row.id })
        #expect(edited.person == "elisa")
        #expect(edited.recordedBy == "matteo")
        // VoiceOver says both, the way the tooltip does.
        let spoken = LedgerAccessibility.label(for: edited)
        #expect(spoken.contains(LedgerAccessibility.person("elisa")))
        #expect(spoken.contains(LedgerAccessibility.recordedBy("matteo")))
        #expect(!LedgerAccessibility.label(for: row).contains(LedgerAccessibility.recordedBy("matteo")))

        // Back to the one who recorded it is the patch's blank, not a name.
        var back = RowDraft(row: edited, store: store)
        back.person = "matteo"
        #expect(try back.patch(against: edited, store: store) == TransactionPatch(person: ""))

        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(store.rows.first { $0.id == row.id }?.person == "matteo")

        manager.redo()
        await store.settle()
        #expect(store.rows.first { $0.id == row.id }?.person == "elisa")
    }

    @Test("Undo of a row moved between two others gives it back by name")
    func undoBetweenOthers() async throws {
        let (store, manager) = try await Self.ledger()
        try await Self.save(Self.line(note: "cena", amount: "24", person: "elisa", in: store), in: store)
        let row = try #require(store.rows.first { $0.note == "cena" })
        let view = try #require(store.transactions.first { $0.id == row.id })
        #expect(AppStore.inverse(of: TransactionPatch(person: "bob"), from: view) == TransactionPatch(person: "elisa"))

        await store.update(transactionId: row.id, patch: TransactionPatch(person: "bob"))
        #expect(store.rows.first { $0.id == row.id }?.person == "bob")
        manager.undo()
        await store.settle()
        #expect(store.rows.first { $0.id == row.id }?.person == "elisa")
    }

    @Test("A name nobody here may be named by is refused on the PERSONA cell, and nothing is written")
    func unknownPerson() async throws {
        let (store, _) = try await Self.ledger()
        try await Self.save(Self.line(note: "pizza", amount: "12", in: store), in: store)
        let row = try #require(store.rows.first { $0.note == "pizza" })
        let count = store.rows.count

        var draft = RowDraft(row: row, store: store)
        draft.person = "paolo"
        do {
            _ = try draft.patch(against: row, store: store)
            Issue.record("an unknown person was accepted")
        } catch let failure as RowDraftError {
            #expect(failure.field == .person)
            #expect((failure.underlying as? AppError)?.code == "not_found")
            // The alert the grid raises has the quick-add line's headline,
            // not a bare "Not found".
            store.report(failure.underlying)
            #expect(store.presentedError?.summary == ErrorMessages.unknownPerson("paolo"))
            store.presentedError = nil
        }

        do {
            _ = try Self.line(note: "cena", amount: "24", person: "paolo", in: store).entry(store: store)
            Issue.record("an unknown person was accepted")
        } catch let failure as RowDraftError {
            #expect(failure.field == .person)
        }
        #expect(store.rows.count == count)
    }

    @Test("A transfer's PERSONA cell is not a field: it stays its author's, whatever the draft says")
    func transferStaysTheAuthors() async throws {
        let (store, _) = try await Self.ledger()
        await store.createWallet(name: "Carta", openingBalance: 0)
        let vault = try #require(store.currentVault)
        let from = try #require(store.wallets.first { $0.name == "Conto" })
        let to = try #require(store.wallets.first { $0.name == "Carta" })
        try await store.core.execute(
            vaultId: vault.id,
            .transferWallet(amount: 1_000, fromWalletId: from.id, toWalletId: to.id, note: "giroconto", occurredAt: CoreDate.offset(Date()))
        )
        store.showTransfers = true
        await store.settle()
        let transfer = try #require(store.rows.first { $0.note == "giroconto" })
        #expect(!transfer.isPersonEditable)

        var draft = RowDraft(row: transfer, store: store)
        draft.person = "elisa"
        #expect(try draft.patch(against: transfer, store: store).isEmpty)
    }
}
