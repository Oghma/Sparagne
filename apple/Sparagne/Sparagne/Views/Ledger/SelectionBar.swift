import SwiftUI
import SparagneCore

/// The line under the grid while two or more rows are selected: how many, and
/// what can be done to all of them at once. The status line beside the tabs
/// sums the same rows (`SheetStats`); this bar is for acting on them.
///
/// The counts on the buttons are the rows the action will touch
/// (`AppStore.bulkTargets`), which can be fewer than the rows selected: a
/// voided row or a transfer stays selected but is passed over.
struct SelectionBar: View {
    @Bindable var store: AppStore
    /// "Set Category…", from this bar or from a selected row's context menu.
    @Binding var showsCategory: Bool
    /// "Set Envelope…", the same way.
    @Binding var showsEnvelope: Bool

    var body: some View {
        let targets = store.bulkTargets.count
        HStack(spacing: 8) {
            Text(String(localized: "\(store.selection.count) rows selected"))
                .font(Face.ui(11.5, .medium))
                .foregroundStyle(Ink.accent)
                .padding(.trailing, 4)
            Button(String(localized: "Delete \(targets) Rows")) {
                Task { await store.voidSelection() }
            }
            .buttonStyle(RowButtonStyle())
            .disabled(targets == 0)
            Button(String(localized: "Set Category\u{2026}")) { showsCategory = true }
                .buttonStyle(RowButtonStyle())
                .disabled(targets == 0)
                .popover(isPresented: $showsCategory, arrowEdge: .top) {
                    BulkCategoryPopover(store: store, isPresented: $showsCategory)
                }
            Button(String(localized: "Set Envelope\u{2026}")) { showsEnvelope = true }
                .buttonStyle(RowButtonStyle())
                .disabled(targets == 0)
                .popover(isPresented: $showsEnvelope, arrowEdge: .top) {
                    BulkEnvelopePopover(store: store, isPresented: $showsEnvelope)
                }
            ChangeKindMenu(selectionOf: store, asButton: true)
            Spacer()
            KeyHint(key: "\u{232B}", label: String(localized: "delete"))
            KeyHint(key: "esc", label: String(localized: "clear"))
        }
        .padding(.horizontal, 12)
        .frame(height: 30)
        .background(Ink.bg)
        .overlay(alignment: .top) { Hairline() }
    }
}

/// "Cambia tipo ▸": the kinds in `targets`, each by its singular name
/// (`QuickAddTokens.kindName`), as a submenu of a context menu or as one of
/// the selection bar's buttons. Not there at all while there is nothing to
/// change into: a transfer, a deleted row, an opening, a vault only read.
struct ChangeKindMenu: View {
    let targets: [TransactionKind]
    var asButton = false
    let change: (TransactionKind) -> Void

    var body: some View {
        if !targets.isEmpty {
            if asButton {
                Menu {
                    items
                } label: {
                    HStack(spacing: 4) {
                        Text(title)
                        Image(systemName: "chevron.down")
                            .font(.system(size: 7, weight: .semibold))
                            .foregroundStyle(Ink.text3)
                    }
                    .rowButtonChrome()
                }
                .plainMenu()
                .fixedSize()
            } else {
                Menu(title) { items }
            }
        }
    }

    private var title: String { String(localized: "Change Kind") }

    private var items: some View {
        ForEach(targets, id: \.self) { kind in
            Button(QuickAddTokens.kindName(kind)) { change(kind) }
        }
    }
}

extension ChangeKindMenu {
    /// For the selection, from the bar or a selected row's menu: every row of
    /// it that can change, in one batch and one undo step; the others are
    /// passed over.
    init(selectionOf store: AppStore, asButton: Bool = false) {
        let ids = store.selectedRows.map(\.id)
        self.init(targets: store.kindTargets(rows: ids), asButton: asButton) { kind in
            Task { await store.changeKind(rows: ids, to: kind) }
        }
    }

    /// For the one row, from its menu, VoiceOver or ⌘K: `rowId` turned into
    /// `kind`. A cursor on it follows it to the new row, on the same cell,
    /// while that row is on screen, as an expense turned into a refund stays
    /// in USCITE; one that went to the other list leaves the cursor behind,
    /// and the toast offers to show it (`KindMoveToast`).
    static func change(_ rowId: Uuid, to kind: TransactionKind, in store: AppStore) async {
        let editor = store.ledgerEditor
        // Read before the write: the reload after it takes the cursor off
        // the row it voids.
        let cell = editor.cursor?.row == rowId ? editor.cursor : nil
        guard let id = await store.changeKind(of: rowId, to: kind), let cell,
              store.rows.contains(where: { $0.id == id })
        else { return }
        editor.select(CellFocus(row: id, field: cell.field))
    }
}

/// "Set Category…": one field, the category every selected row moves to, with
/// the same completion as a CATEGORY cell. Empty is Uncategorized, which the
/// placeholder says; a name nobody has used yet becomes a new category, as it
/// does in a cell.
///
/// ↩ and esc belong to the field: with the list open they pick and close it,
/// otherwise they apply and dismiss. The buttons carry no shortcut of their
/// own, which would take the keys before the field saw them.
struct BulkCategoryPopover: View {
    @Bindable var store: AppStore
    @Binding var isPresented: Bool

    @State private var name = ""
    @State private var completion = CategoryCompletionModel()
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionLabel(text: String(localized: "Category of \(store.bulkTargets.count) rows"))
            TextField(String(localized: "Uncategorized"), text: $name)
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .focused($focused)
                .completing(text: $name, owner: "bulk", isFocused: focused, store: store, completion: completion)
                .padding(.horizontal, 8)
                .frame(height: 24)
                .background(Ink.bg, in: RoundedRectangle(cornerRadius: 6))
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Ink.line2, lineWidth: 1))
                .onSubmit(apply)
            if completion.isOpen {
                CategoryCompletionList(completion: completion)
            }
            HStack {
                Spacer()
                Button(String(localized: "Cancel"), role: .cancel) { isPresented = false }
                Button(String(localized: "Set Category"), action: apply)
            }
        }
        .padding(14)
        .frame(width: FloatingList.width + 28)
        .background(Ink.card)
        .onAppear { focused = true }
        .task { await store.loadCategoryCompletion() }
    }

    private func apply() {
        let chosen = name
        isPresented = false
        Task { await store.setSelectionCategory(chosen) }
    }
}

/// "Set Envelope…": the ENVELOPE cell's list, for every selected row at once
/// (`EnvelopeChoiceModel`). Typing narrows it, ↑↓ move in it, ↩, ⇥ or a click
/// moves the rows to the highlighted envelope and dismisses; esc dismisses
/// and nothing moves. The envelope the rows share, if they all share one,
/// is ticked.
struct BulkEnvelopePopover: View {
    @Bindable var store: AppStore
    @Binding var isPresented: Bool

    @State private var query = ""
    @State private var choices = EnvelopeChoiceModel()
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionLabel(text: String(localized: "Envelope of \(store.bulkTargets.count) rows"))
            TextField(String(localized: "Envelope"), text: $query)
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .focused($focused)
                .padding(.horizontal, 8)
                .frame(height: 24)
                .background(Ink.bg, in: RoundedRectangle(cornerRadius: 6))
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Ink.line2, lineWidth: 1))
                .onKeyPress(.upArrow) {
                    choices.list.move(by: -1)
                    return .handled
                }
                .onKeyPress(.downArrow) {
                    choices.list.move(by: 1)
                    return .handled
                }
                .onKeyPress(.tab) {
                    choices.list.pick()
                    return .handled
                }
                .onSubmit { choices.list.pick() }
                .onChange(of: query) { _, new in choices.filter(new) }
            if choices.list.isOpen {
                EnvelopeChoiceList(choices: choices)
            }
            HStack {
                Spacer()
                Button(String(localized: "Cancel"), role: .cancel) { isPresented = false }
                Button(String(localized: "Set Envelope")) { choices.list.pick() }
                    .disabled(choices.list.selected == nil)
            }
        }
        .padding(14)
        .frame(width: FloatingList.width + 28)
        .background(Ink.card)
        .onAppear {
            choices.open(
                owner: "bulk",
                envelopes: EnvelopeChoiceModel.envelopes(in: store),
                current: store.bulkEnvelope,
                write: apply
            )
            focused = true
        }
    }

    private func apply(_ flowId: Uuid) {
        isPresented = false
        Task { await store.setSelectionEnvelope(flowId) }
    }
}
