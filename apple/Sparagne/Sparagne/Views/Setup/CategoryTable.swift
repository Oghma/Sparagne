import SwiftUI
import SparagneCore

/// Column geometry, fixed so the header, the rows and the empty line agree
/// without a layout pass (`docs/v2/UI.md` §2.3 and §5). ALIASES takes what is
/// left: the table shares the window with the envelopes one, so at the
/// minimum width it still has room for a couple of names.
private enum CategoryColumn {
    static let name: CGFloat = 160
    /// Just wide enough for the SYSTEM badge, which only Opening and
    /// Uncategorized carry.
    static let badge: CGFloat = 72
}

/// The cells a category row is typed into.
///
/// Nothing here drives the traversal: ⇥ walks the focusable views in layout
/// order, and the order of the cases is only the order the cells are drawn in.
enum CategoryField: Hashable {
    case name
    case aliases
}

/// A cell: which row (nil = the empty line at the bottom) and which field.
private struct CategoryCellFocus: Hashable {
    let row: Uuid?
    let field: CategoryField
}

/// The categories of the vault as an editable table (`docs/v2/UI.md` §2.3):
/// the last line adds one, names and aliases edit in place.
///
/// Editing is per row, like the ledger's grid: clicking a cell opens the whole
/// row with the caret there, ↩ commits what changed, esc throws the draft away
/// and leaving the row commits it the way a spreadsheet does. A refusal from
/// the core keeps the row open on the cell that caused it, so the text can be
/// fixed instead of retyped.
struct CategoryTable: View {
    @Bindable var store: AppStore

    /// Which row is open for editing; nil means none, the empty line has no
    /// draft of its own beyond `newName`.
    @State private var editing: Uuid?
    @State private var draft = CategoryDraft()
    @State private var newName = ""
    @State private var hovered: Uuid?
    @State private var merging: MergeSubject?
    @FocusState private var focus: CategoryCellFocus?

    var body: some View {
        Panel(padding: 0) {
            VStack(spacing: 0) {
                title
                Hairline()
                header
                Hairline()
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(active, id: \.id) { category in
                            row(category)
                            Hairline()
                        }
                        if !archived.isEmpty {
                            archivedLabel
                            Hairline()
                            ForEach(archived, id: \.id) { category in
                                row(category)
                                Hairline()
                            }
                        }
                        newLine
                    }
                }
                .scrollBounceBehavior(.basedOnSize)
            }
        }
        // A vault switch reloads the lists under whatever was open.
        .onChange(of: store.currentVault?.id) { _, _ in
            editing = nil
            newName = ""
            focus = nil
        }
        .onChange(of: focus) { _, new in commitIfLeft(new) }
        .sheet(item: $merging) { subject in
            MergeCategorySheet(
                store: store,
                source: subject.category,
                targets: active.filter { $0.id != subject.category.id }
            )
        }
    }

    // MARK: - Chrome

    private var title: some View {
        HStack {
            SectionLabel(text: String(localized: "Categories"))
            Spacer()
        }
        .padding(.horizontal, GridColumn.padding)
        .frame(height: 24)
    }

    private var header: some View {
        HStack(spacing: 0) {
            GridCell(width: CategoryColumn.name) { SectionLabel(text: String(localized: "Name")) }
            GridCell { SectionLabel(text: String(localized: "Aliases")) }
            GridCell(width: CategoryColumn.badge) { Text("") }
        }
        .frame(height: 24)
    }

    private var archivedLabel: some View {
        HStack {
            SectionLabel(text: String(localized: "Archived"))
            Spacer()
        }
        .padding(.horizontal, GridColumn.padding)
        .frame(height: 22)
    }

    // MARK: - Rows

    private var active: [CategoryView] { store.windowCategories.filter { !$0.archived } }
    private var archived: [CategoryView] { store.windowCategories.filter { $0.archived } }

    /// System categories are read-only: the core refuses to rename, archive or
    /// merge them, so they carry no menu at all rather than an empty one.
    @ViewBuilder
    private func row(_ category: CategoryView) -> some View {
        let line = line(category)
        if category.isSystem {
            line
        } else if category.archived {
            line.contextMenu {
                Button(String(localized: "Restore")) { store.restoreCategory(category.id) }
            }
        } else {
            line.contextMenu {
                Button(String(localized: "Merge Into…")) { merging = MergeSubject(category: category) }
                Divider()
                Button(String(localized: "Archive"), role: .destructive) { store.archiveCategory(category.id) }
            }
        }
    }

    @ViewBuilder
    private func line(_ category: CategoryView) -> some View {
        let isEditing = editing == category.id
        CategoryRowView(
            category: category,
            aliases: aliases(of: category.id),
            draft: isEditing ? $draft : nil,
            focus: $focus,
            onOpen: { field in open(category, at: field) },
            onCommit: { submit(category.id) },
            onCancel: cancel
        )
        .background(isEditing || hovered == category.id ? Ink.raised : Color.clear)
        .overlay(alignment: .leading) {
            if isEditing {
                Rectangle().fill(Ink.accent).frame(width: 2)
            }
        }
        .onHover { inside in
            if inside {
                hovered = category.id
            } else if hovered == category.id {
                hovered = nil
            }
        }
    }

    /// The empty last line: a name, and under ALIASES the near names the core
    /// knows, which suggest and never block (`docs/v2/DISTILLATO_V1.md` §2.1).
    private var newLine: some View {
        HStack(spacing: 0) {
            GridCell(width: CategoryColumn.name) {
                TextField(String(localized: "name…"), text: $newName)
                    .textFieldStyle(.plain)
                    .font(Face.row)
                    .foregroundStyle(Ink.text)
                    .focused($focus, equals: CategoryCellFocus(row: nil, field: .name))
                    .onSubmit(commitNewLine)
            }
            GridCell {
                Text(similarHint)
                    .font(Face.row)
                    .foregroundStyle(Ink.dim)
            }
            GridCell(width: CategoryColumn.badge) { Text("") }
        }
        .frame(height: Metrics.rowHeight)
        .background(focus == CategoryCellFocus(row: nil, field: .name) ? Ink.raised : Color.clear)
        .overlay(alignment: .leading) {
            Rectangle().fill(Ink.accent).frame(width: 2)
        }
        .onKeyPress(.escape) {
            newName = ""
            focus = nil
            return .handled
        }
    }

    private var similarHint: String {
        let names = store.similarCategories(name: newName).map(\.name)
        guard !names.isEmpty else { return "" }
        return String(localized: "Similar:") + " " + names.joined(separator: ", ")
    }

    private func aliases(of categoryId: Uuid) -> [String] {
        store.categoryAliases
            .filter { $0.categoryId == categoryId }
            .map(\.alias)
            .sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
    }

    // MARK: - Actions

    /// Opens `category` with the caret in `field`, saving whatever row was
    /// open before. A row the core refuses stays open and keeps the focus, so
    /// the click that would have left it does not lose what was typed.
    private func open(_ category: CategoryView, at field: CategoryField) {
        guard CategoryDraft.isEditable(category) else { return }
        if editing == category.id {
            // Another cell of the same row: move the caret only, or the draft
            // would be rebuilt from the store and lose the edit.
            focus = CategoryCellFocus(row: category.id, field: field)
            return
        }
        if let editing, !commit(editing) { return }
        editing = category.id
        draft = CategoryDraft(name: category.name, aliases: aliases(of: category.id))
        focus = CategoryCellFocus(row: category.id, field: field)
    }

    /// esc: throw the draft away. `editing` is cleared before the focus, so
    /// the focus change that follows does not read as "left the row" and save
    /// what esc was meant to undo.
    private func cancel() {
        editing = nil
        focus = nil
    }

    /// ↩: save the row and close it.
    private func submit(_ categoryId: Uuid) {
        if commit(categoryId) { focus = nil }
    }

    /// The commit on blur a spreadsheet does: a click on another row, on the
    /// empty line or away from the table all write the line being left.
    private func commitIfLeft(_ new: CategoryCellFocus?) {
        guard let editing, new?.row != editing else { return }
        commit(editing)
    }

    /// Writes the draft back as the commands it implies and says whether they
    /// all went through: a rename, then one RemoveAlias per alias that is
    /// gone, then one AddAlias per new one. Nothing changed sends nothing.
    @discardableResult
    private func commit(_ categoryId: Uuid) -> Bool {
        guard editing == categoryId else { return true }
        guard let category = store.windowCategories.first(where: { $0.id == categoryId }) else {
            // Merged away, or gone with a sync while the row was open.
            editing = nil
            return true
        }
        if let name = draft.rename(from: category.name) {
            guard accepted({ store.renameCategory(categoryId, name: name) }) else {
                return keepOpen(categoryId, at: .name)
            }
        }
        let changes = draft.aliasChanges(from: aliases(of: categoryId))
        for alias in changes.removed {
            guard accepted({ store.removeAlias(categoryId: categoryId, alias: alias) }) else {
                return keepOpen(categoryId, at: .aliases)
            }
        }
        for alias in changes.added {
            guard accepted({ store.addAlias(categoryId: categoryId, alias: alias) }) else {
                return keepOpen(categoryId, at: .aliases)
            }
        }
        editing = nil
        return true
    }

    /// ↩ on the empty line. An empty name is not an error, it is nothing; a
    /// name the core refuses stays in the cell so it can be fixed.
    private func commitNewLine() {
        guard let name = CategoryDraft.creation(from: newName) else { return }
        guard accepted({ store.createCategory(name: name) }) else { return }
        newName = ""
        focus = CategoryCellFocus(row: nil, field: .name)
    }

    /// A refused command keeps the row open with its draft and sends the caret
    /// back to the cell that caused it.
    private func keepOpen(_ categoryId: Uuid, at field: CategoryField) -> Bool {
        focus = CategoryCellFocus(row: categoryId, field: field)
        return false
    }

    /// Runs a store command and says whether the core took it. The store turns
    /// a refusal into `presentedError`, the alert the ledger already uses
    /// (`AppStore.report` feeds the same one), instead of throwing.
    private func accepted(_ work: () -> Void) -> Bool {
        let before = store.presentedError
        work()
        return store.presentedError == before
    }
}

// MARK: - One row

private struct CategoryRowView: View {
    let category: CategoryView
    let aliases: [String]
    /// Non-nil while this row is the one being edited.
    let draft: Binding<CategoryDraft>?
    @FocusState.Binding var focus: CategoryCellFocus?
    let onOpen: (CategoryField) -> Void
    let onCommit: () -> Void
    let onCancel: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            if let draft {
                cell(draft.name, .name, width: CategoryColumn.name)
                cell(draft.aliases, .aliases, width: nil)
            } else {
                GridCell(width: CategoryColumn.name) { text(CategoryDraft.label(category)) }
                    .onTapGesture { onOpen(.name) }
                GridCell { aliasText }
                    .onTapGesture { onOpen(.aliases) }
            }
            GridCell(width: CategoryColumn.badge) {
                if category.isSystem {
                    SectionLabel(text: String(localized: "System"))
                } else {
                    Text("")
                }
            }
        }
        .frame(height: Metrics.rowHeight)
        .contentShape(Rectangle())
        .onKeyPress(.escape) {
            guard draft != nil else { return .ignored }
            onCancel()
            return .handled
        }
    }

    /// Archived categories keep their place at the bottom of the list, struck
    /// through and dim (`docs/v2/UI.md` §2.3).
    private func text(_ value: String) -> some View {
        Text(value)
            .font(Face.row)
            .foregroundStyle(category.archived ? Ink.dim : Ink.text)
            .strikethrough(category.archived)
    }

    @ViewBuilder
    private var aliasText: some View {
        if aliases.isEmpty {
            Text(TransactionRow.placeholder)
                .font(Face.row)
                .foregroundStyle(Ink.dim)
        } else {
            text(aliases.joined(separator: ", "))
        }
    }

    private func cell(_ value: Binding<String>, _ field: CategoryField, width: CGFloat?) -> some View {
        GridCell(width: width) {
            TextField("", text: value)
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .focused($focus, equals: CategoryCellFocus(row: category.id, field: field))
                .onSubmit(onCommit)
        }
    }
}

// MARK: - The draft behind an edited row

/// The text in the cells of the category row being edited, and the diff it
/// implies. Plain strings and plain functions, so the rules of `docs/v2/UI.md`
/// §2.3 can be tested without a view.
struct CategoryDraft: Equatable {
    var name: String = ""
    /// The aliases as one comma-separated line, which is how the column reads.
    var aliases: String = ""

    init(name: String = "", aliases: [String] = []) {
        self.name = name
        self.aliases = aliases.joined(separator: ", ")
    }

    /// Only active, non-system rows open for editing: the core refuses to
    /// rename or archive a system category, and an archived one is restored
    /// from the menu, not retyped.
    static func isEditable(_ category: CategoryView) -> Bool {
        !category.isSystem && !category.archived
    }

    /// System categories are named in English by the core (`Opening`,
    /// `Uncategorized`); the table shows the localized term, as
    /// `TransactionRow.categoryLabel` does.
    static func label(_ category: CategoryView) -> String {
        guard category.isSystem else { return category.name }
        switch category.name.lowercased() {
        case "opening": return String(localized: "Opening")
        case "uncategorized": return String(localized: "Uncategorized")
        default: return category.name
        }
    }

    /// The name the empty line would create, or nil when it is blank: ↩ on an
    /// empty line does nothing.
    static func creation(from text: String) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// The aliases as typed: split on commas, trimmed, empties dropped, and
    /// one entry per name whatever the case, since the core would refuse the
    /// second "casa" anyway.
    var aliasList: [String] {
        var seen = Set<String>()
        return aliases
            .split(separator: ",", omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty && seen.insert($0.lowercased()).inserted }
    }

    /// The new name to send, or nil when it did not change. An emptied cell
    /// leaves the name alone, the way the ledger's emptied amount does:
    /// clearing a cell is not a way to say "no name".
    func rename(from current: String) -> String? {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, trimmed != current else { return nil }
        return trimmed
    }

    /// The alias commands the line implies, against the aliases the category
    /// has now: removals first so a name freed on one line can be taken again
    /// on the same commit. Case and spacing alone are not a change.
    func aliasChanges(from current: [String]) -> (removed: [String], added: [String]) {
        let typed = aliasList
        let typedKeys = Set(typed.map { $0.lowercased() })
        let currentKeys = Set(current.map { $0.lowercased() })
        return (
            removed: current.filter { !typedKeys.contains($0.lowercased()) },
            added: typed.filter { !currentKeys.contains($0.lowercased()) }
        )
    }
}
