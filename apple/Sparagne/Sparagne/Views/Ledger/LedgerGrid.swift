import AppKit
import SwiftUI
import SparagneCore

/// The spreadsheet: one row per transaction, oldest
/// first, the recurring periods due this month among them at their date
/// (`PendingRowView`), and an always-empty last line for the next one.
///
/// Edited as a spreadsheet is, a cell at a time (`LedgerEditor`, which the
/// store owns). One cell is selected, the cursor, drawn with an accent ring;
/// a click selects it, the arrows, ⇥ and ⇧⇥ move it. A key typed on it
/// replaces what it says and starts typing into it; ↩, F2 or a double click
/// type into it from what it says. Only that cell is a text field; every
/// other cell, the rest of its row included, is drawn as text.
///
/// The cell typed into is written the moment it is left, whichever way: ↩,
/// ⇥ or an arrow, a click on another cell or anywhere outside the grid, the
/// search field taking the keyboard. esc puts it back as it was. The empty
/// line is registered when the cursor leaves it with an amount in it.
///
/// ⌘-click and ⇧-click pick rows instead, and ⌘A picks them all while no
/// cell is being typed into: the selection the bulk actions work on
/// (`SelectionBar`, `AppStore+Selection.swift`). Picking hides the cursor.
/// With no cursor, ⌫ deletes what is picked, or with nothing picked the row
/// under the pointer, the one showing its trash icon (`DeleteTarget`); ⌘⌫
/// deletes the picked rows or the cursor's.
struct LedgerGrid: View {
    @Bindable var store: AppStore

    /// The row under the pointer, so the eye can follow it across a grid that
    /// is much wider than a line of text. Its trash icon shows, and ⌫ with
    /// nothing selected deletes it.
    @State private var hovered: Uuid?
    /// The lines as last worked out: the body runs again on every pointer
    /// move and keystroke, and the lines change only with their input.
    @State private var linesCache = LedgerLines.Cache()
    /// The cell being typed into, which is the editor's cursor while it is
    /// editing (`followEditor`).
    @FocusState private var focus: CellFocus?
    /// The grid as a whole, while a cell is selected and not typed into, or
    /// rows are picked: what the arrows, the typed keys, ↩, ⌘A, ⌫ and esc
    /// reach then (`selectionKeys`).
    @FocusState private var gridFocused: Bool
    /// The caret in the cell being typed into.
    @State private var caret: TextSelection?
    /// Typing has just started, and the caret is on its way to the end of
    /// what the cell says (`fieldKey`, `caretLanded`).
    @State private var placesCaret = false
    /// The caret was put at the end once the field had taken the keyboard:
    /// from then on the field's caret landing there is the grid's doing.
    @State private var caretSent = false
    /// Typing started from a key pressed on the selected cell
    /// (`EditedCell.typed`).
    @State private var typed = false
    /// Counts the times the cursor's line was asked back into sight by a key
    /// typed while its field was out of it (`gridKey`).
    @State private var cursorWanted = 0
    /// Where a click can land, for the click watcher to tell the grid's cells
    /// from the rest of the window.
    @State private var frames = GridFrames()
    /// "Set Category…", from the selection bar or a selected row's menu.
    @State private var showsBulkCategory = false
    /// "Set Envelope…", the same way.
    @State private var showsBulkEnvelope = false
    /// The list under the CATEGORY cell being typed into: one for the whole
    /// grid, drawn over it rather than inside a row (`hangingCategoryList`).
    @State private var completion = CategoryCompletionModel()
    /// The list under the ENVELOPE cell being typed into, drawn the same way
    /// (`envelopeList`).
    @State private var envelopes = EnvelopeChoiceModel()

    /// The grid's own coordinates, which the click watcher measures in.
    nonisolated private static let space = "ledger.grid"
    /// The empty line's identity among the lines, for the scroll to find it.
    nonisolated private static let newLineID = "ledger.new-line"

    private var editor: LedgerEditor { store.ledgerEditor }

    var body: some View {
        VStack(spacing: 0) {
            GridHeader(store: store, showsWallet: store.showWalletColumn)
                .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
            GeometryReader { viewport in
                ScrollViewReader { proxy in
                    ScrollView {
                        lines(viewportHeight: viewport.size.height)
                            .onGeometryChange(for: CGRect.self) { $0.frame(in: .named(Self.space)) } action: {
                                frames.content = $0
                            }
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
                    // The cursor's line stays in sight, as ↑ ↓ ⇥ take it
                    // along the sheet and as typing starts on it.
                    .onChange(of: editor.cursor) { _, _ in scrollToCursor(with: proxy) }
                    .onChange(of: editor.isEditing) { _, editing in
                        if editing { scrollToCursor(with: proxy) }
                    }
                    .onChange(of: cursorWanted) { _, _ in scrollToCursor(with: proxy) }
                }
            }
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .named(Self.space)) } action: {
                frames.viewport = $0
            }
            // With rows picked, a line under the sheet says what can be done
            // to all of them at once.
            if store.canWrite, store.selection.count > 1 {
                SelectionBar(store: store, showsCategory: $showsBulkCategory, showsEnvelope: $showsBulkEnvelope)
            }
        }
        // Three points in from the cell's edge, as the canvas hangs it (`.ac`).
        .hangingCategoryList(completion, inset: 3 - GridColumn.padding) { frames.categoryList = $0 }
        .overlayPreferenceValue(EnvelopeChoiceAnchor.self) { anchor in
            envelopeList(at: anchor)
        }
        .background(Ink.sheet)
        .background {
            GridClickWatcher(
                frames: frames,
                isWatching: editor.isEditing,
                onClick: { clickLanded(at: $0) },
                onKeyboardLost: keyboardLost
            )
        }
        .coordinateSpace(.named(Self.space))
        // The month, the lists, the tab, the vault and its role, the WALLET
        // column and the author are the store's to tell the editor about
        // (`LedgerEditor.finish`, `reset`, `walletColumnChanged`): the grid
        // only follows where the cursor ends up.
        .onChange(of: editor.isEditing) { _, _ in followEditor() }
        .onChange(of: editor.cursor) { _, _ in followEditor() }
        .onChange(of: focus) { old, new in focusMoved(from: old, to: new) }
        .onAppear {
            followEditor()
            // ⌘A works from the start, before any cell was clicked.
            if editor.cursor == nil { gridFocused = true }
        }
        .onReceive(NotificationCenter.default.publisher(for: NSTextView.didChangeSelectionNotification)) {
            caretLanded($0)
        }
        .task(id: editor.newLineNote) { await suggestCategory() }
        .onReceive(NotificationCenter.default.publisher(for: .duplicateLastRow)) { _ in
            duplicateLast()
        }
    }

    // MARK: - The keys of the grid

    /// Where the keyboard goes while no cell is being typed into: a selected
    /// cell's keys (the arrows, ⇥, ↩, F2, esc, a typed character, ⌫), and
    /// those of the rows picked (⌘A, ⌫, esc).
    ///
    /// An invisible view behind the rows, not the scroll view itself: a
    /// focusable container would take the focus on every click inside it,
    /// before the click reached a cell, and so take the caret from the cell
    /// being typed into under the pointer. Nothing here can be clicked, only
    /// focused from code (`followEditor`, `pick`). Since no cell is inside
    /// it, a key typed into a cell never reaches it; the keys typed on a
    /// selected cell faster than its field takes the caret do, and are added
    /// to what was typed (`LedgerEditor.type`).
    private var selectionKeys: some View {
        Color.clear
            .focusable()
            .focusEffectDisabled()
            .focused($gridFocused)
            // Menu's Select All, which a focused text field would take first.
            .onCommand(#selector(NSResponder.selectAll(_:))) { selectAll() }
            .onKeyPress(phases: [.down, .repeat]) { gridKey($0) }
            .accessibilityHidden(true)
    }

    private func gridKey(_ press: KeyPress) -> KeyPress.Result {
        let key = press.key
        // Typing into a cell whose field is not on screen: its row comes
        // back into sight, and its field with it.
        if editor.isEditing, focus == nil { cursorWanted += 1 }
        if GridKey.isDelete(key) {
            if press.modifiers.contains(.command) { return commandDelete() }
            guard press.modifiers.isDisjoint(with: [.option, .control, .shift]) else { return .ignored }
            if editor.isEditing {
                // A ⌫ typed before the field took the caret.
                if !editor.text.isEmpty { editor.text.removeLast() }
                return .handled
            }
            if editor.cursor != nil {
                typeOnCursor("")
                return .handled
            }
            return deleteRows()
        }
        if key == .escape {
            let handled = editor.cursor != nil || !store.selection.isEmpty
            if editor.cursor != nil { editor.cancel() }
            store.clearSelection()
            return handled ? .handled : .ignored
        }
        guard editor.cursor != nil else { return .ignored }
        if let move = GridKey.move(for: press) {
            editor.move(move)
            return .handled
        }
        if key == .return, GridKey.isPlain(press) {
            if editor.isEditing {
                // A ↩ typed before the field took the caret.
                editor.confirm()
            } else {
                edit(nil)
            }
            return .handled
        }
        if key == GridKey.f2 {
            if !editor.isEditing { edit(nil) }
            return .handled
        }
        if GridKey.isTyping(press) {
            typeOnCursor(press.characters)
            return .handled
        }
        return .ignored
    }

    /// The keys a cell being typed into leaves to the grid (`cellKeys`):
    /// ⇥ ⇧⇥ ↑ ↓ save it and move, esc puts it back. ← → move the caret.
    ///
    /// The field selects all it holds as it takes the caret, and the update
    /// that puts the caret back after it can come a moment after the next
    /// key: until the caret is there (`caretLanded`), a typed key is added to
    /// the editor's text here rather than given to the field to replace what
    /// it has selected (`GridEventMechanicsTests`). Any other key, ← or ⌫,
    /// is the field's, and from then on so is every key.
    private func fieldKey(_ press: KeyPress) -> KeyPress.Result {
        if let move = GridKey.move(for: press), move != .left, move != .right {
            editor.move(move)
            return .handled
        }
        if press.key == .escape {
            editor.cancel()
            return .handled
        }
        guard placesCaret else { return .ignored }
        guard GridKey.isTyping(press) else {
            placesCaret = false
            return .ignored
        }
        editor.type(press.characters)
        caret = caretAtEnd
        return .handled
    }

    /// The field's caret moved. Once it stands at the end of what the cell
    /// says, after the grid put it there, the field has taken over and the
    /// keys typed from then on are its own, wherever the caret goes next.
    /// The field reports it as AppKit's text view does, which is where the
    /// caret really is, not where the grid asked for it.
    private func caretLanded(_ notification: Notification) {
        guard placesCaret, caretSent, let field = notification.object as? NSTextView,
              let window = frames.window, field.window === window, window.firstResponder === field,
              field.string == editor.text
        else { return }
        let selection = field.selectedRange()
        if selection.length == 0, selection.location == (field.string as NSString).length {
            placesCaret = false
        }
    }

    /// The caret goes to the end of what the cell says, once its field has
    /// the keyboard (`focusMoved`).
    private func placeCaret() {
        placesCaret = true
        caretSent = false
        caret = caretAtEnd
    }

    /// A key typed on the selected cell: it replaces what the cell says, or
    /// with "" (⌫) empties it, and the cell's field takes the caret after
    /// it. A cell that cannot be typed into beeps.
    private func typeOnCursor(_ characters: String) {
        let starting = !editor.isEditing
        guard editor.type(characters) else {
            NSSound.beep()
            return
        }
        if starting { typed = true }
        placeCaret()
    }

    /// ↩, F2, a double click or VoiceOver's Edit: `cell` (the cursor's, when
    /// `nil`) is typed into from what it says, the caret after it.
    ///
    /// Refused, it beeps when the cell cannot be typed into, and then it is
    /// selected; a cell being left that refused to be saved has said why
    /// already, and keeps the caret.
    private func edit(_ cell: CellFocus?) {
        let target = cell ?? editor.cursor
        guard editor.edit(cell) else {
            if editor.cursor == target, !editor.isEditing { NSSound.beep() }
            return
        }
        typed = false
        placeCaret()
    }

    private var caretAtEnd: TextSelection { TextSelection(insertionPoint: editor.text.endIndex) }

    /// What ⌫ deletes with no cursor. The hovered row is looked up among the
    /// rows on screen: one just deleted is gone from them while the pointer
    /// has not moved yet, and a second ⌫ must find nothing there.
    private func deleteRows() -> KeyPress.Result {
        let target = DeleteTarget.resolve(
            selection: store.selectedRows.map(\.id),
            hovered: hovered.flatMap { id in store.rows.first { $0.id == id } },
            editing: editor.isEditing,
            canWrite: store.canWrite
        )
        return run(target)
    }

    /// ⌘⌫: the rows picked, or with none the row the cursor is on.
    private func commandDelete() -> KeyPress.Result {
        let target = DeleteTarget.resolveCommandDelete(
            selection: store.selectedRows.map(\.id),
            cursorRow: editor.cursor?.row.flatMap { id in store.rows.first { $0.id == id } },
            editing: editor.isEditing,
            canWrite: store.canWrite
        )
        return run(target)
    }

    private func run(_ target: DeleteTarget) -> KeyPress.Result {
        switch target {
        case .selection:
            Task { await store.voidSelection() }
        case .row(let id):
            Task { await store.void(transactionId: id) }
        case .none:
            return .ignored
        }
        return .handled
    }

    // MARK: - The focus

    /// The keyboard follows the editor: the cell typed into has the caret,
    /// after what it says; with a cell only selected the grid has the keys;
    /// with no cursor the keyboard stays where it is, the search field
    /// included.
    private func followEditor() {
        if editor.isEditing, let cursor = editor.cursor {
            guard focus != cursor else { return }
            placeCaret()
            focus = cursor
        } else {
            // The field goes with the editing, and the caret with it: setting
            // `focus` to nil here could take it from the search field a click
            // has just given it to.
            placesCaret = false
            typed = false
            if editor.cursor != nil { gridFocused = true }
        }
    }

    /// The field takes the caret selecting all it holds: the caret goes back
    /// after what was typed.
    ///
    /// The field losing the keyboard while it is still typed into is one of
    /// two things. Another field took it, the search field or ⌘K's: the cell
    /// is saved, as a click away from it saves it; one that refuses stays as
    /// typed, its error shown, until it is come back to, and the other field
    /// keeps the keyboard. Or nothing took it: the row went out of the lazy
    /// sheet as it scrolled, and the field with it (which SwiftUI does not
    /// always report: `keyboardLost` hears it from the window). The cell is
    /// still typed into; the grid takes the keys meanwhile, which go on
    /// adding to it and bring its row back into sight, and the field takes
    /// them back as it comes into sight (`fieldAppeared`).
    private func focusMoved(from old: CellFocus?, to new: CellFocus?) {
        if new != nil {
            // Typing into a cell is editing, not picking: one or the other.
            store.clearSelection()
            if new == editor.cursor, placesCaret {
                caret = caretAtEnd
                caretSent = true
            }
        }
        guard let old, editor.isEditing, editor.cursor == old, new != old else { return }
        if frames.window?.firstResponder is NSText {
            // A click there already saved the cell, or was refused, on its
            // way (`clickLanded`): once is enough to say why. The focus is
            // told as the click's mouse-up is handled.
            let clicked = [.leftMouseDown, .leftMouseUp].contains(NSApp.currentEvent?.type)
            if !clicked { editor.finish() }
        } else {
            gridFocused = true
        }
    }

    /// The window's keyboard went to no view, the field of the cell being
    /// typed into gone with its row (`focusMoved`): the grid takes the keys.
    private func keyboardLost() {
        guard editor.isEditing else { return }
        gridFocused = true
    }

    /// The field of the cell being typed into came into sight, its row back
    /// in the lazy sheet: while the grid holds the keys for it
    /// (`focusMoved`), it takes them back, the caret at the end.
    private func fieldAppeared(_ cell: CellFocus) {
        guard editor.isEditing, editor.cursor == cell, focus != cell, gridFocused else { return }
        placeCaret()
        focus = cell
    }

    // MARK: - The clicks

    /// A click on a cell: ⌘-click and ⇧-click pick its row, a first click
    /// selects the cell, the second of a double click types into it. The
    /// cells only report a tap, so the count and the modifiers are read off
    /// the event being handled, the mouse-up of that click.
    private func click(_ cell: CellFocus) {
        let event = NSApp.currentEvent
        let modifiers = event?.modifierFlags ?? NSEvent.modifierFlags
        if let id = cell.row, modifiers.contains(.command) || modifiers.contains(.shift),
           let row = store.rows.first(where: { $0.id == id }) {
            pick(row, extending: !modifiers.contains(.command))
            return
        }
        store.clearSelection()
        if editor.isEditing, editor.cursor == cell { return }
        if (event?.clickCount ?? 1) >= 2 {
            edit(cell)
        } else {
            editor.select(cell)
        }
    }

    /// A click anywhere in the window while a cell is being typed into, seen
    /// before it lands (`GridClickWatcher`). On a cell, the cell's own click
    /// saves it and moves the cursor; on the CATEGORY or ENVELOPE list the
    /// list picks. Anywhere else, the search field, the tab bar, a due
    /// period, the ruled lines under the sheet, the cell is saved and the
    /// cursor goes. Every click goes on to what it was aimed at: a cell that
    /// refuses to be saved stays as typed, its error shown.
    ///
    /// Whatever it lands on, the caret is the field's from then on: a click
    /// inside it puts the caret where it fell (`fieldKey`).
    private func clickLanded(at point: CGPoint) {
        guard editor.isEditing else { return }
        placesCaret = false
        if frames.categoryList?.contains(point) == true || frames.envelopeList?.contains(point) == true { return }
        if isOnCell(point) { return }
        editor.finish()
    }

    /// Whether `point` is on a cell of a stored row or of the empty line,
    /// the # column included: the sheet's pitch is fixed, so the line under
    /// the point is known from how far down the lines it is. The trash
    /// icon's track, a due period and the ruled lines below are not cells.
    private func isOnCell(_ point: CGPoint) -> Bool {
        guard frames.viewport.contains(point) else { return false }
        let x = point.x - frames.content.minX
        let y = point.y - frames.content.minY
        guard x >= 0, x < frames.content.width - GridColumn.action, y >= 0 else { return false }
        let index = Int(y / Metrics.rowHeight)
        let lines = linesCache.lines
        if lines.indices.contains(index) {
            if case .row = lines[index] { return true }
            return false
        }
        return index == lines.count && store.canWrite
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

    /// Keeps the cursor's line in sight, the empty line included, moving
    /// the sheet by as little as it takes: a line already in sight does not
    /// move it.
    private func scrollToCursor(with proxy: ScrollViewProxy) {
        guard let cursor = editor.cursor else { return }
        if let id = cursor.row {
            guard let row = store.rows.first(where: { $0.id == id }) else { return }
            proxy.scrollTo(LedgerLine.row(row).id, anchor: nil)
        } else {
            proxy.scrollTo(Self.newLineID, anchor: nil)
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
        let edited = editedCell
        return LazyVStack(spacing: 0) {
            ForEach(lines) { line in
                switch line {
                case .row(let row):
                    let index = ordinals[row.id] ?? 0
                    rowView(index: index, row: row, edited: edited)
                        .rowRule()
                        .onAppear { if index == rows.count - 1 { loadNextPage() } }
                case .pending(let period):
                    PendingRowView(period: period, store: store, showsWallet: store.showWalletColumn)
                        .rowRule()
                }
            }
            // A viewer's ledger is a report: nothing to type into.
            if store.canWrite { newRowView(edited: edited).rowRule().id(Self.newLineID) }
            EmptyRowLines()
                .frame(height: max(2 * Metrics.rowHeight, viewportHeight - drawn))
        }
    }

    /// What the cell being typed into needs, the same for every line.
    private var editedCell: EditedCell {
        EditedCell(
            editor: editor,
            focus: $focus,
            selection: $caret,
            typed: typed,
            keys: fieldKey,
            appeared: fieldAppeared
        )
    }

    /// The cursor as the line `row` (nil = the empty line) sees it.
    private func cursor(on row: Uuid?) -> LineCursor? {
        guard let cursor = editor.cursor, cursor.row == row else { return nil }
        return LineCursor(field: cursor.field, editing: editor.isEditing)
    }

    // MARK: - Existing rows

    /// A stored row as the sheet draws it: with the cells saved into it that
    /// no reload has given back yet (`LedgerEditor.shown`), so a cell just
    /// left says what it was saved as at once. Only what is drawn and read
    /// out changes; the line keeps the stored row's place, and everything
    /// that finds a row, the cursor, the selection, the pointer, the reveal
    /// and the paging, finds it by its id.
    private func rowView(index: Int, row: TransactionRow, edited: EditedCell) -> some View {
        ShownRow(row: row, editor: editor) { shown in
            storedRow(index: index, row: shown, edited: edited)
        }
    }

    @ViewBuilder
    private func storedRow(index: Int, row: TransactionRow, edited: EditedCell) -> some View {
        let cursor = cursor(on: row.id)
        let isEditing = cursor?.editing == true
        let isSelected = store.selection.contains(row.id)
        LedgerRowView(
            ordinal: index + 1,
            row: row,
            store: store,
            showsWallet: store.showWalletColumn,
            cursor: cursor,
            edited: edited,
            completion: completion,
            envelopes: envelopes,
            onClick: { field in click(CellFocus(row: row.id, field: field)) },
            onDelete: trashAction(for: row, editing: isEditing)
        )
        .background(background(selected: isSelected, hovered: hovered == row.id))
        .onHover { inside in
            if inside {
                hovered = row.id
            } else if hovered == row.id {
                hovered = nil
            }
        }
        // VoiceOver reads a row as one sentence; while one of its cells is
        // typed into, as the cells, that one a field with its column's name.
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
    /// selection, a shade deeper under the pointer; the row under the
    /// pointer steps up to the card ground. The cursor's cell carries its
    /// own ring (`cellSelectionRing`, `cellFocusRing`).
    private func background(selected: Bool, hovered: Bool) -> Color {
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
    /// noise. None on the row whose cell is being typed into, and none where
    /// the context menu's Delete is off: a vault this account only reads, a
    /// row deleted already. The due periods and the empty line are not rows
    /// and never get one.
    private func trashAction(for row: TransactionRow, editing: Bool) -> (() -> Void)? {
        guard hovered == row.id, !editing, store.canWrite, DeleteTarget.isDeletable(row) else { return nil }
        return { delete(row) }
    }

    /// What VoiceOver offers on a row: the context menu's actions, the
    /// double click that types into its DESCRIZIONE, and the ⌘-click that
    /// picks it, since neither gesture is at hand without a pointer.
    @ViewBuilder
    private func rowActions(_ row: TransactionRow, selected: Bool) -> some View {
        if store.canWrite {
            if !row.voided {
                Button(String(localized: "Edit")) { edit(CellFocus(row: row.id, field: .note)) }
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

    private func newRowView(edited: EditedCell) -> some View {
        NewRowView(
            store: store,
            cursor: cursor(on: nil),
            edited: edited,
            completion: completion,
            envelopes: envelopes,
            showsWallet: store.showWalletColumn,
            onClick: { field in click(CellFocus(row: nil, field: field)) }
        )
    }

    // MARK: - The envelope list

    /// The ENVELOPE cell's list, hung from the bottom of its row, or from the
    /// top when there is no room below (the empty line, a row at the bottom
    /// of the window), as the CATEGORY cell's is (`hangingCategoryList`) and
    /// for the same reasons: drawn over the whole grid, outside the scroll
    /// view, so no later row covers it and the scroll view does not clip it.
    @ViewBuilder
    private func envelopeList(at anchor: Anchor<CGRect>?) -> some View {
        if let anchor, envelopes.isOpen {
            GeometryReader { proxy in
                let cell = proxy[anchor]
                let height = EnvelopeChoiceList.height(for: envelopes.choices.count)
                let fitsBelow = cell.maxY + height <= proxy.size.height
                let frame = CGRect(
                    x: cell.minX - GridColumn.padding + 3,
                    y: fitsBelow ? cell.maxY - 1 : cell.minY - height + 1,
                    width: FloatingList.width,
                    height: height
                )
                EnvelopeChoiceList(choices: envelopes)
                    .offset(x: frame.minX, y: frame.minY)
                    .onChange(of: frame, initial: true) { _, new in frames.envelopeList = new }
                    .onDisappear { frames.envelopeList = nil }
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

    /// ⌘-click toggles `row`, ⇧-click extends to it. The cell being typed
    /// into is saved first, as any click away from it saves it; one that
    /// refuses to be saved keeps the caret, and nothing is picked. The
    /// cursor goes, and the grid takes the keyboard, so ⌫ and esc reach the
    /// selection.
    private func pick(_ row: TransactionRow, extending: Bool) {
        guard store.canWrite, editor.finish() else { return }
        gridFocused = true
        if extending {
            store.extendSelection(to: row.id)
        } else {
            store.toggleSelection(row.id)
        }
    }

    /// ⌘A with no cell being typed into: every row on screen, the cursor
    /// gone as with any pick.
    private func selectAll() {
        guard !editor.isEditing, store.canWrite, editor.finish() else { return }
        store.selectAllRows()
        gridFocused = true
    }

    /// The empty line's note has been still for a moment: ask the core what
    /// the vault files it under, for the CATEGORY cell to show while it is
    /// empty. An answer that comes back after the note moved on is dropped.
    private func suggestCategory() async {
        let note = editor.newLineNote.trimmingCharacters(in: .whitespacesAndNewlines)
        guard store.canWrite, !note.isEmpty, editor.newLine.suggestion?.note != note else { return }
        guard (try? await Task.sleep(for: .milliseconds(250))) != nil else { return }
        guard let category = await store.suggestedCategory(forNote: note),
              editor.newLineNote.trimmingCharacters(in: .whitespacesAndNewlines) == note
        else { return }
        editor.newLine.suggestion = NoteSuggestion(note: note, category: category)
    }

    /// The trash icon: the row alone, through the context menu's own path,
    /// so it is hidden at once and the toast can still put it back. ⌫ over
    /// the row takes the same path (`selectionKeys`).
    private func delete(_ row: TransactionRow) {
        Task { await store.void(transactionId: row.id) }
    }

    /// ⌘D and the context menu: copy a row into the empty line, ready to be
    /// tweaked and saved, the caret in its IMPORTO.
    private func duplicate(_ row: TransactionRow) {
        guard !store.isReadOnly else { return }
        editor.duplicate(row)
    }

    /// ⌘D with nothing selected duplicates the last row of the month that is
    /// not a transfer (`AppStore.lastRow`).
    private func duplicateLast() {
        guard let last = store.lastRow else { return }
        duplicate(last)
    }
}

// MARK: - A row as shown

/// Hands `content` the row as the sheet shows it (`LedgerEditor.shown`). A
/// view of its own, so that a save redraws the row it was saved into, and
/// that row only.
private struct ShownRow<Content: View>: View {
    let row: TransactionRow
    let editor: LedgerEditor
    @ViewBuilder let content: (TransactionRow) -> Content

    var body: some View {
        content(editor.shown(row))
    }
}

// MARK: - Keys

/// What the grid makes of a key press, with no SwiftUI state in it.
enum GridKey {
    /// F2, which types into the selected cell as ↩ does.
    static let f2 = KeyEquivalent("\u{F705}")
    /// ⇧⇥, which arrives as its own character rather than as ⇥ with ⇧.
    static let backTab = KeyEquivalent("\u{19}")

    /// ⌫ arrives as U+007F, which a handler for any key gets as it is, not
    /// as `.delete`; ⌦ as `.deleteForward`.
    static func isDelete(_ key: KeyEquivalent) -> Bool {
        key == .delete || key == .deleteForward || key == KeyEquivalent("\u{7F}")
    }

    /// No modifier that changes what the key means: ⇧, ⌥, ⌃ or ⌘.
    static func isPlain(_ press: KeyPress) -> Bool {
        press.modifiers.isDisjoint(with: [.shift, .option, .control, .command])
    }

    /// The arrows and ⇥ with no modifier, and ⇧⇥.
    static func move(for press: KeyPress) -> CursorMove? {
        if press.key == backTab { return .backTab }
        if press.key == .tab {
            if press.modifiers.contains(.shift) { return .backTab }
            return press.modifiers.isDisjoint(with: [.option, .control, .command]) ? .tab : nil
        }
        guard isPlain(press) else { return nil }
        switch press.key {
        case .upArrow: return .up
        case .downArrow: return .down
        case .leftArrow: return .left
        case .rightArrow: return .right
        default: return nil
        }
    }

    /// A character to type: anything printable, with ⇧ or ⌥ if need be (`@`
    /// is ⌥ò on an Italian keyboard), but not a ⌘ or ⌃ shortcut, and not
    /// the private characters AppKit gives the arrows and function keys.
    static func isTyping(_ press: KeyPress) -> Bool {
        guard press.modifiers.isDisjoint(with: [.command, .control]), !press.characters.isEmpty else { return false }
        return press.characters.unicodeScalars.allSatisfy { scalar in
            !CharacterSet.controlCharacters.contains(scalar) && !(0xF700...0xF8FF).contains(scalar.value)
        }
    }
}

// MARK: - The click watcher

/// Where a click can land in the grid, in the grid's own coordinates, as last
/// laid out. A reference rather than state: the frames move as the sheet
/// scrolls, and nothing has to be drawn again for that.
final class GridFrames {
    /// The scroll view.
    var viewport = CGRect.zero
    /// The lines inside it, which move with the scroll.
    var content = CGRect.zero
    /// The CATEGORY and ENVELOPE lists while they are open.
    var categoryList: CGRect?
    var envelopeList: CGRect?
    /// The grid's window, whose first responder says where the keyboard
    /// went (`LedgerGrid.focusMoved`).
    weak var window: NSWindow?
}

/// Sees every left click in the grid's window before the window hands it
/// on, while `isWatching`, and tells `onClick` with the point in the grid's
/// coordinates; the click always goes on. A click on a scroll bar is the
/// scroll bar's and never told. It also tells `frames` which window the grid
/// is in, and `onKeyboardLost` when that window's keyboard goes to no view
/// at all.
private struct GridClickWatcher: NSViewRepresentable {
    let frames: GridFrames
    let isWatching: Bool
    let onClick: (CGPoint) -> Void
    let onKeyboardLost: () -> Void

    func makeNSView(context: Context) -> GridClickWatcherView {
        let view = GridClickWatcherView()
        view.frames = frames
        updateNSView(view, context: context)
        return view
    }

    func updateNSView(_ view: GridClickWatcherView, context: Context) {
        view.isWatching = isWatching
        view.onClick = onClick
        view.onKeyboardLost = onKeyboardLost
    }

    static func dismantleNSView(_ view: GridClickWatcherView, coordinator: ()) {
        view.stop()
    }
}

/// The watcher's view, behind the whole grid so its bounds are the grid's.
/// Measured from the top left, as SwiftUI measures.
final class GridClickWatcherView: NSView {
    var frames: GridFrames?
    var isWatching = false
    var onClick: ((CGPoint) -> Void)?
    /// The window's keyboard went to no view at all, while `isWatching`.
    var onKeyboardLost: (() -> Void)?
    private var monitor: Any?
    private var responder: NSKeyValueObservation?

    override var isFlipped: Bool { true }

    /// Never the target of a click itself.
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        frames?.window = window
        stop()
        guard window != nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            guard let self, self.isWatching, event.window === self.window else { return event }
            if self.isOnScroller(event) { return event }
            self.onClick?(self.convert(event.locationInWindow, from: nil))
            return event
        }
        // A field taken out of the window with its row, as the lazy sheet
        // scrolls, leaves the keyboard to the window itself, and SwiftUI's
        // focus does not say so. Told after the change, which comes in the
        // middle of a layout.
        responder = window?.observe(\.firstResponder) { [weak self] _, _ in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self, self.isWatching, let window = self.window,
                          window.firstResponder == nil || window.firstResponder === window
                    else { return }
                    self.onKeyboardLost?()
                }
            }
        }
    }

    func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        responder = nil
    }

    private func isOnScroller(_ event: NSEvent) -> Bool {
        guard let content = window?.contentView else { return false }
        // `hitTest` takes the point in the coordinates of the view's parent.
        let point = content.superview?.convert(event.locationInWindow, from: nil) ?? event.locationInWindow
        var view = content.hitTest(point)
        while let current = view {
            if current is NSScroller { return true }
            view = current.superview
        }
        return false
    }
}
