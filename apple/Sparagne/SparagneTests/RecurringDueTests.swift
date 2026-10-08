import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// "Da confermare" on the Ricorrenze tab (`DueRecurringCard`): the periods
/// a template is waiting on, and what Registra, Salta and Registra tutte
/// leave behind. Every assertion reads the core's own answer after the
/// command, not the card.
struct RecurringDueTests {
    /// Vault `Main`, wallet `Cash` holding 100.00.
    private static func onboarded() async throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.recurring.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 10_000)
        #expect(store.presentedError == nil)
        return store
    }

    /// A daily expense that started two days ago: three periods due, the day
    /// before yesterday, yesterday and today.
    @discardableResult
    private static func dailyExpense(
        _ store: AppStore,
        note: String,
        amount: Int64,
        flowId: Uuid? = nil,
        owner: String? = nil
    ) async throws -> RecurringView {
        let start = try #require(Calendar.current.date(byAdding: .day, value: -2, to: Date()))
        await store.createRecurring(
            kind: .expense,
            amount: amount,
            walletId: nil,
            flowId: flowId,
            category: "Bills",
            note: note,
            schedule: Schedule(frequency: .daily, interval: 1, startDate: CoreDate.day(start), endDate: nil),
            owner: owner
        )
        #expect(store.presentedError == nil)
        return try #require(store.recurringTemplates.first { $0.note == note })
    }

    private static func due(_ store: AppStore, _ template: RecurringView) -> [NaiveDate] {
        store.pendingRecurringItems.first { $0.template.id == template.id }?.due ?? []
    }

    /// The balance of `Cash`, in cents.
    private static func cash(_ store: AppStore) throws -> Int64 {
        try #require(store.wallets.first).balance
    }

    @Test("The due list is every period of every template, oldest first")
    func duePeriodsAreOldestFirst() async throws {
        let store = try await Self.onboarded()
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 100)
        let gym = try await Self.dailyExpense(store, note: "Gym", amount: 50)

        let periods = store.duePeriods
        #expect(periods.count == 6)
        #expect(periods.map(\.date) == periods.map(\.date).sorted())
        #expect(Set(periods.map(\.template.id)) == [rent.id, gym.id])
        #expect(Self.due(store, rent) == Self.due(store, rent).sorted())
    }

    @Test("Executing one period writes its transaction and leaves the other periods due")
    func executeOnePeriod() async throws {
        let store = try await Self.onboarded()
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 999)
        let first = try #require(Self.due(store, rent).first)

        await store.executeRecurring(rent.id, periodDate: first)

        #expect(store.presentedError == nil)
        #expect(Self.due(store, rent).count == 2)
        #expect(!Self.due(store, rent).contains(first))
        let spent: Int64 = 999
        #expect(try Self.cash(store) == 10_000 - spent)
    }

    // MARK: - The owner

    @Test("A template is its creator's unless another owner was picked, and an edit moves it")
    func ownerOnCreateAndEdit() async throws {
        let store = try await Self.onboarded()
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 999)
        let gym = try await Self.dailyExpense(store, note: "Gym", amount: 50, owner: "elisa")
        #expect(rent.owner == "tester")
        #expect(gym.owner == "elisa")

        await store.updateRecurring(rent.id, patch: RecurringPatch(owner: "elisa"))
        #expect(store.presentedError == nil)
        #expect(store.recurringTemplates.first { $0.id == rent.id }?.owner == "elisa")
    }

    @Test("Registra records the period for the template's owner, by whoever pressed it")
    func executeForTheOwner() async throws {
        let store = try await Self.onboarded()
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 999, owner: "elisa")
        let first = try #require(Self.due(store, rent).first)

        await store.executeRecurring(rent.id, periodDate: first)

        #expect(store.presentedError == nil)
        let row = try #require(try await Self.everyRow(store).first { $0.note == "Rent" })
        #expect(row.person == "elisa")
        #expect(row.createdBy == "tester")
    }

    /// Every row of the vault, whatever month the periods fell in: two days
    /// ago may be last month.
    private static func everyRow(_ store: AppStore) async throws -> [TransactionView] {
        try await VaultExporter.allTransactions(core: store.core, vaultId: try #require(store.currentVault).id)
    }

    @Test("Registra tutte records each period for its own template's owner")
    func executeAllForEachOwner() async throws {
        let store = try await Self.onboarded()
        try await Self.dailyExpense(store, note: "Rent", amount: 100, owner: "elisa")
        try await Self.dailyExpense(store, note: "Gym", amount: 50)

        await store.executeAllDueRecurring()

        #expect(store.presentedError == nil)
        let rows = try await Self.everyRow(store).filter { $0.note == "Rent" || $0.note == "Gym" }
        #expect(rows.count == 6)
        #expect(rows.allSatisfy { $0.createdBy == "tester" })
        #expect(rows.filter { $0.note == "Rent" }.allSatisfy { $0.person == "elisa" })
        #expect(rows.filter { $0.note == "Gym" }.allSatisfy { $0.person == "tester" })
    }

    @Test("Under the PERSONA filter the Mastro shows the due periods of that person's templates only")
    func pendingRowsFollowThePersonFilter() async throws {
        let store = try await Self.onboarded()
        try await Self.dailyExpense(store, note: "Rent", amount: 100, owner: "elisa")
        try await Self.dailyExpense(store, note: "Gym", amount: 50)
        // A row for elisa, so the filter has her to show.
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 10, person: "elisa")

        func pendingNotes() -> Set<String> {
            let lines = LedgerLines.interleave(
                rows: store.ledgerLinesInput.rows,
                due: store.ledgerLinesInput.due,
                month: store.month,
                direction: store.direction,
                person: store.ledgerLinesInput.person
            )
            return Set(lines.compactMap { line in
                if case .pending(let period) = line { period.template.note } else { nil }
            })
        }
        // The month of the periods: two days ago may be the month before.
        #expect(!pendingNotes().isEmpty)

        store.person = "elisa"
        await store.settle()
        #expect(store.person == "elisa")
        #expect(pendingNotes() == ["Rent"])

        store.person = "tester"
        await store.settle()
        #expect(pendingNotes() == ["Gym"])
    }

    // MARK: - An owner who left the vault

    @Test("An owner has left when the members are known and are not them; never the author, never without a list")
    func ownerHasLeftRule() {
        #expect(AppStore.ownerHasLeft("elisa", members: ["tester", "bob"], author: "tester"))
        #expect(!AppStore.ownerHasLeft("bob", members: ["tester", "bob"], author: "tester"))
        // The author is left out of the command: the server never sees them.
        #expect(!AppStore.ownerHasLeft("Tester", members: ["bob"], author: "tester"))
        // No list: logged out, a demo database, or not heard yet.
        #expect(!AppStore.ownerHasLeft("elisa", members: nil, author: "tester"))
        #expect(!AppStore.ownerHasLeft("elisa", members: [], author: "tester"))
        #expect(!AppStore.ownerHasLeft(" ", members: ["bob"], author: "tester"))
    }

    @Test("A template whose owner left can only skip its periods; Registra tutte records the others and counts what it left")
    func ownerWhoLeft() async throws {
        let store = try await Self.onboarded()
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 100, owner: "elisa")
        let gym = try await Self.dailyExpense(store, note: "Gym", amount: 50)
        let vault = try #require(store.currentVault)
        // Logged in, and elisa is not among the members any more.
        store.setVaultMembers([vault.id: ["tester", "bob"]])
        #expect(store.ownerHasLeft(rent))
        #expect(!store.ownerHasLeft(gym))

        // Registra, pressed anyway, never reaches the core.
        let first = try #require(Self.due(store, rent).first)
        await store.executeRecurring(rent.id, periodDate: first)
        let refusal = try #require(store.presentedError)
        #expect(refusal.code == "not_a_member")
        #expect(refusal.summary.contains("elisa"))
        #expect(Self.due(store, rent).count == 3)
        store.presentedError = nil

        // Registra tutte records gym's three, leaves rent's three and says so.
        await store.executeAllDueRecurring()
        let held = try #require(store.presentedError)
        #expect(held.code == "not_a_member")
        #expect(held.summary == AppStore.periodsNotRecorded(3))
        #expect(Self.due(store, gym).isEmpty)
        #expect(Self.due(store, rent).count == 3)
        #expect(try Self.cash(store) == 10_000 - 3 * 50)
        store.presentedError = nil

        // Salta still works.
        await store.skipRecurring(rent.id, periodDate: first)
        #expect(store.presentedError == nil)
        #expect(Self.due(store, rent).count == 2)

        // Given to a member, the template records again.
        await store.updateRecurring(rent.id, patch: RecurringPatch(owner: "bob"))
        let given = try #require(store.recurringTemplates.first { $0.id == rent.id })
        #expect(!store.ownerHasLeft(given))
        await store.executeAllDueRecurring()
        #expect(store.presentedError == nil)
        #expect(Self.due(store, rent).isEmpty)
    }

    @Test("Logged out, an owner nobody checks records as before")
    func ownerUncheckedWhileLoggedOut() async throws {
        let store = try await Self.onboarded()
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 100, owner: "elisa")
        store.setVaultMembers(nil)
        #expect(!store.ownerHasLeft(rent))
        await store.executeAllDueRecurring()
        #expect(store.presentedError == nil)
        #expect(Self.due(store, rent).isEmpty)
    }

    @Test("Skipping a period takes it off the list and writes nothing")
    func skipOnePeriod() async throws {
        let store = try await Self.onboarded()
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 999)
        let first = try #require(Self.due(store, rent).first)

        await store.skipRecurring(rent.id, periodDate: first)

        #expect(store.presentedError == nil)
        #expect(Self.due(store, rent).count == 2)
        #expect(!Self.due(store, rent).contains(first))
        #expect(try Self.cash(store) == 10_000)
    }

    @Test("Execute All writes every due period of every template, one command each")
    func executeAll() async throws {
        let store = try await Self.onboarded()
        try await Self.dailyExpense(store, note: "Rent", amount: 100)
        try await Self.dailyExpense(store, note: "Gym", amount: 50)
        let vault = try #require(store.currentVault)
        let outbox = try await store.core.syncState(vaultId: vault.id).outbox

        await store.executeAllDueRecurring()

        #expect(store.presentedError == nil)
        #expect(store.pendingRecurringItems.isEmpty)
        #expect(store.duePeriods.isEmpty)
        let spent: Int64 = 3 * 100 + 3 * 50
        #expect(try Self.cash(store) == 10_000 - spent)
        // One batch, but every period keeps its own command in the log.
        #expect(try await store.core.syncState(vaultId: vault.id).outbox == outbox + 6)
    }

    @Test("Execute All is all or nothing: one period refused leaves every period due")
    func executeAllIsAtomic() async throws {
        let store = try await Self.onboarded()
        // An envelope with nothing in it that may not go below zero: its
        // template is refused, the one on Unallocated would not be.
        await store.createEnvelope(name: "Vacanze", mode: .unlimited, allowNegative: false, openingAllocation: 0)
        let vacanze = try #require(store.flows.first { $0.name == "Vacanze" })
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 100)
        let hotel = try await Self.dailyExpense(store, note: "Hotel", amount: 500, flowId: vacanze.id)

        await store.executeAllDueRecurring()

        #expect(store.presentedError?.code == "insufficient_funds")
        #expect(Self.due(store, rent).count == 3)
        #expect(Self.due(store, hotel).count == 3)
        #expect(try Self.cash(store) == 10_000)
    }

    @Test("An archived template stays in the panel's list and can be restored")
    func restoreAnArchivedTemplate() async throws {
        let store = try await Self.onboarded()
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 100)

        await store.archiveRecurring(rent.id)
        #expect(store.presentedError == nil)
        #expect(store.recurringTemplates.first { $0.id == rent.id }?.archived == true)
        // An archived template has nothing due.
        #expect(Self.due(store, rent).isEmpty)

        await store.restoreRecurring(rent.id)

        #expect(store.presentedError == nil)
        #expect(store.recurringTemplates.first { $0.id == rent.id }?.archived == false)
        #expect(Self.due(store, rent).count == 3)
    }

    @Test("Creating a template answers with its id, whatever else the list holds")
    func createAnswersWithTheNewId() async throws {
        let store = try await Self.onboarded()
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 100)
        let today = CoreDate.day(Date())

        let created = await store.createRecurring(
            kind: .income,
            amount: 2_000,
            walletId: nil,
            flowId: nil,
            category: "Salary",
            note: "Payroll",
            schedule: Schedule(frequency: .monthly(day: 27), interval: 1, startDate: today, endDate: nil)
        )

        #expect(store.presentedError == nil)
        let id = try #require(created)
        #expect(id != rent.id)
        #expect(store.recurringTemplates.first { $0.id == id }?.note == "Payroll")
    }

    @Test("A refused template answers with no id")
    func refusedCreateAnswersNil() async throws {
        let store = try await Self.onboarded()
        let vault = try #require(store.currentVault)
        store.setReadOnlyVaults([vault.id])

        let created = await store.createRecurring(
            kind: .expense,
            amount: 100,
            walletId: nil,
            flowId: nil,
            category: "Bills",
            note: "Rent",
            schedule: Schedule(frequency: .daily, interval: 1, startDate: CoreDate.day(Date()), endDate: nil)
        )

        #expect(created == nil)
        #expect(store.presentedError?.code == "forbidden")
        #expect(store.recurringTemplates.isEmpty)
    }

    @Test("A read-only vault refuses Execute All before it reaches the core")
    func readOnlyRefusesExecuteAll() async throws {
        let store = try await Self.onboarded()
        let rent = try await Self.dailyExpense(store, note: "Rent", amount: 100)
        let vault = try #require(store.currentVault)
        store.setReadOnlyVaults([vault.id])

        await store.executeAllDueRecurring()

        #expect(store.presentedError?.code == "forbidden")
        #expect(Self.due(store, rent).count == 3)
        #expect(try Self.cash(store) == 10_000)
    }
}
