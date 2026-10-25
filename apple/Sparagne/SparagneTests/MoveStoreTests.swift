import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// "Move between flows" and between wallets on the store
/// (`AppStore+Move.swift`), against the in-memory core: the lines go as
/// transfers in one batch, and a draft the check stops sends nothing. Every assertion reads the core's own
/// answer after the command.
struct MoveStoreTests {
    /// Vault `Casa`, wallet `Conto` opened with 1.000,00 in Unallocated,
    /// envelopes `Affitto` with no cap and `Vacanze` capped at 500,00 on its
    /// balance. Transfers shown, so the month's list has them.
    private static func vault() async throws -> AppStore {
        let store = try await TestStore.make()
        await store.createEnvelope(name: "Affitto", mode: .unlimited, allowNegative: false, openingAllocation: 0)
        await store.createEnvelope(name: "Vacanze", mode: .netCapped(cap: 50_000), allowNegative: false, openingAllocation: 0)
        store.showTransfers = true
        await store.reload()
        #expect(store.presentedError == nil)
        return store
    }

    private static func flow(_ store: AppStore, _ name: String) throws -> FlowView {
        try #require(store.flows.first { $0.name == name })
    }

    private static func unallocated(_ store: AppStore) throws -> FlowView {
        try #require(store.flows.first { $0.isUnallocated })
    }

    private static func draft(_ store: AppStore, _ lines: [(String, String)]) throws -> MoveDraft {
        var draft = MoveDraft(sourceId: try unallocated(store).id, day: CoreDate.day(Date()))
        for (name, amount) in lines {
            let id = try flow(store, name).id
            draft.add(id)
            draft.setAmount(amount, for: id)
        }
        return draft
    }

    private static func transfers(_ store: AppStore) -> [TransactionView] {
        store.transactions.filter { $0.kind == .transferFlow && !$0.voided }
    }

    @Test("Two lines go as two transfers out of Unallocated, with the note, on the day chosen")
    func moves() async throws {
        let store = try await Self.vault()
        var draft = try Self.draft(store, [("Affitto", "300"), ("Vacanze", "200,50")])
        draft.note = "riparto straordinario"
        let rent = try Self.flow(store, "Affitto").id
        let holiday = try Self.flow(store, "Vacanze").id
        let source = try Self.unallocated(store).id

        #expect(await store.move(draft))
        #expect(store.presentedError == nil)
        #expect(try Self.flow(store, "Affitto").balance == 30_000)
        #expect(try Self.flow(store, "Vacanze").balance == 20_050)
        #expect(try Self.unallocated(store).balance == 100_000 - 50_050)

        let transfers = Self.transfers(store)
        #expect(transfers.count == 2)
        #expect(Set(transfers.compactMap(\.toId)) == [rent, holiday])
        #expect(transfers.allSatisfy { $0.fromId == source })
        #expect(transfers.allSatisfy { $0.note == "Riparto straordinario" })
        #expect(transfers.allSatisfy { $0.occurredAt.hasPrefix(draft.day) })
        #expect(store.savedAt != nil)
    }

    @Test("A line over its cap sends nothing, not even the lines that fit")
    func overCapSendsNothing() async throws {
        let store = try await Self.vault()
        let draft = try Self.draft(store, [("Affitto", "300"), ("Vacanze", "600")])

        #expect(!(await store.move(draft)))
        // Stopped by the check: the core never saw it, so no alert.
        #expect(store.presentedError == nil)
        #expect(try Self.flow(store, "Affitto").balance == 0)
        #expect(try Self.flow(store, "Vacanze").balance == 0)
        #expect(try Self.unallocated(store).balance == 100_000)
        #expect(Self.transfers(store).isEmpty)
    }

    @Test("Checked again on the envelopes as they are now: room taken since the draft was made stops it")
    func checkedAgain() async throws {
        let store = try await Self.vault()
        let draft = try Self.draft(store, [("Vacanze", "400")])
        #expect(draft.check(targets: store.moveTargets(.flow), currency: store.currency).canMove)

        // Elsewhere, Vacanze gets 200,00 meanwhile: 400,00 more no longer fits.
        var first = try Self.draft(store, [("Vacanze", "200")])
        first.note = "prima"
        #expect(await store.move(first))

        #expect(!(await store.move(draft)))
        #expect(store.presentedError == nil)
        #expect(try Self.flow(store, "Vacanze").balance == 20_000)
        #expect(Self.transfers(store).count == 1)
    }

    @Test("A cap on the income counts what came in: the room the sheet shows is the core's")
    func incomeCap() async throws {
        let store = try await Self.vault()
        await store.createEnvelope(name: "Auto", mode: .incomeCapped(cap: 30_000), allowNegative: false, openingAllocation: 0)
        #expect(await store.move(try Self.draft(store, [("Auto", "200")])))

        let auto = try Self.flow(store, "Auto")
        #expect(auto.incomeTotal == 20_000)
        #expect(MoveCheck.headroom(MoveTarget(auto)) == 10_000)

        // What fits by the sheet's count goes through; one cent more does not
        // even leave it.
        let over = try Self.draft(store, [("Auto", "100,01")])
        #expect(over.check(targets: store.moveTargets(.flow), currency: store.currency).rows[0].issue == .overCap(excess: 1))
        #expect(!(await store.move(over)))
        #expect(await store.move(try Self.draft(store, [("Auto", "100")])))
        #expect(store.presentedError == nil)
        #expect(try Self.flow(store, "Auto").incomeTotal == 30_000)
    }

    @Test("A past day reads what the envelopes held then: money that came in later is not there to move")
    func pastDay() async throws {
        let store = try await Self.vault()
        #expect(await store.move(try Self.draft(store, [("Affitto", "300")])))
        let rent = try Self.flow(store, "Affitto").id
        let unallocated = try Self.unallocated(store).id
        let today = CoreDate.day(Date())
        let tenDaysAgo = CoreDate.day(Date().addingTimeInterval(-10 * 86_400))

        let then = try #require(await store.moveBalances(.flow, on: tenDaysAgo))
        // The opening balance and the 300,00 came today: then, nothing.
        #expect(then[unallocated]?.balance == 0)
        #expect(then[unallocated]?.low == 0)
        #expect(then[unallocated]?.lowOn == nil)
        #expect(then[rent]?.balance == 0)
        let now = try #require(await store.moveBalances(.flow, on: today))
        #expect(now[unallocated]?.balance == 70_000)
        #expect(now[rent]?.balance == 30_000)

        // Affitto may not go below zero, and ten days ago it held nothing.
        var back = MoveDraft(sourceId: rent, day: tenDaysAgo)
        back.add(unallocated)
        back.setAmount("100", for: unallocated)
        #expect(back.check(targets: store.moveTargets(.flow), currency: store.currency, balances: then).blocker == .sourceShort(shortfall: 10_000))
        #expect(!(await store.move(back)))
        #expect(store.presentedError == nil)
        #expect(try Self.flow(store, "Affitto").balance == 30_000)

        // Unallocated may: dated ten days ago it is only pointed out, and goes.
        var early = MoveDraft(sourceId: unallocated, day: tenDaysAgo)
        early.add(rent)
        early.setAmount("50", for: rent)
        #expect(early.check(targets: store.moveTargets(.flow), currency: store.currency, balances: then).sourceState == .belowZero(shortfall: 5_000))
        #expect(await store.move(early))
        #expect(try Self.flow(store, "Affitto").balance == 35_000)
    }

    @Test("Money moves back into Unallocated from another envelope too")
    func fromAnEnvelope() async throws {
        let store = try await Self.vault()
        #expect(await store.move(try Self.draft(store, [("Affitto", "300")])))

        var back = MoveDraft(sourceId: try Self.flow(store, "Affitto").id, day: CoreDate.day(Date()))
        let unallocated = try Self.unallocated(store).id
        back.add(unallocated)
        back.setAmount("100", for: unallocated)
        #expect(await store.move(back))
        #expect(try Self.flow(store, "Affitto").balance == 20_000)
        #expect(try Self.unallocated(store).balance == 80_000)
    }

    @Test("Between wallets the lines go as wallet transfers and the flows stay as they are")
    func betweenWallets() async throws {
        let store = try await Self.vault()
        await store.createWallet(name: "Contanti", openingBalance: 0)
        let conto = try #require(store.wallets.first { $0.name == "Conto" }).id
        let cash = try #require(store.wallets.first { $0.name == "Contanti" }).id
        let today = CoreDate.day(Date())
        var draft = MoveDraft(side: .wallet, sourceId: conto, day: today)
        draft.add(cash)
        draft.setAmount("250", for: cash)

        let balances = try #require(await store.moveBalances(.wallet, on: today))
        #expect(balances[conto]?.balance == 100_000)
        #expect(balances[cash]?.balance == 0)
        #expect(await store.move(draft))
        #expect(store.presentedError == nil)
        #expect(store.wallets.first { $0.id == conto }?.balance == 75_000)
        #expect(store.wallets.first { $0.id == cash }?.balance == 25_000)
        #expect(try Self.unallocated(store).balance == 100_000)
        #expect(Self.transfers(store).isEmpty)
        #expect(store.transactions.filter { $0.kind == .transferWallet && !$0.voided }.map(\.toId) == [cash])
    }
}
