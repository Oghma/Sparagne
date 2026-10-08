import SwiftUI
import SparagneCore

/// Column geometry, fixed so the header, the rows and the empty line agree
/// without a layout pass (`docs/v2/UI.md` §2.3 and §5): the canvas's 140 for
/// NOME, 80 for the usage and 36 for the action. ALIAS takes what is left.
private enum CategoryColumn {
    static let name: CGFloat = 140
    static let usage: CGFloat = 80
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
    /// The names the core reports as close to what is being typed in the
    /// empty line; filled by a task, never by the layout pass.
    @State private var similar: [String] = []
    @FocusState private var focus: CategoryCellFocus?

    var body: some View {
        SetupCard(
            title: String(localized: "Categories"),
            hint: String(localized: "aliases take quick-add to the category: #pizza → Restaurants")
        ) {
            SetupTableBody {
                header
                ForEach(active, id: \.id) { category in
                    row(category)
                }
                // Archived ones keep their place at the bottom, dimmed.
                ForEach(archived, id: \.id) { category in
                    row(category)
                }
                // A viewer reads the categories; nothing to add.
                if store.canWrite { newLine }
            }
        }
        // A vault switch reloads the lists under whatever was open.
        .onChange(of: store.currentVault?.id) { _, _ in
            editing = nil
            newName = ""
            focus = nil
        }
        .onChange(of: focus) { _, new in Task { await commitIfLeft(new) } }
        // Turned read-only under an open row: it could not be saved.
        .onChange(of: store.isReadOnly) { _, _ in cancel() }
        .task(id: newName) {
            similar = await store.similarCategories(name: newName).map(\.name)
        }
        .sheet(item: $merging) { subject in
            MergeCategorySheet(
                store: store,
                source: subject.category,
                targets: active.filter { $0.id != subject.category.id }
            )
        }
    }

    // MARK: - Chrome

    private var header: some View {
        SetupHeaderRow {
            SetupCell(width: CategoryColumn.name) { Text(String(localized: "Name")) }
            SetupCell { Text(String(localized: "Aliases")) }
            SetupCell(width: CategoryColumn.usage, alignment: .trailing) { Text(String(localized: "Rows 90 d")) }
            SetupCell(width: SetupColumn.action) { EmptyView() }
        }
    }

    // MARK: - Rows

    private var active: [CategoryView] { store.windowCategories.filter { !$0.archived && matchesFilter($0) } }
    private var archived: [CategoryView] { store.windowCategories.filter { $0.archived && matchesFilter($0) } }

    /// The top bar's search, on the name and the aliases; the empty line stays.
    private func matchesFilter(_ category: CategoryView) -> Bool {
        TableFilter.matches(store.tabFilter, [category.name] + aliases(of: category.id))
    }

    /// System categories are read-only: the core refuses to rename, archive or
    /// merge them, so they carry no menu at all rather than an empty one. So
    /// does every category of a vault this account only reads.
    @ViewBuilder
    private func row(_ category: CategoryView) -> some View {
        let line = line(category)
        if category.isSystem || !store.canWrite {
            line
        } else if category.archived {
            line.contextMenu {
                Button(String(localized: "Restore")) {
                    Task { await store.restoreCategory(category.id) }
                }
            }
        } else {
            line.contextMenu {
                Button(String(localized: "Merge Into…")) { merging = MergeSubject(category: category) }
                Divider()
                Button(String(localized: "Archive"), role: .destructive) {
                    Task { await store.archiveCategory(category.id) }
                }
            }
        }
    }

    @ViewBuilder
    private func line(_ category: CategoryView) -> some View {
        let isEditing = editing == category.id
        CategoryRowView(
            category: category,
            aliases: aliases(of: category.id),
            usage: store.categoryUsage[category.id] ?? 0,
            draft: isEditing ? $draft : nil,
            isHovered: hovered == category.id,
            isWritable: store.canWrite,
            focus: $focus,
            onOpen: { field in Task { await open(category, at: field) } },
            onCommit: { Task { await submit(category.id) } },
            onCancel: cancel,
            onArchive: { Task { await store.archiveCategory(category.id) } },
            onRestore: { Task { await store.restoreCategory(category.id) } }
        )
        .onHover { inside in
            if inside {
                hovered = category.id
            } else if hovered == category.id {
                hovered = nil
            }
        }
    }

    /// The empty last line: a name, and under ALIAS the near names the core
    /// knows, which suggest and never block (`docs/v2/DISTILLATO_V1.md` §2.1).
    private var newLine: some View {
        let active = focus == CategoryCellFocus(row: nil, field: .name)
        return SetupRow(highlighted: active, editing: active, separator: false) {
            SetupCell(width: CategoryColumn.name) {
                TextField(String(localized: "New category…"), text: $newName)
                    .textFieldStyle(.plain)
                    .font(Face.row)
                    .foregroundStyle(Ink.text)
                    .focused($focus, equals: CategoryCellFocus(row: nil, field: .name))
                    .onSubmit { Task { await commitNewLine() } }
            }
            SetupCell {
                Text(similarHint)
                    .font(Face.row)
                    .foregroundStyle(Ink.text3)
            }
            SetupCell(width: CategoryColumn.usage) { EmptyView() }
            SetupCell(width: SetupColumn.action) { EmptyView() }
        }
        .onKeyPress(.escape) {
            newName = ""
            focus = nil
            return .handled
        }
    }

    /// The hint beside the empty line, refreshed by a `task(id:)` on the
    /// typed name: the lookup is a core query, so it cannot be computed while
    /// the row is being laid out. With nothing close, it says what the
    /// ALIAS column takes.
    private var similarHint: String {
        guard !similar.isEmpty else { return String(localized: "aliases, comma-separated") }
        let names = similar.joined(separator: ", ")
        return String(localized: "Similar: \(names)")
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
    private func open(_ category: CategoryView, at field: CategoryField) async {
        guard CategoryDraft.isEditable(category), store.canWrite else { return }
        if editing == category.id {
            // Another cell of the same row: move the caret only, or the draft
            // would be rebuilt from the store and lose the edit.
            focus = CategoryCellFocus(row: category.id, field: field)
            return
        }
        if let editing, !(await commit(editing)) { return }
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
    private func submit(_ categoryId: Uuid) async {
        if await commit(categoryId) { focus = nil }
    }

    /// The commit on blur a spreadsheet does: a click on another row, on the
    /// empty line or away from the table all write the line being left.
    private func commitIfLeft(_ new: CategoryCellFocus?) async {
        guard let editing, new?.row != editing else { return }
        await commit(editing)
    }

    /// Writes the draft back as the commands it implies and says whether they
    /// all went through: a rename, then one RemoveAlias per alias that is
    /// gone, then one AddAlias per new one. Nothing changed sends nothing.
    @discardableResult
    private func commit(_ categoryId: Uuid) async -> Bool {
        guard editing == categoryId else { return true }
        guard let category = store.windowCategories.first(where: { $0.id == categoryId }) else {
            // Merged away, or gone with a sync while the row was open.
            editing = nil
            return true
        }
        if let name = draft.rename(from: category.name) {
            guard await accepted({ await store.renameCategory(categoryId, name: name) }) else {
                return keepOpen(categoryId, at: .name)
            }
        }
        let changes = draft.aliasChanges(from: aliases(of: categoryId))
        for alias in changes.removed {
            guard await accepted({ await store.removeAlias(categoryId: categoryId, alias: alias) }) else {
                return keepOpen(categoryId, at: .aliases)
            }
        }
        for alias in changes.added {
            guard await accepted({ await store.addAlias(categoryId: categoryId, alias: alias) }) else {
                return keepOpen(categoryId, at: .aliases)
            }
        }
        editing = nil
        return true
    }

    /// ↩ on the empty line. An empty name is not an error, it is nothing; a
    /// name the core refuses stays in the cell so it can be fixed.
    private func commitNewLine() async {
        guard let name = CategoryDraft.creation(from: newName) else { return }
        guard await accepted({ await store.createCategory(name: name) }) else { return }
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
    private func accepted(_ work: () async -> Void) async -> Bool {
        let before = store.presentedError
        await work()
        return store.presentedError == before
    }
}

// MARK: - One row

private struct CategoryRowView: View {
    let category: CategoryView
    let aliases: [String]
    /// Rows in the last 90 days.
    let usage: Int
    /// Non-nil while this row is the one being edited.
    let draft: Binding<CategoryDraft>?
    let isHovered: Bool
    /// False on a vault this account only reads: no archive or restore icon.
    let isWritable: Bool
    @FocusState.Binding var focus: CategoryCellFocus?
    let onOpen: (CategoryField) -> Void
    let onCommit: () -> Void
    let onCancel: () -> Void
    let onArchive: () -> Void
    let onRestore: () -> Void

    var body: some View {
        SetupRow(highlighted: draft != nil || isHovered, editing: draft != nil) {
            if let draft {
                cell(draft.name, .name, width: CategoryColumn.name)
                cell(draft.aliases, .aliases, width: nil)
            } else {
                SetupCell(width: CategoryColumn.name) { nameCell }
                    .onTapGesture { onOpen(.name) }
                SetupCell { aliasCell }
                    .onTapGesture { onOpen(.aliases) }
            }
            SetupCell(width: CategoryColumn.usage, alignment: .trailing) { usageText }
            actionCell
        }
        .onKeyPress(.escape) {
            guard draft != nil else { return .ignored }
            onCancel()
            return .handled
        }
    }

    /// The name, with the "system" tag on the two the core owns. An archived
    /// category is dimmed, not struck through: the tag beside its alias
    /// column already says why it is down here.
    private var nameCell: some View {
        HStack(spacing: 6) {
            Text(CategoryDraft.label(category))
                .font(Face.row)
                .foregroundStyle(tint)
                .lineLimit(1)
            if category.isSystem { SetupTag(text: String(localized: "system")) }
        }
    }

    private var tint: Color {
        if category.archived { return Ink.text3 }
        return category.isSystem ? Ink.text2 : Ink.text
    }

    /// The aliases as chips; for an archived row the "archived" tag and a
    /// ghost "Restore" button instead, next to each other. They used to sit
    /// in the 36 pt action column, which cut the word off.
    @ViewBuilder
    private var aliasCell: some View {
        if category.archived {
            HStack(spacing: 6) {
                SetupTag(text: String(localized: "archived"))
                if isWritable {
                    Button(action: onRestore) {
                        Text(String(localized: "Restore"))
                            .font(Face.ui(11))
                            .foregroundStyle(Ink.text2)
                            .padding(.horizontal, 4)
                            .frame(height: 18)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(String(localized: "Restore \(category.name)"))
                }
            }
        } else {
            HStack(spacing: 3) {
                ForEach(aliases, id: \.self) { AliasChip(text: $0) }
            }
            .clipped()
        }
    }

    /// Zero reads as background, a system category has no usage worth
    /// counting (`Opening` and `Uncategorized` come from the core).
    @ViewBuilder
    private var usageText: some View {
        if category.isSystem {
            Text("—").font(Face.row).foregroundStyle(Ink.text3)
        } else {
            Text(usage.formatted(.number))
                .font(Face.row)
                .foregroundStyle(usage == 0 || category.archived ? Ink.text3 : Ink.text)
        }
    }

    /// The archive icon, only for the row under the pointer: the same
    /// command the context menu already sends, one click closer. Hidden while
    /// editing, for a system category (which the core refuses either way) and
    /// for an archived one, whose Restore button is beside its tag.
    @ViewBuilder
    private var actionCell: some View {
        SetupCell(width: SetupColumn.action) {
            if draft == nil, isHovered, isWritable, !category.isSystem, !category.archived {
                SetupIconButton(
                    symbol: "archivebox",
                    help: String(localized: "Archive"),
                    label: String(localized: "Archive \(category.name)"),
                    action: onArchive
                )
            }
        }
    }

    private func cell(_ value: Binding<String>, _ field: CategoryField, width: CGFloat?) -> some View {
        SetupCell(width: width) {
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
