import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// A template turned into the other kind, and a template without a wallet in
/// a vault of several, against the real core. Every assertion reads what
/// the core holds after the command.
struct RecurringKindStoreTests {
    /// Vault `Main`, wallet `Cash` holding 100.00.
    private static func onboarded() async throws -> AppStore {
        try await TestStore.make(vault: "Main", wallet: "Cash", opening: 10_000, author: "tester")
    }

    private static var today: NaiveDate { CoreDate.day(Date()) }

    private static func day(_ offset: Int) throws -> NaiveDate {
        try #require(NaiveDay.adding(offset, to: today))
    }

    /// A daily expense that started two days ago: the day before yesterday,
    /// yesterday and today are due. `until` ends it early.
    private static func dailyExpense(
        _ store: AppStore,
        note: String = "Gym",
        walletId: Uuid? = nil,
        flowId: Uuid? = nil,
        until end: NaiveDate? = nil,
        owner: String? = nil
    ) async throws -> RecurringView {
        let created = await store.createRecurring(
            kind: .expense,
            amount: 50,
            walletId: walletId,
            flowId: flowId,
            category: "Bills",
            note: note,
            schedule: Schedule(frequency: .daily, interval: 1, startDate: try day(-2), endDate: end),
            owner: owner
        )
        #expect(store.presentedError == nil)
        return try template(store, try #require(created))
    }

    private static func template(_ store: AppStore, _ id: Uuid) throws -> RecurringView {
        try #require(store.recurringTemplates.first { $0.id == id })
    }

    /// Every period of `template` still due, recorded or skipped one by one.
    private static func decideAll(_ store: AppStore, _ template: RecurringView) async {
        for (index, date) in store.dueDates(of: template.id).enumerated() {
            if index == 0 {
                await store.executeRecurring(template.id, periodDate: date)
            } else {
                await store.skipRecurring(template.id, periodDate: date)
            }
        }
        #expect(store.dueDates(of: template.id).isEmpty)
    }

    private static func outbox(_ store: AppStore) async throws -> UInt32 {
        try await store.core.syncState(vaultId: try #require(store.currentVault).id).outbox
    }

    // MARK: - The change

    @Test("The template is archived and its copy of the other kind starts after the last period decided, with the same fields")
    func expenseBecomesIncome() async throws {
        let store = try await Self.onboarded()
        await store.createEnvelope(name: "Casa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let casa = try #require(store.flows.first { $0.name == "Casa" })
        let cash = try #require(store.wallets.first)
        let gym = try await Self.dailyExpense(store, walletId: cash.id, flowId: casa.id, until: try Self.day(30), owner: "elisa")
        await Self.decideAll(store, gym)
        #expect(await store.recurringKindStart(gym.id) == .success(try Self.day(1)))
        #expect(await store.recurringKindRefusal(gym.id) == nil)
        let before = try await Self.outbox(store)

        let id = try #require(await store.changeRecurringKind(gym.id))

        #expect(store.presentedError == nil)
        #expect(try Self.template(store, gym.id).archived)
        let copy = try Self.template(store, id)
        #expect(copy.kind == .income)
        #expect(copy.amount == gym.amount)
        #expect(copy.walletId == cash.id)
        #expect(copy.flowId == casa.id)
        #expect(copy.category == gym.category)
        #expect(copy.note == gym.note)
        #expect(copy.owner == "elisa")
        #expect(copy.enabled)
        #expect(!copy.archived)
        #expect(copy.schedule == Schedule(frequency: .daily, interval: 1, startDate: try Self.day(1), endDate: try Self.day(30)))
        // No period comes due twice: the copy's first is tomorrow.
        #expect(store.pendingRecurringItems.isEmpty)
        // One batch: the archive and the copy, each its own command.
        #expect(try await Self.outbox(store) == before + 2)
    }

    @Test("An income with nothing decided becomes an expense from the schedule's first period")
    func incomeBecomesExpense() async throws {
        let store = try await Self.onboarded()
        let start = try Self.day(10)
        let salary = try #require(await store.createRecurring(
            kind: .income,
            amount: 240_000,
            walletId: nil,
            flowId: nil,
            category: "Salary",
            note: "Payroll",
            schedule: Schedule(frequency: .monthly(day: 27), interval: 1, startDate: start, endDate: nil)
        ))
        let first = try #require(try RecurringFixture.core(Self.template(store, salary).schedule, start, 1).first)

        let id = try #require(await store.changeRecurringKind(salary))

        let copy = try Self.template(store, id)
        #expect(copy.kind == .expense)
        #expect(copy.walletId == nil)
        #expect(copy.owner == "tester")
        #expect(copy.schedule == Schedule(frequency: .monthly(day: 27), interval: 1, startDate: first, endDate: nil))
    }

    @Test("A paused template's copy is paused too, and takes over the periods it never decided")
    func pausedStaysPaused() async throws {
        let store = try await Self.onboarded()
        let gym = try await Self.dailyExpense(store)
        await store.executeRecurring(gym.id, periodDate: try Self.day(-2))
        await store.setRecurringEnabled(gym.id, false)
        // Paused, nothing is due: yesterday and today wait for it to run.
        #expect(store.pendingRecurringItems.isEmpty)
        let before = try await Self.outbox(store)

        let id = try #require(await store.changeRecurringKind(gym.id))

        #expect(store.presentedError == nil)
        let copy = try Self.template(store, id)
        let yesterday = try Self.day(-1)
        #expect(!copy.enabled)
        #expect(copy.schedule.startDate == yesterday)
        #expect(try await Self.outbox(store) == before + 3)

        // Resumed, the copy has the two periods the template had left.
        await store.setRecurringEnabled(id, true)
        #expect(store.dueDates(of: id) == [yesterday, Self.today])
    }

    // MARK: - Refused

    @Test("Periods waiting keep the kind, and nothing is written")
    func refusedWhilePending() async throws {
        let store = try await Self.onboarded()
        let gym = try await Self.dailyExpense(store)
        #expect(await store.recurringKindRefusal(gym.id) == .pendingPeriods)
        let before = try await Self.outbox(store)

        #expect(await store.changeRecurringKind(gym.id) == nil)

        #expect(store.presentedError?.message == RecurringKindChange.Refusal.pendingPeriods.reason)
        #expect(!(try Self.template(store, gym.id).archived))
        #expect(store.recurringTemplates.count == 1)
        #expect(try await Self.outbox(store) == before)
        // One period left waiting is enough.
        store.presentedError = nil
        await store.skipRecurring(gym.id, periodDate: try Self.day(-2))
        await store.skipRecurring(gym.id, periodDate: try Self.day(-1))
        #expect(await store.recurringKindRefusal(gym.id) == .pendingPeriods)
    }

    @Test("An owner who left the vault keeps the kind")
    func refusedForAnOwnerWhoLeft() async throws {
        let store = try await Self.onboarded()
        let gym = try await Self.dailyExpense(store, owner: "elisa")
        await Self.decideAll(store, gym)
        store.setVaultMembers([try #require(store.currentVault).id: ["tester", "bob"]])

        #expect(await store.recurringKindRefusal(gym.id) == .ownerLeft)
        #expect(await store.changeRecurringKind(gym.id) == nil)
        #expect(!(try Self.template(store, gym.id).archived))
    }

    @Test("A vault only read, an archived template and a schedule with no period left keep the kind")
    func otherRefusals() async throws {
        let store = try await Self.onboarded()
        let gym = try await Self.dailyExpense(store, until: try Self.day(-1))
        await Self.decideAll(store, gym)
        #expect(await store.recurringKindRefusal(gym.id) == .noPeriodsLeft)

        let rent = try await Self.dailyExpense(store, note: "Rent")
        await store.archiveRecurring(rent.id)
        #expect(await store.recurringKindRefusal(rent.id) == .archived)

        store.setReadOnlyVaults([try #require(store.currentVault).id])
        #expect(await store.recurringKindRefusal(gym.id) == .readOnly)
        #expect(await store.changeRecurringKind(gym.id) == nil)
        #expect(store.presentedError?.code == "forbidden")
    }

    // MARK: - No wallet with several

    @Test("A template without a wallet warns once the vault has a second one, where the core refuses to record it")
    func lacksWalletWithSeveral() async throws {
        let store = try await Self.onboarded()
        let gym = try await Self.dailyExpense(store)
        #expect(!store.lacksWallet(gym))
        #expect(store.defaultWalletId == nil)

        await store.createWallet(name: "Contanti", openingBalance: 0)
        #expect(store.wallets.count == 2)
        #expect(store.lacksWallet(gym))

        await store.executeRecurring(gym.id, periodDate: try Self.day(-2))
        #expect(store.presentedError != nil)
        #expect(store.dueDates(of: gym.id).count == 3)
        store.presentedError = nil

        // A new template lands on the default wallet now, and so does a
        // duplicate of one without.
        let wallet = try #require(store.defaultWalletId)
        #expect(RecurringDraft(today: Self.today, walletId: store.defaultWalletId).walletId == wallet)
        #expect(RecurringDraft(duplicating: gym, today: Self.today, defaultWalletId: store.defaultWalletId).walletId == wallet)

        await store.updateRecurring(gym.id, patch: RecurringPatch(walletId: wallet))
        #expect(!store.lacksWallet(try Self.template(store, gym.id)))
        await store.executeRecurring(gym.id, periodDate: try Self.day(-2))
        #expect(store.presentedError == nil)
    }

    @Test("An archived template without a wallet does not warn")
    func archivedDoesNotWarn() async throws {
        let store = try await Self.onboarded()
        let gym = try await Self.dailyExpense(store)
        await store.createWallet(name: "Contanti", openingBalance: 0)
        await store.archiveRecurring(gym.id)
        #expect(!store.lacksWallet(try Self.template(store, gym.id)))
    }
}
