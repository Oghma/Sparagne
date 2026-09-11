import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The draft behind the setup tab's envelope table (`docs/v2/UI.md` §2.3):
/// what an edited row sends back, what the empty line creates, and which cell
/// a refused amount points at. The draft is a plain struct, so none of this
/// needs a view.
@MainActor
struct EnvelopeTableTests {
    private static func makeStore() throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.envelopes.\(UUID().uuidString)"))
        return AppStore(client: try CoreClient.inMemory(author: "matteo"), defaults: defaults)
    }

    /// A vault whose Unallocated holds the wallet's opening balance, which is
    /// what a new envelope's allocation is moved out of.
    private static func vault() throws -> AppStore {
        let store = try makeStore()
        store.bootstrap()
        store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        #expect(store.presentedError == nil)
        return store
    }

    /// An envelope as the store would hand it to the table.
    private static func flow(
        name: String = "Casa",
        mode: FlowMode = .unlimited,
        allowNegative: Bool = false
    ) -> FlowView {
        FlowView(
            id: UUID().uuidString,
            name: name,
            balance: 0,
            mode: mode,
            incomeTotal: nil,
            allowNegative: allowNegative,
            archived: false,
            isUnallocated: false
        )
    }

    // MARK: - The diff of an edited row

    @Test("A row nobody touched sends nothing")
    func unchangedDraftIsEmpty() throws {
        let flow = Self.flow(mode: .netCapped(cap: 150_000_00), allowNegative: true)
        let patch = try EnvelopeDraft(flow: flow).patch(against: flow, currency: .eur)
        #expect(patch.isEmpty)
        #expect(patch == EnvelopePatch())
    }

    @Test("A renamed row sends the name and only the name")
    func nameOnly() throws {
        let flow = Self.flow(name: "Casa")
        var draft = EnvelopeDraft(flow: flow)
        draft.name = "  Casa nuova  "
        let patch = try draft.patch(against: flow, currency: .eur)
        #expect(patch.name == "Casa nuova")
        #expect(patch.mode == nil)
        #expect(patch.allowNegative == nil)
    }

    @Test("Changing the kind of cap keeps the amount and sends only the mode")
    func capKind() throws {
        let flow = Self.flow(mode: .netCapped(cap: 30_000_00))
        var draft = EnvelopeDraft(flow: flow)
        #expect(draft.kind == .net)
        #expect(draft.cap == "30000.00")

        draft.kind = .income
        let patch = try draft.patch(against: flow, currency: .eur)
        #expect(patch.mode == .incomeCapped(cap: 30_000_00))
        #expect(patch.name == nil)
        #expect(patch.allowNegative == nil)
    }

    @Test("A new cap amount travels as the mode, since the cap lives inside it")
    func capAmount() throws {
        let flow = Self.flow(mode: .netCapped(cap: 30_000_00))
        var draft = EnvelopeDraft(flow: flow)
        draft.cap = "35000,50"
        let patch = try draft.patch(against: flow, currency: .eur)
        #expect(patch.mode == .netCapped(cap: 35_000_50))
        #expect(patch.name == nil)
    }

    @Test("Toggling NEG sends allowNegative and nothing else")
    func negative() throws {
        let flow = Self.flow(allowNegative: false)
        var draft = EnvelopeDraft(flow: flow)
        draft.allowNegative = true
        let patch = try draft.patch(against: flow, currency: .eur)
        #expect(patch.allowNegative == true)
        #expect(patch.name == nil)
        #expect(patch.mode == nil)
    }

    @Test("An emptied name leaves the name alone, as an emptied amount does in the ledger")
    func emptyNameIsNotAChange() throws {
        let flow = Self.flow(name: "Casa")
        var draft = EnvelopeDraft(flow: flow)
        draft.name = "   "
        #expect(try draft.patch(against: flow, currency: .eur).isEmpty)
    }

    // MARK: - Refused amounts point at the cell

    @Test("A cap that does not parse is reported on the CAP cell")
    func badCapAmount() throws {
        let flow = Self.flow(mode: .netCapped(cap: 30_000_00))
        var draft = EnvelopeDraft(flow: flow)
        draft.cap = "abc"
        do {
            _ = try draft.patch(against: flow, currency: .eur)
            Issue.record("a cap of \"abc\" should not have parsed")
        } catch let failure as EnvelopeDraftError {
            #expect(failure.field == .cap)
        }
    }

    @Test("A capped kind with no amount is reported on the CAP cell too")
    func missingCapAmount() throws {
        let flow = Self.flow(mode: .netCapped(cap: 30_000_00))
        var draft = EnvelopeDraft(flow: flow)
        draft.cap = ""
        do {
            _ = try draft.patch(against: flow, currency: .eur)
            Issue.record("a net cap with no amount should not have resolved")
        } catch let failure as EnvelopeDraftError {
            #expect(failure.field == .cap)
        }

        // The same draft with no cap at all is fine: it means "no cap".
        draft.kind = .none
        #expect(try draft.patch(against: flow, currency: .eur).mode == .unlimited)
    }

    @Test("A bad opening allocation is reported on the SALDO cell of the empty line")
    func badAllocation() throws {
        var line = EnvelopeDraft()
        line.name = "Emergenza"
        line.allocation = "10,000"
        do {
            _ = try line.envelope(currency: .eur)
            Issue.record("three decimals should not have parsed")
        } catch let failure as EnvelopeDraftError {
            #expect(failure.field == .allocation)
        }
    }

    // MARK: - The empty line

    @Test("An empty name is nothing to save, not an error")
    func blankLineCreatesNothing() throws {
        #expect(try EnvelopeDraft().envelope(currency: .eur) == nil)
    }

    @Test("The empty line parses its kind, its cap and its allocation, and creates the envelope")
    func newLineCreates() throws {
        let store = try Self.vault()
        var line = EnvelopeDraft()
        line.name = "  Emergenza  "
        line.kind = .income
        line.cap = "30000.00"
        line.allowNegative = true
        line.allocation = "250,00"

        let entry = try #require(try line.envelope(currency: store.currency))
        #expect(entry.name == "Emergenza")
        #expect(entry.mode == .incomeCapped(cap: 30_000_00))
        #expect(entry.allowNegative)
        #expect(entry.allocation == 25_000)

        store.createEnvelope(
            name: entry.name,
            mode: entry.mode,
            allowNegative: entry.allowNegative,
            openingAllocation: entry.allocation
        )
        #expect(store.presentedError == nil)

        let created = try #require(store.flows.first { $0.name == "Emergenza" })
        #expect(created.mode == .incomeCapped(cap: 30_000_00))
        #expect(created.allowNegative)
        #expect(created.balance == 25_000)
        // The allocation came out of Unallocated, not out of thin air.
        let unallocated = try #require(store.flows.first { $0.isUnallocated })
        #expect(unallocated.balance == 100_000 - 25_000)
    }

    // MARK: - Through the store

    @Test("The diff reaches the core as an update of the changed fields only")
    func patchReachesTheStore() throws {
        let store = try Self.vault()
        store.createEnvelope(name: "Casa", mode: .netCapped(cap: 150_000_00), allowNegative: false, openingAllocation: 0)
        let flow = try #require(store.flows.first { $0.name == "Casa" })

        var draft = EnvelopeDraft(flow: flow)
        draft.name = "Casa nuova"
        draft.allowNegative = true
        let patch = try draft.patch(against: flow, currency: store.currency)
        #expect(patch.mode == nil)

        store.updateEnvelope(flow.id, name: patch.name, mode: patch.mode, allowNegative: patch.allowNegative)
        #expect(store.presentedError == nil)

        let updated = try #require(store.flows.first { $0.id == flow.id })
        #expect(updated.name == "Casa nuova")
        #expect(updated.allowNegative)
        // The mode was not in the diff, so the cap survived the rename.
        #expect(updated.mode == .netCapped(cap: 150_000_00))
    }

    @Test("The table's three kinds describe every mode, and only those")
    func kinds() {
        #expect(EnvelopeCapKind(.unlimited) == .none)
        #expect(EnvelopeCapKind(.netCapped(cap: 1)) == .net)
        #expect(EnvelopeCapKind(.incomeCapped(cap: 1)) == .income)
        #expect(EnvelopeCapKind.cap(of: .unlimited) == nil)
        #expect(EnvelopeCapKind.cap(of: .incomeCapped(cap: 42)) == 42)
        #expect(EnvelopeCapKind.none.mode(cap: 42) == .unlimited)
        #expect(!EnvelopeCapKind.none.isCapped)
    }
}
