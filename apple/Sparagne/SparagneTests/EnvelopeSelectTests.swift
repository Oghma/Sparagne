import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The ENVELOPE cell of the grid, a select over the vault's envelopes: the
/// filter on its own, the model the cell drives, and the draft it writes.
struct EnvelopeSelectTests {
    private static func choice(_ name: String) -> EnvelopeChoice {
        EnvelopeChoice(id: "id-\(name)", name: name)
    }

    @Test("Blank text offers every envelope in the vault's order; text keeps those from its start, then those holding it")
    func filter() {
        let envelopes = ["Non allocato", "Casa", "Macchina", "Spesa", "Vacanze", "Cassa comune"].map { Self.choice($0) }

        #expect(EnvelopeChoiceModel.choices(for: "  ", among: envelopes) == envelopes)
        #expect(EnvelopeChoiceModel.choices(for: "ca", among: envelopes).map(\.name) == ["Casa", "Cassa comune", "Non allocato", "Vacanze"])
        #expect(EnvelopeChoiceModel.choices(for: "ferie", among: envelopes).isEmpty)
    }

    @Test("Case and accents never count")
    func folding() {
        let envelopes = [Self.choice("Città"), Self.choice("Spesa")]
        #expect(EnvelopeChoiceModel.choices(for: "CITTA", among: envelopes).map(\.name) == ["Città"])
    }

    @Test("Opened, the list highlights the envelope the row is on; arrows wrap; a pick writes its id and closes")
    func modelPicks() {
        let envelopes = ["Non allocato", "Casa", "Spesa"].map { Self.choice($0) }
        let model = EnvelopeChoiceModel()
        var written: Uuid?

        model.open(owner: "cell", envelopes: envelopes, current: "id-Spesa") { written = $0 }
        #expect(model.list.isOpen)
        #expect(model.list.owner == AnyHashable("cell"))
        #expect(model.list.items == envelopes)
        #expect(model.list.selected?.name == "Spesa")
        // Already the row's envelope: ↩ saves the row instead.
        #expect(!model.picksOnReturn)

        model.list.move(by: 1)
        #expect(model.list.selected?.name == "Non allocato")
        model.list.move(by: -1)
        #expect(model.list.selected?.name == "Spesa")
        model.list.move(by: -1)
        #expect(model.picksOnReturn)
        #expect(model.list.pick())
        #expect(written == "id-Casa")
        #expect(!model.list.isOpen)
        #expect(model.list.items.isEmpty)
    }

    @Test("Typing narrows the list and highlights the best match; nothing matching leaves nothing to pick")
    func modelFilters() {
        let envelopes = ["Non allocato", "Casa", "Spesa"].map { Self.choice($0) }
        let model = EnvelopeChoiceModel()
        var written: Uuid?
        model.open(owner: "cell", envelopes: envelopes, current: "id-Casa") { written = $0 }

        model.filter("sp")
        #expect(model.list.items.map(\.name) == ["Spesa"])
        #expect(model.list.selected?.name == "Spesa")

        model.filter("ferie")
        #expect(model.list.isOpen)
        #expect(model.list.items.isEmpty)
        #expect(model.query == "ferie")
        #expect(!model.picksOnReturn)
        #expect(!model.list.pick())
        #expect(written == nil)

        // Emptied again: all of them, back on the row's envelope.
        model.filter("")
        #expect(model.list.items == envelopes)
        #expect(model.list.selected?.name == "Casa")
    }

    @Test("A closed list ignores the text of a cell it is not open for")
    func closedIgnoresText() {
        let model = EnvelopeChoiceModel()
        model.filter("casa")
        #expect(!model.list.isOpen)
        #expect(model.list.items.isEmpty)
    }

    // MARK: - Over a store

    private static func household() async throws -> (store: AppStore, cash: FlowView) {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.envelope-select.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 100_000)
        await store.createEnvelope(name: "Cash", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        await store.createEnvelope(name: "Viaggi", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let cash = try #require(store.flows.first { $0.name == "Cash" })
        await store.addRow(day: Date(), flowId: cash.id, category: "Spesa", note: "coop", amount: 5_000)
        #expect(store.presentedError == nil)
        return (store, cash)
    }

    @Test("The list names Unallocated in the app's language and leaves archived envelopes out")
    func envelopesFromTheStore() async throws {
        let (store, _) = try await Self.household()
        let viaggi = try #require(store.flows.first { $0.name == "Viaggi" })
        await store.archiveEnvelope(viaggi.id)

        let names = EnvelopeChoiceModel.envelopes(in: store).map(\.name)
        #expect(names.first == NameBook.unallocatedLabel)
        #expect(names.contains("Cash"))
        #expect(!names.contains("Viaggi"))
    }

    @Test("The envelope picked is what the row is saved on, and only a change is sent")
    func draftCarriesThePick() async throws {
        let (store, cash) = try await Self.household()
        let viaggi = try #require(store.flows.first { $0.name == "Viaggi" })
        let row = try #require(store.rows.first { $0.note == "coop" })

        var draft = RowDraft(row: row, store: store)
        #expect(draft.flowId == cash.id)
        #expect(try draft.patch(against: row, store: store).flowId == nil)

        draft.flowId = viaggi.id
        let patch = try draft.patch(against: row, store: store)
        #expect(patch.flowId == viaggi.id)
        await store.update(transactionId: row.id, patch: patch)
        #expect(store.presentedError == nil)
        #expect(store.rows.first { $0.id == row.id }?.envelopeDisplay == "Viaggi")
    }

    @Test("The empty line starts on the sticky envelope and saves the one picked")
    func newLineCarriesThePick() async throws {
        let (store, cash) = try await Self.household()
        let viaggi = try #require(store.flows.first { $0.name == "Viaggi" })

        var draft = RowDraft.blank(in: store)
        #expect(draft.flowId == cash.id)
        draft.amount = "12"
        #expect(try draft.entry(store: store)?.flowId == cash.id)

        draft.flowId = viaggi.id
        #expect(try draft.entry(store: store)?.flowId == viaggi.id)

        // Nothing picked yet (the vault had not loaded): the sticky default.
        draft.flowId = nil
        #expect(try draft.entry(store: store)?.flowId == store.defaultFlowId)
    }

    @Test("A row on an archived envelope still saves its other cells, and stays on it")
    func archivedEnvelopeStays() async throws {
        let (store, _) = try await Self.household()
        // Only an envelope back to zero can be archived: a gift in, a gift out.
        let viaggi = try #require(store.flows.first { $0.name == "Viaggi" })
        await store.addRow(day: Date(), flowId: viaggi.id, category: "Regali", note: "busta", amount: 2_000, kind: .income)
        await store.addRow(day: Date(), flowId: viaggi.id, category: "Regali", note: "regalo", amount: 2_000)
        await store.archiveEnvelope(viaggi.id)
        #expect(store.presentedError == nil)
        #expect(!store.flows.contains { $0.id == viaggi.id })
        let archived = try #require(store.rows.first { $0.note == "regalo" })
        #expect(archived.envelopeDisplay == "Viaggi")

        var draft = RowDraft(row: archived, store: store)
        draft.note = "coop sabato"
        let patch = try draft.patch(against: archived, store: store)
        #expect(patch.note == "coop sabato")
        #expect(patch.flowId == nil)
    }

    @Test("A transfer between envelopes shows its route, opens with no envelope to pick and sends none")
    func transferLeavesTheLegs() async throws {
        let (store, _) = try await Self.household()
        await store.submit(quickAdd: "tf> 10.00 >cash >viaggi")
        store.showTransfers = true
        await store.settle()
        #expect(store.presentedError == nil)
        let transfer = try #require(store.rows.first { $0.kind == .transferFlow })
        // The column has room for the source; the route goes after the note.
        #expect(transfer.envelopeColumn == "Cash")
        #expect(transfer.transferRoute == "Cash \u{2192} Viaggi")
        #expect(store.rows.first { $0.note == "coop" }?.transferRoute == nil)

        var draft = RowDraft(row: transfer, store: store)
        #expect(draft.flowId == nil)
        draft.note = "giroconto"
        let patch = try draft.patch(against: transfer, store: store)
        #expect(patch.note == "giroconto")
        #expect(patch.flowId == nil)
    }
}
