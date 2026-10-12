import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// "Move between envelopes" on the store (`AppStore+EnvelopeMove.swift`),
/// against the in-memory core: the lines go as transfers in one batch, and a
/// draft the check stops sends nothing. Every assertion reads the core's own
/// answer after the command.
struct EnvelopeMoveStoreTests {
    /// Vault `Casa`, wallet `Conto` opened with 1.000,00 in Unallocated,
    /// envelopes `Affitto` with no cap and `Vacanze` capped at 500,00 on its
    /// balance. Transfers shown, so the month's list has them.
    private static func vault() async throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.move.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
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

    private static func draft(_ store: AppStore, _ lines: [(String, String)]) throws -> EnvelopeMoveDraft {
        var draft = EnvelopeMoveDraft(sourceId: try unallocated(store).id, day: CoreDate.day(Date()))
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

        #expect(await store.moveBetweenEnvelopes(draft))
        #expect(store.presentedError == nil)
        #expect(try Self.flow(store, "Affitto").balance == 30_000)
        #expect(try Self.flow(store, "Vacanze").balance == 20_050)
        #expect(try Self.unallocated(store).balance == 100_000 - 50_050)

        let transfers = Self.transfers(store)
        #expect(transfers.count == 2)
        #expect(Set(transfers.compactMap(\.toId)) == [rent, holiday])
        #expect(transfers.allSatisfy { $0.fromId == source })
        #expect(transfers.allSatisfy { $0.note == "riparto straordinario" })
        #expect(transfers.allSatisfy { $0.occurredAt.hasPrefix(draft.day) })
        #expect(store.savedAt != nil)
    }

    @Test("A line over its cap sends nothing, not even the lines that fit")
    func overCapSendsNothing() async throws {
        let store = try await Self.vault()
        let draft = try Self.draft(store, [("Affitto", "300"), ("Vacanze", "600")])

        #expect(!(await store.moveBetweenEnvelopes(draft)))
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
        #expect(draft.check(flows: store.flows, currency: store.currency).canMove)

        // Elsewhere, Vacanze gets 200,00 meanwhile: 400,00 more no longer fits.
        var first = try Self.draft(store, [("Vacanze", "200")])
        first.note = "prima"
        #expect(await store.moveBetweenEnvelopes(first))

        #expect(!(await store.moveBetweenEnvelopes(draft)))
        #expect(store.presentedError == nil)
        #expect(try Self.flow(store, "Vacanze").balance == 20_000)
        #expect(Self.transfers(store).count == 1)
    }

    @Test("A cap on the income counts what came in: the room the sheet shows is the core's")
    func incomeCap() async throws {
        let store = try await Self.vault()
        await store.createEnvelope(name: "Auto", mode: .incomeCapped(cap: 30_000), allowNegative: false, openingAllocation: 0)
        #expect(await store.moveBetweenEnvelopes(try Self.draft(store, [("Auto", "200")])))

        let auto = try Self.flow(store, "Auto")
        #expect(auto.incomeTotal == 20_000)
        #expect(EnvelopeMoveCheck.headroom(auto) == 10_000)

        // What fits by the sheet's count goes through; one cent more does not
        // even leave it.
        let over = try Self.draft(store, [("Auto", "100,01")])
        #expect(over.check(flows: store.flows, currency: store.currency).rows[0].issue == .overCap(excess: 1))
        #expect(!(await store.moveBetweenEnvelopes(over)))
        #expect(await store.moveBetweenEnvelopes(try Self.draft(store, [("Auto", "100")])))
        #expect(store.presentedError == nil)
        #expect(try Self.flow(store, "Auto").incomeTotal == 30_000)
    }

    @Test("Money moves back into Unallocated from another envelope too")
    func fromAnEnvelope() async throws {
        let store = try await Self.vault()
        #expect(await store.moveBetweenEnvelopes(try Self.draft(store, [("Affitto", "300")])))

        var back = EnvelopeMoveDraft(sourceId: try Self.flow(store, "Affitto").id, day: CoreDate.day(Date()))
        let unallocated = try Self.unallocated(store).id
        back.add(unallocated)
        back.setAmount("100", for: unallocated)
        #expect(await store.moveBetweenEnvelopes(back))
        #expect(try Self.flow(store, "Affitto").balance == 20_000)
        #expect(try Self.unallocated(store).balance == 80_000)
    }
}
