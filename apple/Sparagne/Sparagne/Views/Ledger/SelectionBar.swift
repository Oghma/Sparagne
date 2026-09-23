import SwiftUI
import SparagneCore

/// The line under the grid while two or more rows are selected: how many, and
/// what can be done to all of them at once. It takes the place of the key
/// hints, which are about typing a row, not about picking several.
///
/// The counts on the buttons are the rows the action will touch
/// (`AppStore.bulkTargets`), which can be fewer than the rows selected: a
/// voided row or a transfer stays selected but is passed over.
struct SelectionBar: View {
    @Bindable var store: AppStore
    /// "Set Category…", from this bar or from a selected row's context menu.
    @Binding var showsCategory: Bool

    var body: some View {
        let targets = store.bulkTargets.count
        HStack(spacing: 18) {
            Text(String(localized: "\(store.selection.count) rows selected"))
                .font(Face.footnote)
                .foregroundStyle(Ink.accent)
            action(String(localized: "Void \(targets) Rows")) {
                Task { await store.voidSelection() }
            }
            .disabled(targets == 0)
            action(String(localized: "Set Category\u{2026}")) { showsCategory = true }
                .disabled(targets == 0)
                .popover(isPresented: $showsCategory, arrowEdge: .top) {
                    BulkCategoryPopover(store: store, isPresented: $showsCategory)
                }
            Spacer()
            hint("\u{232B}", String(localized: "void"))
            hint("esc", String(localized: "clear"))
        }
        .padding(.horizontal, GridColumn.padding)
        .frame(height: 26)
        .background(Ink.bg)
    }

    private func action(_ title: String, perform: @escaping () -> Void) -> some View {
        Button(title, action: perform)
            .buttonStyle(.plain)
            .font(Face.footnote)
            .foregroundStyle(Ink.text)
    }

    private func hint(_ key: String, _ label: String) -> some View {
        HStack(spacing: 5) {
            Text(key).font(Face.footnote).foregroundStyle(Ink.text)
            Text(label).font(Face.footnote).foregroundStyle(Ink.dim)
        }
    }
}

/// "Set Category…": one field, the category every selected row moves to. Empty
/// is Uncategorized, which the placeholder says; a name nobody has used yet
/// becomes a new category, as it does in a cell.
struct BulkCategoryPopover: View {
    @Bindable var store: AppStore
    @Binding var isPresented: Bool

    @State private var name = ""
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionLabel(text: String(localized: "Category of \(store.bulkTargets.count) rows"))
            TextField(String(localized: "Uncategorized"), text: $name)
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .focused($focused)
                .padding(.horizontal, 8)
                .frame(height: 24)
                .background(Ink.bg)
                .overlay(Rectangle().strokeBorder(Ink.line, lineWidth: 1))
                .onSubmit(apply)
            HStack {
                Spacer()
                Button(String(localized: "Cancel"), role: .cancel) { isPresented = false }
                    .keyboardShortcut(.cancelAction)
                Button(String(localized: "Set Category"), action: apply)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(14)
        .frame(width: 280)
        .background(Ink.panel)
        .onAppear { focused = true }
    }

    private func apply() {
        let chosen = name
        isPresented = false
        Task { await store.setSelectionCategory(chosen) }
    }
}
