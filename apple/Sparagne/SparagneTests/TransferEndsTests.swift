import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The ends of a transfer's "Da → A" popover (`TransferEnds`): what its two
/// menus offer, what ⇄ and the arrows do to them, and when OK may write.
struct TransferEndsTests {
    private static let options = ["a", "b", "c"].map { TransferEnds.Option(id: $0, name: $0.uppercased()) }

    /// Envelopes Cash and Viaggi, and a transfer of 10,00 from one to the
    /// other.
    private static func envelopeTransfer() async throws -> (store: AppStore, transfer: TransactionRow) {
        let store = try await TestStore.make()
        await store.createEnvelope(name: "Cash", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        await store.createEnvelope(name: "Viaggi", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        await store.submit(quickAdd: "tf> 10.00 >cash >viaggi")
        store.showTransfers = true
        await store.settle()
        #expect(store.presentedError == nil)
        return (store, try #require(store.rows.first { $0.kind == .transferFlow }))
    }

    @Test("OK is off until an end changes, and while the two are one")
    func canSave() {
        var ends = TransferEnds(options: Self.options, from: "a", to: "b")
        #expect(!ends.canSave)
        ends.to = "c"
        #expect(ends.canSave)
        ends.from = "c"
        #expect(!ends.canSave)
    }

    @Test("⇄ swaps the two ends, and swapped back they are the transfer's again")
    func swap() {
        var ends = TransferEnds(options: Self.options, from: "a", to: "b")
        ends.swap()
        #expect(ends.from == "b")
        #expect(ends.to == "a")
        #expect(ends.canSave)
        ends.swap()
        #expect(!ends.canSave)
    }

    @Test("↑↓ walk one menu round, past the other end too, and leave the other menu alone")
    func step() {
        var ends = TransferEnds(options: Self.options, from: "a", to: "b")
        ends.step(.from, by: 1)
        #expect(ends.from == "b")
        #expect(!ends.canSave)
        ends.step(.from, by: 1)
        #expect(ends.from == "c")
        ends.step(.from, by: 1)
        #expect(ends.from == "a")

        ends.step(.to, by: -1)
        ends.step(.to, by: -1)
        #expect(ends.to == "c")
        #expect(ends.from == "a")
        #expect(ends[.to] == "c")
    }

    @Test("Between envelopes the menus hold the active ones, Unallocated first, the transfer's ends picked")
    func envelopeOptions() async throws {
        let (store, transfer) = try await Self.envelopeTransfer()
        let ends = try #require(TransferEnds(row: transfer, store: store))
        #expect(ends.options.map(\.name) == [NameBook.unallocatedLabel, "Cash", "Viaggi"])
        #expect(ends.name(of: ends.from) == "Cash")
        #expect(ends.name(of: ends.to) == "Viaggi")
        #expect(!ends.canSave)
    }

    @Test("An end archived since stays at the bottom of the menus, so they can still name it")
    func archivedEndStays() async throws {
        let (store, _) = try await Self.envelopeTransfer()
        // Only an envelope back to zero can be archived: the 10,00 that went
        // out of Cash comes back in.
        let cash = try #require(store.flows.first { $0.name == "Cash" })
        await store.addRow(day: Date(), flowId: cash.id, category: "Regali", note: "busta", amount: 1_000, kind: .income)
        await store.archiveEnvelope(cash.id)
        #expect(store.presentedError == nil)
        #expect(!store.flows.contains { $0.id == cash.id })
        let transfer = try #require(store.rows.first { $0.kind == .transferFlow })

        let ends = try #require(TransferEnds(row: transfer, store: store))
        #expect(ends.options.map(\.name) == [NameBook.unallocatedLabel, "Viaggi", "Cash"])
        #expect(ends.from == cash.id)
        #expect(ends.name(of: ends.from) == "Cash")
    }

    @Test("Between wallets the menus hold the active wallets, in the vault's order")
    func walletOptions() async throws {
        let store = try await TestStore.make()
        await store.createWallet(name: "Contanti", openingBalance: 0)
        await store.submit(quickAdd: "tw> 10.00 @conto @contanti")
        store.showTransfers = true
        await store.settle()
        #expect(store.presentedError == nil)
        let transfer = try #require(store.rows.first { $0.kind == .transferWallet })

        let ends = try #require(TransferEnds(row: transfer, store: store))
        #expect(ends.options.map(\.name) == ["Contanti", "Conto"])
        #expect(ends.name(of: ends.from) == "Conto")
        #expect(ends.name(of: ends.to) == "Contanti")
    }

    @Test("An entry has no ends to pick")
    func entryHasNone() async throws {
        let store = try await TestStore.make()
        #expect(TransferEnds(row: TestRow.make(kind: .expense), store: store) == nil)
    }
}
