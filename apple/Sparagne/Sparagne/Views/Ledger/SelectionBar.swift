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
