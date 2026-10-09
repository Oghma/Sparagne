import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The Riparto tab's commands on the store (`AppStore+Allocation.swift`),
/// against the in-memory core: a plan made from its first line, the period
/// it puts due, Distribuisci, Salta and Annulla, and a vault only read.
/// Every assertion reads the core's own answer after the command.
struct AllocationStoreTests {
    /// Vault `Casa`, wallet `Conto` opened with 1.000,00 (in Unallocated,
    /// as an opening the plan never counts), envelopes `Affitto` with no cap
    /// and `Vacanze` capped at 500,00 on its balance.
    private static func vault() async throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.allocation.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        await store.createEnvelope(name: "Affitto", mode: .unlimited, allowNegative: false, openingAllocation: 0)
        await store.createEnvelope(name: "Vacanze", mode: .netCapped(cap: 50_000), allowNegative: false, openingAllocation: 0)
        #expect(store.presentedError == nil)
        return store
    }

    private static var today: NaiveDate { CoreDate.day(Date()) }

    private static func flow(_ store: AppStore, _ name: String) throws -> FlowView {
        try #require(store.flows.first { $0.name == name })
    }

    private static func unallocated(_ store: AppStore) throws -> Int64 {
        try #require(store.flows.first { $0.isUnallocated }).balance
    }

    /// An income into Unallocated, today.
    private static func income(_ store: AppStore, _ amount: Int64, note: String = "stipendio") async throws {
        let unallocated = try #require(store.flows.first { $0.isUnallocated })
        await store.addRow(
            day: Date(),
            flowId: unallocated.id,
            category: "Stipendio",
            note: note,
            amount: amount,
            kind: .income
        )
        #expect(store.presentedError == nil)
    }

    /// Affitto 800,00, then Vacanze up to its cap: monthly, from today, so
    /// today is the first period and is due.
    private static func plan(_ store: AppStore) async throws -> [AllocationLine] {
        let lines = [
            AllocationLine(flowId: try flow(store, "Affitto").id, rule: .fixed(amount: 80_000)),
            AllocationLine(flowId: try flow(store, "Vacanze").id, rule: .fillToCap),
        ]
        let saved = await store.saveAllocationLines(lines, schedule: AllocationScheduleDraft(today: today).schedule)
        #expect(saved)
        #expect(store.presentedError == nil)
        return lines
    }

    // MARK: - The plan

    @Test("The first line creates the plan, monthly from today; the next edits replace its list")
    func createAndEdit() async throws {
        let store = try await Self.vault()
        #expect(store.allocationPlan == nil)
        let lines = try await Self.plan(store)

        let plan = try #require(store.allocationPlan)
        #expect(plan.lines == lines)
        #expect(plan.enabled)
        #expect(plan.schedule == AllocationScheduleDraft(today: Self.today).schedule)
        #expect(plan.createdBy == "matteo")
        #expect(store.savedAt != nil)

        // An envelope twice is refused, and the plan stays as it was.
        let twice = lines + [AllocationLine(flowId: lines[0].flowId, rule: .percent(basisPoints: 1_250))]
        #expect(!(await store.saveAllocationLines(twice, schedule: plan.schedule)))
        #expect(store.presentedError != nil)
        store.presentedError = nil
        #expect(store.allocationPlan?.lines == lines)

        let moved = AllocationLines.moving(lines, at: 1, by: -1) ?? []
        #expect(await store.saveAllocationLines(moved, schedule: plan.schedule))
        #expect(store.presentedError == nil)
        #expect(store.allocationPlan?.lines == moved)
        #expect(store.allocationPlan?.id == plan.id)
    }

    @Test("The schedule and the Attivo switch go one field at a time, and a paused plan has nothing due")
    func scheduleAndSwitch() async throws {
        let store = try await Self.vault()
        _ = try await Self.plan(store)
        #expect(store.pendingAllocation != nil)
        #expect(store.actionableAllocationCount == 1)

        await store.setAllocationEnabled(false)
        #expect(store.presentedError == nil)
        #expect(store.allocationPlan?.enabled == false)
        #expect(store.pendingAllocation == nil)
        #expect(store.actionableAllocationCount == 0)

        await store.setAllocationEnabled(true)
        var draft = AllocationScheduleDraft(today: Self.today)
        draft.cadence = .weekly
        await store.setAllocationSchedule(draft.schedule)
        #expect(store.presentedError == nil)
        #expect(store.allocationPlan?.schedule == draft.schedule)
        #expect(store.allocationPlan?.enabled == true)
    }

    // MARK: - The period due

    @Test("A period due today shares out the incomes since the start, and the preview says what each line gets")
    func preview() async throws {
        let store = try await Self.vault()
        let lines = try await Self.plan(store)
        try await Self.income(store, 150_000)

        let pending = try #require(store.pendingAllocation)
        #expect(pending.planId == store.allocationPlan?.id)
        #expect(pending.periodDate == Self.today)
        #expect(pending.missed == 0)
        // The opening balance is not an income to share out.
        #expect(store.allocationBase.total == 150_000)
        #expect(store.allocationBase.incomes.map(\.note) == ["stipendio"])

        let preview = try #require(await store.previewAllocation(lines: lines, total: store.allocationBase.total))
        #expect(preview.lines.map(\.amount) == [80_000, 50_000])
        #expect(preview.lines.map(\.status) == [.full, .full])
        #expect(preview.lines[1].room == 50_000)
        #expect(preview.distributed == 130_000)
        #expect(preview.remainder == 20_000)
        #expect(preview.unallocatedAfter == 250_000 - 130_000)

        // A total lowered by hand: the last line runs short first.
        let short = try #require(await store.previewAllocation(lines: lines, total: 60_000))
        #expect(short.lines.map(\.amount) == [60_000, 0])
        #expect(short.lines.map(\.status) == [.short, .short])
    }

    @Test("Distribuisci moves what the plan gives on the total, closes the period and leaves the toast")
    func execute() async throws {
        let store = try await Self.vault()
        _ = try await Self.plan(store)
        try await Self.income(store, 150_000)

        await store.executeAllocation(total: store.allocationBase.total)

        #expect(store.presentedError == nil)
        #expect(try Self.flow(store, "Affitto").balance == 80_000)
        #expect(try Self.flow(store, "Vacanze").balance == 50_000)
        #expect(try Self.unallocated(store) == 250_000 - 130_000)
        #expect(store.pendingAllocation == nil)
        #expect(store.actionableAllocationCount == 0)
        #expect(store.allocationBase.total == 0)

        let run = try #require(store.allocationRuns.first)
        #expect(store.allocationRuns.count == 1)
        #expect(run.periodDate == Self.today)
        #expect(run.outcome == .executed)
        #expect(run.total == 150_000)
        #expect(run.moves.map(\.amount) == [80_000, 50_000])
        #expect(run.createdBy == "matteo")

        let undo = try #require(store.allocationUndo)
        #expect(undo.distributed == 130_000)
        #expect(undo.periodDate == Self.today)
    }

    @Test("Annulla puts the period back: its transfers deleted, its incomes back in the total")
    func reopen() async throws {
        let store = try await Self.vault()
        _ = try await Self.plan(store)
        try await Self.income(store, 150_000)
        await store.executeAllocation(total: 150_000)
        #expect(store.allocationUndo != nil)

        // From the toast.
        await store.undoAllocation()
        #expect(store.presentedError == nil)
        #expect(store.allocationUndo == nil)
        #expect(try Self.flow(store, "Affitto").balance == 0)
        #expect(try Self.flow(store, "Vacanze").balance == 0)
        #expect(try Self.unallocated(store) == 250_000)
        #expect(store.pendingAllocation?.periodDate == Self.today)
        #expect(store.allocationBase.total == 150_000)
        #expect(store.allocationRuns.isEmpty)

        // From the history, after sharing out again.
        await store.executeAllocation(total: 100_000)
        #expect(try Self.flow(store, "Affitto").balance == 80_000)
        #expect(try Self.flow(store, "Vacanze").balance == 20_000)
        await store.reopenAllocation(periodDate: Self.today)
        #expect(store.presentedError == nil)
        #expect(try Self.flow(store, "Affitto").balance == 0)
        #expect(store.pendingAllocation != nil)
        #expect(store.allocationUndo == nil)
    }

    @Test("Salta decides the period with nothing moved, and its incomes do not come back in the next total")
    func skip() async throws {
        let store = try await Self.vault()
        _ = try await Self.plan(store)
        try await Self.income(store, 150_000)

        await store.skipAllocation()

        #expect(store.presentedError == nil)
        #expect(store.pendingAllocation == nil)
        #expect(store.allocationBase.total == 0)
        #expect(try Self.flow(store, "Affitto").balance == 0)
        #expect(try Self.unallocated(store) == 250_000)
        #expect(store.allocationRuns.map(\.outcome) == [.skipped])
        #expect(store.allocationUndo == nil)

        // Annulla on a skipped period: due again, with its incomes.
        await store.reopenAllocation(periodDate: Self.today)
        #expect(store.pendingAllocation != nil)
        #expect(store.allocationBase.total == 150_000)
    }

    @Test("A period where no line gets anything is skipped rather than sent empty")
    func nothingToMoveSkips() async throws {
        let store = try await Self.vault()
        _ = try await Self.plan(store)
        #expect(store.pendingAllocation != nil)
        #expect(store.allocationBase.total == 0)

        await store.executeAllocation(total: 0)

        #expect(store.presentedError == nil)
        #expect(store.pendingAllocation == nil)
        #expect(store.allocationRuns.map(\.outcome) == [.skipped])
        #expect(store.allocationUndo == nil)
    }

    @Test("The history lists the decided periods once loaded, and empties with another vault")
    func history() async throws {
        let store = try await Self.vault()
        _ = try await Self.plan(store)
        await store.loadAllocationRuns()
        #expect(store.allocationRuns.isEmpty)
        await store.skipAllocation()
        #expect(store.allocationRuns.count == 1)

        await store.createVault(name: "Altro", walletName: "Conto", openingBalance: 0)
        #expect(store.currentVault?.name == "Altro")
        #expect(store.allocationPlan == nil)
        #expect(store.allocationRuns.isEmpty)
        await store.loadAllocationRuns()
        #expect(store.allocationRuns.isEmpty)
    }

    // MARK: - A vault only read

    @Test("A vault only read shows its period but refuses every allocation command before the core")
    func readOnly() async throws {
        let store = try await Self.vault()
        let lines = try await Self.plan(store)
        try await Self.income(store, 150_000)
        let vault = try #require(store.currentVault)
        store.setReadOnlyVaults([vault.id])
        #expect(store.pendingAllocation != nil)
        #expect(store.actionableAllocationCount == 0)

        #expect(!(await store.saveAllocationLines(Array(lines.reversed()), schedule: AllocationScheduleDraft(today: Self.today).schedule)))
        #expect(store.presentedError?.code == "forbidden")
        store.presentedError = nil
        #expect(store.allocationPlan?.lines == lines)

        await store.setAllocationEnabled(false)
        #expect(store.presentedError?.code == "forbidden")
        store.presentedError = nil

        await store.executeAllocation(total: 150_000)
        #expect(store.presentedError?.code == "forbidden")
        store.presentedError = nil
        await store.skipAllocation()
        #expect(store.presentedError?.code == "forbidden")
        store.presentedError = nil

        #expect(store.pendingAllocation != nil)
        #expect(store.allocationRuns.isEmpty)
        #expect(store.allocationUndo == nil)
        #expect(try Self.flow(store, "Affitto").balance == 0)
    }
}
