import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// "Da confermare" on the Ricorrenze tab (`DueRecurringCard`): the periods
/// a template is waiting on, and what Registra, Salta and Registra tutte
/// leave behind. Every assertion reads the core's own answer after the
/// command, not the card.
@MainActor
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
        flowId: Uuid? = nil
    ) async throws -> RecurringView {
        let start = try #require(Calendar.current.date(byAdding: .day, value: -2, to: Date()))
        await store.createRecurring(
            kind: .expense,
            amount: amount,
            walletId: nil,
            flowId: flowId,
            category: "Bills",
            note: note,
            schedule: Schedule(frequency: .daily, interval: 1, startDate: CoreDate.day(start), endDate: nil)
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
