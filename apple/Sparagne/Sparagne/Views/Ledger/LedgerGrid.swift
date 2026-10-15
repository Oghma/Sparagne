import SwiftUI
import SparagneCore

/// The spreadsheet: one row per transaction, oldest
/// first, the recurring periods due this month among them at their date
/// (`PendingRowView`), and an always-empty last line for the next one.
///
/// Editing is per row, not per cell: clicking a cell opens the whole row for
/// editing with the focus on that cell, ⇥ walks the fields, ↩ commits the diff
/// as one `UpdateTransaction` and esc throws the draft away, so a row is never
/// half-written.
///
/// Leaving the row saves it too, the way a spreadsheet does: ⇥ off IMPORTO, a
/// click on another row or on the search field all commit what was typed. esc
/// is the only way to lose it.
///
/// ⌘-click and ⇧-click pick rows instead of opening one, and ⌘A picks them all
/// while no cell is being edited: the selection the bulk actions work on
/// (`SelectionBar`, `AppStore+Selection.swift`). The grid takes the keyboard
/// then (`selectionKeys`), so ⌫ deletes what is selected and esc lets it go,
/// while a text field keeps its own ⌘A and ⌫ as long as it is being typed
/// into. With nothing selected, ⌫ deletes the row under the pointer, the one
/// showing its trash icon (`DeleteTarget`).
struct LedgerGrid: View {
    @Bindable var store: AppStore

    /// Which row is open for editing; `nil` is the new-row line at the bottom.
    @State private var editing: Uuid?
    @State private var draft = RowDraft()
    @State private var newRow = RowDraft()
    /// The row under the pointer, so the eye can follow it across a grid that
    /// is much wider than a line of text. Its trash icon shows, and ⌫ with
    /// nothing selected deletes it.
    @State private var hovered: Uuid?
    /// The lines as last worked out: the body runs again on every pointer
    /// move and keystroke, and the lines change only with their input.
    @State private var linesCache = LedgerLines.Cache()
    @FocusState private var focus: CellFocus?
    /// The grid as a whole, when no cell has the caret: what ⌘A, ⌫ and esc
    /// reach while rows are being picked rather than typed into.
    @FocusState private var gridFocused: Bool
    /// "Set Category…", from the selection bar or a selected row's menu.
    @State private var showsBulkCategory = false
    /// "Set Envelope…", the same way.
    @State private var showsBulkEnvelope = false
    /// The list under the CATEGORY cell being typed into: one for the whole
    /// grid, drawn over it rather than inside a row (`hangingCategoryList`).
    @State private var completion = CategoryCompletionModel()
    /// The list under the ENVELOPE cell that has the caret, drawn the same
    /// way (`envelopeList`).
    @State private var envelopes = EnvelopeChoiceModel()

    var body: some View {
        VStack(spacing: 0) {
            GridHeader(store: store, showsWallet: store.showWalletColumn)
                .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
            GeometryReader { viewport in
                ScrollViewReader { proxy in
                    ScrollView {
                        lines(viewportHeight: viewport.size.height)
                    }
                    .scrollBounceBehavior(.basedOnSize)
                    .background { selectionKeys }
                    // A row asked for from the RIEPILOGO's heaviest expenses:
                    // the grid may appear before the month's rows are loaded,
                    // so it looks again as they arrive.
                    .onAppear { reveal(with: proxy) }
                    .onChange(of: store.revealedRow) { _, _ in reveal(with: proxy) }
                    .onChange(of: store.rows.count) { _, _ in reveal(with: proxy) }
                    .onChange(of: store.rows.first?.id) { _, _ in reveal(with: proxy) }
                }
            }
            // With rows picked, a line under the sheet says what can be done
            // to all of them at once.
            if store.canWrite, store.selection.count > 1 {
                SelectionBar(store: store, showsCategory: $showsBulkCategory, showsEnvelope: $showsBulkEnvelope)
            }
        }
        // Three points in from the cell's edge, as the canvas hangs it (`.ac`).
        .hangingCategoryList(completion, inset: 3 - GridColumn.padding)
        .overlayPreferenceValue(EnvelopeChoiceAnchor.self) { anchor in
            envelopeList(at: anchor)
        }
        .background(Ink.sheet)
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
        // A login or a new local name changes who the author is: the empty
        // line follows, unless its PERSONA cell was already set to someone.
        .onChange(of: store.currentAuthor) { old, new in
            if newRow.person == old { newRow.person = new }
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
    /// ⌘A takes every row, ⌫ and ⌦ delete the selection, or with none the
    /// row under the pointer (`deleteTarget`), esc lets the selection go.
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
                switch deleteTarget {
                case .selection:
                    Task { await store.voidSelection() }
                case .row(let id):
                    Task { await store.void(transactionId: id) }
                case .none:
                    return .ignored
                }
                return .handled
            }
            .onKeyPress(.escape) {
                guard focus == nil, !store.selection.isEmpty else { return .ignored }
                store.clearSelection()
                return .handled
            }
            .accessibilityHidden(true)
    }

    /// What ⌫ deletes now. The hovered row is looked up among the rows on
    /// screen: one just deleted is gone from them while the pointer has not
    /// moved yet, and a second ⌫ must find nothing there.
    private var deleteTarget: DeleteTarget {
        DeleteTarget.resolve(
            selection: store.selectedRows.map(\.id),
            hovered: hovered.flatMap { id in store.rows.first { $0.id == id } },
            editing: focus != nil || editing != nil,
            canWrite: store.canWrite
        )
    }

    // MARK: - A row revealed

    /// Scrolls the row `AppStore.reveal` asked for to the middle of the
    /// sheet, once it is among the rows, and lets it go: the request is
    /// consumed, so coming back to the tab later does not jump again. Until
    /// the row is loaded the request waits.
    private func reveal(with proxy: ScrollViewProxy) {
        guard let id = store.revealedRow, let row = store.rows.first(where: { $0.id == id }) else { return }
        // The rows' identity in the `ForEach` is their line's, not the
        // transaction's id.
        let target = LedgerLine.row(row).id
        // The next turn of the run loop: on the first appearance the lazy
        // stack has not laid its rows out yet, and the scroll would land
        // nowhere.
        Task { @MainActor in
            proxy.scrollTo(target, anchor: .center)
            if store.revealedRow == id { store.revealedRow = nil }
        }
    }

    // MARK: - The lines

    /// The rows with the due periods among them (`AppStore.ledgerLinesInput`),
    /// the empty line, and ruled lines down to the bottom of the window: the
    /// sheet's pitch is fixed, so how much is left to rule is known without
    /// measuring anything.
    private func lines(viewportHeight: CGFloat) -> some View {
        let input = store.ledgerLinesInput
        linesCache.update(input)
        let rows = input.rows
        let lines = linesCache.lines
        // The # column numbers the stored rows only: a due period is not one
        // yet, and gets a symbol instead.
        let ordinals = linesCache.ordinals
        let drawn = CGFloat(lines.count + (store.canWrite ? 1 : 0)) * Metrics.rowHeight
        return LazyVStack(spacing: 0) {
            ForEach(lines) { line in
                switch line {
                case .row(let row):
                    let index = ordinals[row.id] ?? 0
                    rowView(index: index, row: row)
                        .rowRule()
                        .onAppear { if index == rows.count - 1 { loadNextPage() } }
                case .pending(let period):
                    PendingRowView(period: period, store: store, showsWallet: store.showWalletColumn)
                        .rowRule()
                }
            }
            // A viewer's ledger is a report: nothing to type into.
            if store.canWrite { newRowView.rowRule() }
            EmptyRowLines()
                .frame(height: max(2 * Metrics.rowHeight, viewportHeight - drawn))
        }
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
            envelopes: envelopes,
            onOpen: { field in open(row, at: field) },
            onCommit: { submit(row.id) },
            onCancel: cancelEditing,
            onDelete: trashAction(for: row, editing: isEditing)
        )
        .background(background(editing: isEditing, selected: isSelected, hovered: hovered == row.id))
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
    /// selection, a shade deeper under the pointer. The
    /// row being edited and the one under the pointer step up to the card
    /// ground; the focused cell carries its own ring (`cellFocusRing`).
    private func background(editing: Bool, selected: Bool, hovered: Bool) -> Color {
        if editing { return Ink.card }
        if selected { return Ink.accent.opacity(hovered ? 0.2 : 0.14) }
        return hovered ? Ink.card : Color.clear
    }

    /// "Delete" is what the window calls a void. The core's log is
    /// append-only and synced, so nothing is removed: the row stays in the
    /// history, struck through when the chip shows it and out of every total.
    /// "Annulla", the old Italian word for it, was also Cancel and the
    /// system's Undo.
    @ViewBuilder
    private func rowMenu(_ row: TransactionRow) -> some View {
        // A transfer's two ends do not fit the empty line, so it has no copy
        // to offer (`RowDraft.duplicate`).
        if !row.isTransfer {
            Button(String(localized: "Duplicate")) { duplicate(row) }
                .disabled(store.isReadOnly)
            Divider()
        }
        Button(String(localized: "Delete"), role: .destructive) {
            Task { await store.void(transactionId: row.id) }
        }
        .disabled(store.isReadOnly || row.voided)
    }

    /// The trash icon's action, for the row under the pointer only, as the
    /// SETUP tables show their archive icon: a sheet full of icons would be
    /// noise. None on the row being edited, whose last cell is being typed
    /// into, and none where the context menu's Delete is off: a vault this
    /// account only reads, a row deleted already. The due periods and the
    /// empty line are not rows and never get one.
    private func trashAction(for row: TransactionRow, editing: Bool) -> (() -> Void)? {
        guard hovered == row.id, !editing, store.canWrite, DeleteTarget.isDeletable(row) else { return nil }
        return { delete(row) }
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
                Button(String(localized: "Delete")) { Task { await store.void(transactionId: row.id) } }
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
        Button(String(localized: "Set Envelope\u{2026}")) { showsBulkEnvelope = true }
            .disabled(targets == 0)
        Divider()
        Button(String(localized: "Delete \(targets) Rows"), role: .destructive) {
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
            envelopes: envelopes,
            onCommit: commitNewRow,
            onCancel: { gridFocused = true },
            showsWallet: store.showWalletColumn
        )
            .background(focus?.row == nil && focus != nil ? Ink.card : Color.clear)
    }

    // MARK: - The envelope list

    /// The ENVELOPE cell's list, hung from the bottom of its row, or from the
    /// top when there is no room below (the empty line, a row at the bottom
    /// of the window), as the CATEGORY cell's is (`hangingCategoryList`) and
    /// for the same reasons: drawn over the whole grid, outside the scroll
    /// view, so no later row covers it, the scroll view does not clip it, and
    /// a click on it is not a click on the grid that would close the row.
    @ViewBuilder
    private func envelopeList(at anchor: Anchor<CGRect>?) -> some View {
        if let anchor, envelopes.isOpen {
            GeometryReader { proxy in
                let cell = proxy[anchor]
                let height = EnvelopeChoiceList.height(for: envelopes.choices.count)
                let fitsBelow = cell.maxY + height <= proxy.size.height
                EnvelopeChoiceList(choices: envelopes)
                    .offset(
                        x: cell.minX - GridColumn.padding + 3,
                        y: fitsBelow ? cell.maxY - 1 : cell.minY - height + 1
                    )
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
    /// same row do not conflict. A cell the parser or
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
                    kind: entry.kind,
                    person: entry.person
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

    /// The trash icon: the row alone, through the context menu's own path,
    /// so it is hidden at once and the toast can still put it back. ⌫ over
    /// the row takes the same path (`selectionKeys`).
    private func delete(_ row: TransactionRow) {
        Task { await store.void(transactionId: row.id) }
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
