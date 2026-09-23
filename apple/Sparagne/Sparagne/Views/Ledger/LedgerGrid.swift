import SwiftUI
import SparagneCore

/// The spreadsheet (`docs/v2/UI.md` §2.1): one row per transaction, oldest
/// first, with an always-empty last line for the next one.
///
/// Editing is per row, not per cell: clicking a cell opens the whole row for
/// editing with the focus on that cell, ⇥ walks the fields, ↩ commits the diff
/// as one `UpdateTransaction` and esc throws the draft away. That is what the
/// footer hints promise, and it means a row is never half-written.
///
/// Leaving the row saves it too, the way a spreadsheet does: ⇥ off IMPORTO, a
/// click on another row or on the search field all commit what was typed. esc
/// is the only way to lose it.
///
/// ⌘-click and ⇧-click pick rows instead of opening one, and ⌘A picks them all
/// while no cell is being edited: the selection the bulk actions work on
/// (`SelectionBar`, `AppStore+Selection.swift`). The grid takes the keyboard
/// then (`selectionKeys`), so ⌫ voids what is selected and esc lets it go,
/// while a text field keeps its own ⌘A and ⌫ as long as it is being typed
/// into.
struct LedgerGrid: View {
    @Bindable var store: AppStore

    /// Which row is open for editing; `nil` is the new-row line at the bottom.
    @State private var editing: Uuid?
    @State private var draft = RowDraft()
    @State private var newRow = RowDraft()
    /// The row under the pointer, so the eye can follow it across a grid that
    /// is much wider than a line of text.
    @State private var hovered: Uuid?
    @FocusState private var focus: CellFocus?
    /// The grid as a whole, when no cell has the caret: what ⌘A, ⌫ and esc
    /// reach while rows are being picked rather than typed into.
    @FocusState private var gridFocused: Bool
    /// "Set Category…", from the selection bar or a selected row's menu.
    @State private var showsBulkCategory = false
    /// The list under the CATEGORY cell being typed into: one for the whole
    /// grid, drawn over it rather than inside a row (`completionList`).
    @State private var completion = CategoryCompletionModel()

    var body: some View {
        VStack(spacing: 0) {
            GridHeader(showsWallet: store.showWalletColumn)
            Hairline()
            ScrollView {
                LazyVStack(spacing: 0) {
                    let rows = store.rows
                    ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                        rowView(index: index, row: row)
                            .onAppear { if index == rows.count - 1 { loadNextPage() } }
                        Hairline()
                    }
                    // A viewer's ledger is a report: nothing to type into.
                    if store.canWrite { newRowView }
                }
            }
            .scrollBounceBehavior(.basedOnSize)
            .background { selectionKeys }
            // The hints are all about typing rows; with rows picked, the line
            // says what can be done to them instead.
            if store.canWrite {
                Hairline()
                if store.selection.count > 1 {
                    SelectionBar(store: store, showsCategory: $showsBulkCategory)
                } else {
                    KeyHints()
                }
            }
        }
        .overlayPreferenceValue(CategoryCompletionAnchor.self) { anchor in
            completionList(at: anchor)
        }
        .background(Ink.bg)
        // The vault turned read-only under an open row (the owner changed the
        // role): the row could not be saved, so it closes.
        .onChange(of: store.isReadOnly) { _, _ in resetEditing() }
        // The month and the vault change what the empty line means, so its
        // draft starts over with them. A sync landing or a void must not:
        // those move `store.rows` under a line that is being typed.
        .onChange(of: store.month) { _, _ in
            resetEditing()
            newRow = RowDraft.blank(in: store)
        }
        .onChange(of: store.currentVault?.id) { _, _ in
            resetEditing()
            newRow = RowDraft.blank(in: store)
        }
        .onChange(of: store.direction) { _, new in
            resetEditing()
            // A duplicated refund belongs to USCITE: saved from ENTRATE it
            // would vanish from the list it was typed into.
            if let kind = newRow.kind, !new.kinds.contains(kind) { newRow = RowDraft.blank(in: store) }
        }
        // Showing the column fills the empty line's cell with the sticky
        // default; hiding it puts that value back out of reach.
        .onChange(of: store.showWalletColumn) { _, _ in
            resetEditing()
            newRow = RowDraft.blank(in: store)
        }
        .onChange(of: focus) { _, new in
            commitIfLeft(new)
            // Typing into a cell is editing, not picking: one or the other.
            if new != nil { store.clearSelection() }
        }
        .onAppear {
            newRow = RowDraft.blank(in: store)
            // ⌘A works from the start, before any row was clicked.
            if focus == nil { gridFocused = true }
        }
        .task(id: newRow.note) { await suggestCategory() }
        .onReceive(NotificationCenter.default.publisher(for: .duplicateLastRow)) { _ in
            duplicateLast()
        }
    }

    // MARK: - The keys of a selection

    /// Where the keyboard goes while rows are picked rather than typed into:
    /// ⌘A takes every row, ⌫ and ⌦ void the selection, esc lets it go.
    ///
    /// An invisible view behind the rows, not the scroll view itself: a
    /// focusable container would take the focus on every click inside it,
    /// before the click reached a cell, and so close the row being edited
    /// under the pointer. Nothing here can be clicked, only focused from code
    /// (`pick`, `submit`, `cancelEditing`); and since no cell is inside it, a
    /// key typed into a cell never reaches these handlers, which only ever see
    /// keys while no cell has the caret.
    private var selectionKeys: some View {
        Color.clear
            .focusable()
            .focusEffectDisabled()
            .focused($gridFocused)
            // Menu's Select All, which a focused text field would take first.
            .onCommand(#selector(NSResponder.selectAll(_:))) { selectAll() }
            .onKeyPress(keys: [.delete, .deleteForward]) { _ in
                guard focus == nil, !store.selection.isEmpty else { return .ignored }
                Task { await store.voidSelection() }
                return .handled
            }
            .onKeyPress(.escape) {
                guard focus == nil, !store.selection.isEmpty else { return .ignored }
                store.clearSelection()
                return .handled
            }
            .accessibilityHidden(true)
    }

    // MARK: - Existing rows

    @ViewBuilder
    private func rowView(index: Int, row: TransactionRow) -> some View {
        let isEditing = editing == row.id
        let isSelected = store.selection.contains(row.id)
        LedgerRowView(
            ordinal: index + 1,
            row: row,
            store: store,
            showsWallet: store.showWalletColumn,
            draft: isEditing ? $draft : nil,
            focus: $focus,
            completion: completion,
            onOpen: { field in open(row, at: field) },
            onCommit: { submit(row.id) },
            onCancel: cancelEditing
        )
        .background(background(editing: isEditing, selected: isSelected, hovered: hovered == row.id))
        .overlay(alignment: .leading) {
            if isEditing || isSelected {
                Rectangle().fill(Ink.accent).frame(width: 2)
            }
        }
        .onHover { inside in
            if inside {
                hovered = row.id
            } else if hovered == row.id {
                hovered = nil
            }
        }
        // VoiceOver reads a closed row as one sentence; an open one is a group
        // of fields, each with its column's name.
        .accessibilityElement(children: isEditing ? .contain : .ignore)
        .accessibilityLabel(LedgerAccessibility.label(for: row, showsWallet: store.showWalletColumn))
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .accessibilityActions { rowActions(row, selected: isSelected) }
        .contextMenu {
            // A row inside a selection speaks for all of it; any other row
            // for itself, as it always has.
            if isSelected, store.selection.count > 1 {
                bulkMenu
            } else {
                rowMenu(row)
            }
        }
    }

    /// Selected rows are tinted with the accent, the palette's color for a
    /// selection (`docs/v2/UI.md` §5), a shade deeper under the pointer.
    private func background(editing: Bool, selected: Bool, hovered: Bool) -> Color {
        if editing { return Ink.raised }
        if selected { return Ink.accent.opacity(hovered ? 0.22 : 0.15) }
        return hovered ? Ink.raised : Color.clear
    }

    @ViewBuilder
    private func rowMenu(_ row: TransactionRow) -> some View {
        // A transfer's two ends do not fit the empty line, so it has no copy
        // to offer (`RowDraft.duplicate`).
        if !row.isTransfer {
            Button(String(localized: "Duplicate")) { duplicate(row) }
                .disabled(store.isReadOnly)
            Divider()
        }
        Button(String(localized: "Void"), role: .destructive) {
            Task { await store.void(transactionId: row.id) }
        }
        .disabled(store.isReadOnly || row.voided)
    }

    /// What VoiceOver offers on a row: the context menu's actions, the click
    /// that opens it, and the ⌘-click that picks it, since neither gesture is
    /// at hand without a pointer.
    @ViewBuilder
    private func rowActions(_ row: TransactionRow, selected: Bool) -> some View {
        if store.canWrite {
            if !row.voided {
                Button(String(localized: "Edit")) { open(row, at: .note) }
            }
            if !row.isTransfer {
                Button(String(localized: "Duplicate")) { duplicate(row) }
            }
            if !row.voided {
                Button(String(localized: "Void")) { Task { await store.void(transactionId: row.id) } }
            }
            Button(selected ? String(localized: "Deselect") : String(localized: "Select")) {
                store.toggleSelection(row.id)
            }
        }
    }

    @ViewBuilder
    private var bulkMenu: some View {
        let targets = store.bulkTargets.count
        Button(String(localized: "Set Category\u{2026}")) { showsBulkCategory = true }
            .disabled(targets == 0)
        Divider()
        Button(String(localized: "Void \(targets) Rows"), role: .destructive) {
            Task { await store.voidSelection() }
        }
        .disabled(targets == 0)
    }

    // MARK: - The empty line

    private var newRowView: some View {
        NewRowView(
            store: store,
            draft: $newRow,
            focus: $focus,
            completion: completion,
            onCommit: commitNewRow,
            showsWallet: store.showWalletColumn
        )
            .background(focus?.row == nil && focus != nil ? Ink.raised : Color.clear)
            .overlay(alignment: .leading) {
                Rectangle().fill(Ink.accent).frame(width: 2)
            }
    }

    // MARK: - The completion list

    /// The CATEGORY cell's list, hung from the bottom of its row, or from the
    /// top when there is no room below (the empty line, a row at the bottom
    /// of the window). Drawn over the whole grid, outside the scroll view, so
    /// no later row covers it, the scroll view does not clip it, and a click
    /// on it is not a click on the grid that would close the row.
    @ViewBuilder
    private func completionList(at anchor: Anchor<CGRect>?) -> some View {
        if let anchor, completion.isOpen {
            GeometryReader { proxy in
                let cell = proxy[anchor]
                let height = CategoryCompletionList.height(for: completion.candidates.count)
                let fitsBelow = cell.maxY + height <= proxy.size.height
                CategoryCompletionList(completion: completion)
                    .offset(x: cell.minX - GridColumn.padding, y: fitsBelow ? cell.maxY : cell.minY - height)
            }
        }
    }

    // MARK: - Actions

    /// The last row loaded has scrolled into view: a month longer than a
    /// page goes on with the next one, so it is never silently cut short.
    private func loadNextPage() {
        guard store.nextCursor != nil else { return }
        Task { await store.loadMore() }
    }

    /// Opens `row` with the caret in `field`, saving whatever line was open
    /// before. A row that refuses to save stays open and keeps the focus, so
    /// the click that would have left it does not lose it.
    ///
    /// The click may have been a ⌘-click or a ⇧-click, which pick the row
    /// instead. The cells only report a tap, so the modifiers are read off the
    /// event being handled.
    private func open(_ row: TransactionRow, at field: RowField) {
        let modifiers = NSEvent.modifierFlags
        if modifiers.contains(.command) || modifiers.contains(.shift) {
            pick(row, extending: !modifiers.contains(.command))
            return
        }
        store.clearSelection()
        guard !row.voided, store.canWrite else { return }
        if editing == row.id {
            // Another cell of the row already open: move the caret only, or
            // the draft would be rebuilt from the stored row and lose the edit.
            focus = CellFocus(row: row.id, field: field)
            return
        }
        if let editing, !commit(editing) { return }
        editing = row.id
        draft = RowDraft(row: row, store: store)
        focus = CellFocus(row: row.id, field: field)
    }

    /// esc: throw the draft away. `editing` is cleared before the focus, so
    /// the focus change that follows does not read as "left the row" and save
    /// what esc was meant to undo.
    private func resetEditing() {
        editing = nil
        focus = nil
    }

    /// esc from a row: the draft goes and the grid keeps the keyboard, so ⌘A
    /// has somewhere to land.
    private func cancelEditing() {
        resetEditing()
        gridFocused = true
    }

    /// ↩: save the row and close it.
    private func submit(_ rowId: Uuid) {
        if commit(rowId) {
            focus = nil
            gridFocused = true
        }
    }

    /// ⌘-click toggles `row`, ⇧-click extends to it. The row being edited is
    /// saved first, as any click away from it saves it; one that refuses to
    /// save keeps the caret, and nothing is picked. The grid takes the
    /// keyboard, so ⌫ and esc reach the selection.
    private func pick(_ row: TransactionRow, extending: Bool) {
        guard store.canWrite else { return }
        if let editing, !commit(editing) { return }
        focus = nil
        gridFocused = true
        if extending {
            store.extendSelection(to: row.id)
        } else {
            store.toggleSelection(row.id)
        }
    }

    /// ⌘A with no cell being edited: every row on screen.
    private func selectAll() {
        guard focus == nil, store.canWrite else { return }
        store.selectAllRows()
        gridFocused = true
    }

    /// The commit on blur a spreadsheet does: ⇥ off IMPORTO lands on the empty
    /// line, a click lands on another row or on the search field, and in every
    /// case the line being left is written.
    private func commitIfLeft(_ new: CellFocus?) {
        guard let editing, new?.row != editing else { return }
        commit(editing)
    }

    /// Writes the draft back and says whether it went through.
    ///
    /// Sends only what changed, so two people editing different cells of the
    /// same row do not conflict (`docs/v2/ARCH.md` §4). A cell the parser or
    /// the core refuses keeps the row open with the draft intact and puts the
    /// focus back where the mistake is.
    @discardableResult
    private func commit(_ rowId: Uuid) -> Bool {
        guard editing == rowId else { return true }
        guard let row = store.rows.first(where: { $0.id == rowId }) else {
            // Voided, or carried out of the month by a sync while it was open.
            editing = nil
            return true
        }
        do {
            let patch = try draft.patch(against: row, store: store)
            editing = nil
            // The write goes to the core actor; the row closes now, because
            // the draft was already checked against the cells.
            if !patch.isEmpty { Task { await store.update(transactionId: rowId, patch: patch) } }
            return true
        } catch let failure as RowDraftError {
            store.report(failure.underlying)
            focus = CellFocus(row: rowId, field: failure.field)
            return false
        } catch {
            store.report(error)
            return false
        }
    }

    private func commitNewRow() {
        do {
            guard let entry = try newRow.entry(store: store), entry.amount > 0 else { return }
            Task {
                await store.addRow(
                    day: entry.day,
                    flowId: entry.flowId,
                    category: entry.category,
                    note: entry.note,
                    amount: entry.amount,
                    walletId: entry.walletId,
                    kind: entry.kind
                )
            }
            newRow = RowDraft.blank(in: store)
            focus = CellFocus(row: nil, field: .date)
        } catch let failure as RowDraftError {
            store.report(failure.underlying)
            focus = CellFocus(row: nil, field: failure.field)
        } catch {
            store.report(error)
        }
    }

    /// The empty line's note has been still for a moment: ask the core what
    /// the vault files it under, for the CATEGORY cell to show while it is
    /// empty. An answer that comes back after the note moved on is dropped.
    private func suggestCategory() async {
        let note = newRow.note.trimmingCharacters(in: .whitespacesAndNewlines)
        guard store.canWrite, !note.isEmpty, newRow.suggestion?.note != note else { return }
        guard (try? await Task.sleep(for: .milliseconds(250))) != nil else { return }
        guard let category = await store.suggestedCategory(forNote: note),
              newRow.note.trimmingCharacters(in: .whitespacesAndNewlines) == note
        else { return }
        newRow.suggestion = NoteSuggestion(note: note, category: category)
    }

    /// ⌘D and the context menu: copy a row into the empty line, ready to be
    /// tweaked and saved.
    private func duplicate(_ row: TransactionRow) {
        guard !store.isReadOnly, let copy = RowDraft.duplicate(of: row, store: store) else { return }
        newRow = copy
        focus = CellFocus(row: nil, field: .amount)
    }

    /// ⌘D with nothing selected duplicates the last row of the month that is
    /// not a transfer (`AppStore.lastRow`).
    private func duplicateLast() {
        guard let last = store.lastRow else { return }
        duplicate(last)
    }
}

// MARK: - Focus

/// A cell: which row (nil = the new line) and which field.
struct CellFocus: Hashable {
    let row: Uuid?
    let field: RowField
}

/// The cells a row is typed into. PERSONA is not one: it is the author of the
/// command, not a field (`docs/v2/UI.md` §3).
///
/// Nothing here drives the traversal. ⇥ and ⇧⇥ are the system's, walking the
/// focusable views in layout order, and the order of the cases is only the
/// order the cells are laid out in. A closed row holds no text field, so from
/// IMPORTO of the row being edited ⇥ reaches the DATA cell of the empty line
/// and never gets stuck on the rows in between; ⇧⇥ from DATA goes back up to
/// the header, whose search field is the previous focusable view. Both leave
/// the row, which is what `commitIfLeft` is for.
enum RowField: Hashable, CaseIterable {
    case date
    case flow
    case category
    case note
    /// Only in the hierarchy while the optional WALLET column is on.
    case wallet
    case amount

    /// The column heading, which is what VoiceOver calls the cell: an open
    /// row's fields have no placeholder to be read instead.
    var label: String {
        switch self {
        case .date: String(localized: "Date")
        case .flow: String(localized: "Envelope")
        case .category: String(localized: "Category")
        case .note: String(localized: "Description")
        case .wallet: String(localized: "Wallet")
        case .amount: String(localized: "Amount")
        }
    }
}

// MARK: - The draft behind an edited row

/// A category the core suggested for a note (`AppStore.suggestedCategory`),
/// and the note it was for.
struct NoteSuggestion: Equatable, Sendable {
    let note: String
    let category: String
}

/// A cell that refused what was typed. The grid reports `underlying`, which
/// carries the core's own message, and puts the focus back on `field`.
struct RowDraftError: Error {
    let field: RowField
    let underlying: Error
}

/// The text in the cells of the row being edited. Everything is a string
/// until it commits, so a half-typed amount never has to be a number.
///
/// Main-actor bound because resolving a name or parsing an amount reads the
/// store, which owns the core.
@MainActor
struct RowDraft {
    var day = Date()
    var flow = ""
    var category = ""
    var note = ""
    var wallet = ""
    var amount = ""
    /// What the new line is written as. `nil` is the direction on screen's
    /// (`LedgerDirection.newRowKind`); a duplicate keeps its source's, so a
    /// refund copied into the line is saved as a refund, not an expense.
    var kind: TransactionKind?
    /// The category the vault's history files the note under, as the core
    /// suggested it for the note it was asked about. The empty line shows it
    /// in its CATEGORY cell while that is empty, and saves it if it still is.
    var suggestion: NoteSuggestion?

    nonisolated init() {}

    /// The suggestion, while it is still about the note in the cell: one
    /// keystroke in DESCRIZIONE and it no longer is.
    var suggestedCategory: String? {
        guard let suggestion, suggestion.note == note.trimmingCharacters(in: .whitespacesAndNewlines) else {
            return nil
        }
        return suggestion.category
    }

    init(row: TransactionRow, store: AppStore) {
        kind = row.kind
        day = row.occurredAt
        flow = row.envelopeDisplay == TransactionRow.placeholder ? "" : row.envelopeDisplay
        category = row.category
        note = row.note
        // A transfer's WALLET cell is an arrow between two wallets, not a
        // name: nothing the cell could resolve, so it opens empty and the
        // commit leaves the legs alone.
        wallet = row.isTransfer || row.walletDisplay == TransactionRow.placeholder ? "" : row.walletDisplay
        amount = LedgerMoney.editable(row.absoluteAmount)
    }

    /// A new line: today if the month on screen is the current one, its first
    /// day otherwise, and the sticky envelope already filled in.
    static func blank(in store: AppStore) -> RowDraft {
        var draft = RowDraft()
        let now = Date()
        draft.day = MonthKey(now) == store.month ? now : store.month.start()
        if let id = store.defaultFlowId, let flow = store.flows.first(where: { $0.id == id }) {
            draft.flow = store.flowName(flow)
        }
        if store.showWalletColumn { draft.wallet = store.defaultWalletName ?? "" }
        return draft
    }

    /// `row` copied into the empty line, dated today, ready to be tweaked and
    /// saved (⌘D and the context menu). `nil` for a transfer: the line has one
    /// wallet and one envelope, and a transfer's two ends fit neither.
    static func duplicate(of row: TransactionRow, store: AppStore) -> RowDraft? {
        guard !row.isTransfer else { return nil }
        var draft = RowDraft(row: row, store: store)
        draft.day = Date()
        return draft
    }

    /// The fields of a new row, resolved and parsed. `nil` when the amount
    /// cell is still empty: tabbing to the end of a blank line and pressing ↩
    /// is not an error, it is nothing.
    func entry(
        store: AppStore
    ) throws -> (
        day: Date,
        flowId: Uuid?,
        category: String?,
        note: String,
        amount: Int64,
        walletId: Uuid?,
        kind: TransactionKind?
    )? {
        guard !amount.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        let resolvedFlow = try cell(.flow) { try store.resolveFlow(named: flow) }
        let resolvedWallet = try cell(.wallet) { try store.resolveWallet(named: wallet) }
        let parsedAmount = try cell(.amount) { try parseMoney(text: amount, currency: store.currency) }
        let trimmed = category.trimmingCharacters(in: .whitespacesAndNewlines)
        return (
            day: day,
            flowId: resolvedFlow ?? store.defaultFlowId,
            // An empty cell showing a suggestion saves what it shows.
            category: trimmed.isEmpty ? suggestedCategory : trimmed,
            note: note,
            amount: parsedAmount,
            walletId: resolvedWallet,
            kind: kind
        )
    }

    /// The diff against the row as it is stored: only the cells that changed.
    func patch(against row: TransactionRow, store: AppStore) throws -> TransactionPatch {
        var patch = TransactionPatch()

        // An emptied amount cell leaves the amount alone: clearing it is not a
        // way to say zero, and the core would refuse zero anyway.
        if !amount.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            let parsed = try cell(.amount) { try parseMoney(text: amount, currency: store.currency) }
            if parsed != row.absoluteAmount, parsed > 0 { patch.amount = parsed }
        }

        let trimmedCategory = category.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedCategory != row.category { patch.category = trimmedCategory }

        let trimmedNote = note.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedNote != row.note { patch.note = trimmedNote }

        let resolvedFlow = try cell(.flow) { try store.resolveFlow(named: flow) }
        if let flowId = resolvedFlow, flowId != row.flowId { patch.flowId = flowId }

        // Only when the cell was actually retyped: a row sitting on an
        // archived wallet still shows its name, which no longer resolves.
        let trimmedWallet = wallet.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmedWallet.isEmpty, trimmedWallet != row.walletDisplay {
            let resolvedWallet = try cell(.wallet) { try store.resolveWallet(named: trimmedWallet) }
            if let walletId = resolvedWallet, walletId != row.walletId { patch.walletId = walletId }
        }

        if !Calendar.current.isDate(day, inSameDayAs: row.occurredAt) {
            patch.occurredAt = AppStore.stamp(day: day, likeTimeOf: row.occurredAt)
        }
        return patch
    }

    /// Runs a cell's parse, tagging whatever it throws with the cell it came
    /// from so the grid can send the focus back there.
    private func cell<T>(_ field: RowField, _ work: () throws -> T) throws -> T {
        do {
            return try work()
        } catch {
            throw RowDraftError(field: field, underlying: error)
        }
    }
}
