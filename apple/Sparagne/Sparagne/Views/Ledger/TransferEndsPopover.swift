import SparagneCore
import SwiftUI

/// The "Da → A" popover of a transfer in the Mastro: its two ends, wallets or envelopes by its kind, and ⇄ to swap them.
///
/// Two menus with ⇄ between them, the keyboard on one of them at a time:
/// ⇥ or ← → go from one to the other, ↑↓ pick in it, space swaps the two,
/// ↩ is OK and esc is Annulla. OK writes both ends at once
/// (`LedgerEditor.saveEnds`), and stays off while they are the same one or
/// the two the transfer has. What the core refuses, an envelope that cannot
/// go below zero, a cap, an archived end, comes back as the store's alert,
/// as a cell's refusal does.
struct TransferEndsPopover: View {
    let row: TransactionRow
    let store: AppStore
    let editor: LedgerEditor

    @State private var ends: TransferEnds?
    /// The menu the keyboard is on, ringed in the accent.
    @State private var side: TransferEnds.Side = .from
    @FocusState private var focused: Bool

    init(row: TransactionRow, store: AppStore, editor: LedgerEditor) {
        self.row = row
        self.store = store
        self.editor = editor
        _ends = State(initialValue: TransferEnds(row: row, store: store))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionLabel(text: title)
            if let ends {
                HStack(alignment: .bottom, spacing: 8) {
                    menu(.from, ends)
                    swapButton
                    menu(.to, ends)
                }
            }
            HStack {
                Spacer()
                Button(String(localized: "Cancel"), role: .cancel) { editor.closeEnds() }
                    .keyboardShortcut(.cancelAction)
                Button(String(localized: "OK"), action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!(ends?.canSave ?? false))
            }
            VStack(spacing: 6) {
                Hairline()
                HStack(spacing: 10) {
                    KeyHint(key: "\u{2191}\u{2193}", label: String(localized: "select"))
                    KeyHint(key: "\u{21E5}", label: String(localized: "From / To"))
                    KeyHint(key: String(localized: "space"), label: String(localized: "swap"))
                    Spacer(minLength: 0)
                }
            }
        }
        .padding(14)
        .background(Ink.card)
        // The keys of the menus are the popover's: neither a menu nor a
        // button takes the keyboard while Full Keyboard Access is off.
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(phases: [.down, .repeat]) { key($0) }
        .onAppear { focused = true }
    }

    private var title: String {
        row.kind == .transferWallet
            ? String(localized: "Transfer between wallets")
            : String(localized: "Transfer between envelopes")
    }

    /// Da or A: its label over a menu of every end on offer, the one it is on
    /// ticked.
    private func menu(_ side: TransferEnds.Side, _ ends: TransferEnds) -> some View {
        let label = side == .from ? String(localized: "From") : String(localized: "To")
        let value = ends.name(of: ends[side])
        return VStack(alignment: .leading, spacing: 4) {
            SectionLabel(text: label)
            Menu {
                Picker(label, selection: pick(side)) {
                    ForEach(ends.options) { option in
                        Text(option.name).tag(option.id)
                    }
                }
                .pickerStyle(.inline)
                .labelsHidden()
            } label: {
                FormBox(focused: focused && self.side == side) {
                    Text(value).lineLimit(1)
                    Spacer(minLength: 4)
                    Image(systemName: "chevron.down")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(Ink.text3)
                }
                .contentShape(Rectangle())
            }
            .plainMenu()
            .accessibilityLabel(label)
            .accessibilityValue(value)
        }
        .frame(width: 160)
    }

    /// A pick with the pointer also puts the keyboard on that menu.
    private func pick(_ side: TransferEnds.Side) -> Binding<Uuid> {
        Binding(
            get: { ends?[side] ?? "" },
            set: { id in
                ends?[side] = id
                self.side = side
            }
        )
    }

    private var swapButton: some View {
        Button {
            ends?.swap()
        } label: {
            Image(systemName: "arrow.left.arrow.right")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(Ink.text2)
                .frame(width: 24, height: 24)
                .background(Ink.card, in: RoundedRectangle(cornerRadius: 6))
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Ink.line2, lineWidth: 1))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(String(localized: "Swap From and To"))
        .accessibilityLabel(String(localized: "Swap From and To"))
    }

    private func key(_ press: KeyPress) -> KeyPress.Result {
        if press.key == .tab || press.key == GridKey.backTab {
            side = side.other
            return .handled
        }
        guard GridKey.isPlain(press) else { return .ignored }
        switch press.key {
        case .upArrow: ends?.step(side, by: -1)
        case .downArrow: ends?.step(side, by: 1)
        case .leftArrow: side = .from
        case .rightArrow: side = .to
        case .space: ends?.swap()
        default: return .ignored
        }
        return .handled
    }

    private func save() {
        guard let ends, ends.canSave else { return }
        editor.saveEnds(of: row.id, from: ends.from, to: ends.to)
    }
}

/// The ends picked in the popover, against the two the transfer has: what OK
/// writes, and whether it may.
struct TransferEnds: Equatable {
    /// An end on offer, named as the grid names it.
    struct Option: Identifiable, Equatable {
        let id: Uuid
        let name: String
    }

    /// The menu the keyboard is on.
    enum Side: Equatable {
        case from, to

        var other: Side { self == .from ? .to : .from }
    }

    /// In the order the menus list them.
    let options: [Option]
    /// The ends the transfer has now.
    let storedFrom: Uuid
    let storedTo: Uuid
    var from: Uuid
    var to: Uuid

    init(options: [Option], from: Uuid, to: Uuid) {
        self.options = options
        storedFrom = from
        storedTo = to
        self.from = from
        self.to = to
    }

    /// Two ends, and not the two the transfer has: anything else would be
    /// refused by the core, or write nothing.
    var canSave: Bool { from != to && (from != storedFrom || to != storedTo) }

    subscript(side: Side) -> Uuid {
        get { side == .from ? from : to }
        set {
            if side == .from { from = newValue } else { to = newValue }
        }
    }

    /// ⇄.
    mutating func swap() {
        (from, to) = (to, from)
    }

    /// ↑ is `step(side, by: -1)`, ↓ is `step(side, by: 1)`: the end before or
    /// after in the menu, round from the last to the first as the lists'
    /// arrows go. The other end is not skipped: OK says it is the same one.
    mutating func step(_ side: Side, by delta: Int) {
        guard !options.isEmpty else { return }
        let index = options.firstIndex { $0.id == self[side] } ?? 0
        self[side] = options[FloatingList.wrapped(index, by: delta, count: options.count)].id
    }

    func name(of id: Uuid) -> String {
        options.first { $0.id == id }?.name ?? TransactionRow.placeholder
    }
}

extension TransferEnds {
    /// `row`'s ends among the active envelopes, Unallocated first, for a
    /// transfer between envelopes, or the active wallets for one between
    /// wallets. An end archived since stays on offer at the bottom, so its
    /// menu can still say it; the core refuses a write with it. `nil` for an
    /// entry.
    init?(row: TransactionRow, store: AppStore) {
        guard let from = row.fromId, let to = row.toId else { return nil }
        var options: [Option]
        let name: (Uuid) -> String
        switch row.kind {
        case .transferFlow:
            options = store.flows.map { Option(id: $0.id, name: store.flowName($0)) }
            name = { store.envelopeName($0) }
        case .transferWallet:
            options = store.wallets.map { Option(id: $0.id, name: $0.name) }
            name = { store.walletName($0) }
        case .income, .expense, .refund:
            return nil
        }
        for id in [from, to] where !options.contains(where: { $0.id == id }) {
            options.append(Option(id: id, name: name(id)))
        }
        self.init(options: options, from: from, to: to)
    }
}

extension View {
    /// Hangs `row`'s "Da → A" popover from this cell while `isPresented`, the
    /// editor having it open (`LedgerEditor.endsRowId`). Closed from outside,
    /// by a click away, it closes the editor's too, unless that has gone on
    /// to another row meanwhile.
    func transferEndsPopover(
        for row: TransactionRow,
        store: AppStore,
        editor: LedgerEditor,
        isPresented: Bool
    ) -> some View {
        popover(
            isPresented: Binding(
                get: { isPresented },
                set: { shown in
                    if !shown, editor.endsRowId == row.id { editor.closeEnds() }
                }
            ),
            arrowEdge: .bottom
        ) {
            TransferEndsPopover(row: row, store: store, editor: editor)
        }
    }
}
