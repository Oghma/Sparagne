import SwiftUI
import SparagneCore

/// Column geometry, shared by the header, the rows and the empty line so the
/// three agree without a layout pass.
///
/// The widths are what a cell's content gets: the canvas's tracks
/// (`# 34 · Data 60 · Busta 84 · Categoria 124 · Descrizione · Wallet 84 ·
/// Persona 84 · Importo 96`) less the padding `GridCell` adds on both sides,
/// then a 28-point track after IMPORTO for the trash icon.
enum GridColumn {
    /// A cell's horizontal padding, on each side (`Metrics.cellPad`).
    static let padding: CGFloat = Metrics.cellPad
    static let ordinal: CGFloat = 34 - 2 * padding
    // The other columns are dragged wider or narrower in the header: their
    // widths are `AppStore.ledgerColumns` (`LedgerColumn`).
    /// The track after IMPORTO for the trash icon of the row under the
    /// pointer (`RowActionTrack`): the whole track, with no cell padding
    /// around it, just wide enough for the 20-point button.
    static let action: CGFloat = 28
}

/// The narrow track at the end of every line of the grid, where the row
/// under the pointer shows its trash icon (`RowDeleteButton`). The header,
/// the due periods and the empty line reserve it too, empty, so the icon
/// coming and going moves no column.
///
/// The width hangs on a clear fill with the content over it: a frame around
/// nothing at all, an empty line's track or a row with no icon showing, is
/// dropped from the layout and would take its 28 points with it.
struct RowActionTrack<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        Color.clear
            .frame(width: GridColumn.action)
            .overlay { content }
    }
}

extension RowActionTrack where Content == EmptyView {
    init() {
        content = EmptyView()
    }
}

/// One cell: fixed width, one line, the grid's padding. Shared with the
/// statement import's tables (`Views/Import`), ruled as the grid is.
struct GridCell<Content: View>: View {
    var width: CGFloat?
    var alignment: Alignment = .leading
    @ViewBuilder var content: Content

    @ViewBuilder
    var body: some View {
        if width == nil {
            // DESCRIZIONE takes what the other columns leave, and never more:
            // a tag and two buttons that do not fit are cut at the cell's
            // edge rather than pushing the row wider than the sheet, where
            // its columns would no longer line up with the header's.
            cell
                .frame(minWidth: 0, maxWidth: .infinity, alignment: alignment)
                .padding(.horizontal, GridColumn.padding)
                .clipped()
                .contentShape(Rectangle())
        } else {
            cell
                .frame(width: width, alignment: alignment)
                .padding(.horizontal, GridColumn.padding)
                // The whole column is the click target. SwiftUI does not
                // hit-test the transparent part of a frame, so without this a
                // short "Casa" or the "—" of an empty note would leave most of
                // its cell dead and the row would refuse to open.
                .contentShape(Rectangle())
        }
    }

    private var cell: some View {
        content
            .lineLimit(1)
            .truncationMode(.tail)
    }
}

/// The column headings: small, medium weight, `text3`, on the sheet's own
/// ground (`.r.hd`). The grid draws the stronger rule under them.
struct GridHeader: View {
    let store: AppStore
    /// The optional WALLET column, off by default.
    var showsWallet = false

    private var columns: LedgerColumnWidths { store.ledgerColumns }

    var body: some View {
        HStack(spacing: 0) {
            GridCell(width: GridColumn.ordinal, alignment: .trailing) {
                heading("#", spoken: String(localized: "Number"))
            }
            GridCell(width: columns[.date]) { heading(RowField.date.label) }
            GridCell(width: columns[.flow]) { heading(RowField.flow.label) }
            GridCell(width: columns[.category]) { heading(RowField.category.label) }
            GridCell { heading(RowField.note.label) }
            if showsWallet {
                GridCell(width: columns[.wallet]) { heading(RowField.wallet.label) }
            }
            GridCell(width: columns[.person]) { heading(RowField.person.label) }
            GridCell(width: columns[.amount], alignment: .trailing) { heading(RowField.amount.label) }
            RowActionTrack()
        }
        .frame(height: Metrics.headerHeight)
        .background(Ink.sheet)
        .overlay { ColumnResizeHandles(store: store, showsWallet: showsWallet) }
    }

    /// A column heading, announced as one, and `#` as a word.
    private func heading(_ text: String, spoken: String? = nil) -> some View {
        SectionLabel(text: text)
            .accessibilityLabel(spoken ?? text)
            .accessibilityAddTraits(.isHeader)
    }
}

/// What VoiceOver reads for a row of the grid: its cells as one sentence, in
/// the order the eye scans them, so the grid is walked a row at a time rather
/// than a cell at a time. The amount says its kind, which the grid shows only
/// by color.
enum LedgerAccessibility {
    static func label(for row: TransactionRow, showsWallet: Bool = false, locale: Locale = .autoupdatingCurrent) -> String {
        var parts = [
            row.occurredAt.formatted(Date.FormatStyle(date: .long, time: .omitted).locale(locale)),
            amount(row),
            row.category,
        ]
        if !row.note.isEmpty { parts.append(row.note) }
        if row.envelopeDisplay != TransactionRow.placeholder {
            parts.append(String(localized: "envelope \(row.envelopeDisplay)"))
        }
        // A wallet transfer's route is on screen whatever the column, in
        // the description's tag (`TransactionRow.transferRoute`).
        if showsWallet || row.kind == .transferWallet, row.walletDisplay != TransactionRow.placeholder {
            parts.append(String(localized: "wallet \(row.walletDisplay)"))
        }
        parts.append(person(row.person))
        if row.isOnBehalf { parts.append(recordedBy(row.recordedBy)) }
        if row.voided { parts.append(String(localized: "deleted")) }
        return parts.joined(separator: ", ")
    }

    /// "Persona, Elisa": the column's name before the value, as the open
    /// row's cells are read.
    static func person(_ name: String) -> String {
        String(localized: "Person, \(name)")
    }

    /// "registrata da Matteo": who typed a row that is for someone else, the
    /// same words the PERSONA cell shows on hover.
    static func recordedBy(_ name: String) -> String {
        String(localized: "recorded by \(name)")
    }

    /// A due period as one sentence: what it is, how much, when; the two
    /// buttons are the row's actions (`PendingRowView`).
    static func label(for period: DuePeriod, locale: Locale = .autoupdatingCurrent) -> String {
        let template = period.template
        var parts: [String] = []
        if let note = template.note, !note.isEmpty { parts.append(note) }
        if let category = template.category, !category.isEmpty { parts.append(category) }
        parts.append(LedgerMoney.amount(template.amount))
        if let day = CoreDate.localDay(period.date) {
            parts.append(day.formatted(Date.FormatStyle(locale: locale).day().month(.wide)))
        }
        parts.append(person(template.owner))
        return String(localized: "Recurring entry to confirm: \(parts.joined(separator: ", "))")
    }

    private static func amount(_ row: TransactionRow) -> String {
        let value = LedgerMoney.amount(row.absoluteAmount)
        return switch row.kind {
        case .expense: String(localized: "expense \(value)")
        case .income: String(localized: "income \(value)")
        case .refund: String(localized: "refund \(value)")
        case .transferWallet, .transferFlow: String(localized: "transfer \(value)")
        }
    }
}

// MARK: - Pieces of a row

/// The DATA column's `01 gio`: the day in the row's size, the weekday smaller
/// and fainter, so the column scans by number (`.dy`).
struct DayLabel: View {
    let date: Date
    var tint: Color = Ink.text
    var struck = false

    var body: some View {
        HStack(spacing: 4) {
            Text(LedgerDate.dayNumber(date))
                .font(Face.row)
                .foregroundStyle(tint)
            Text(LedgerDate.weekday(date))
                .font(Face.small)
                .foregroundStyle(Ink.text3)
        }
        .strikethrough(struck)
        .lineLimit(1)
    }
}

/// A small label after a description (`.tag`): "rimborso", "da confermare".
/// `opacity` is the tint's strength in the ground behind the text.
struct RowTag: View {
    let text: String
    let tint: Color
    var opacity = 0.12

    var body: some View {
        Text(text)
            .font(Face.ui(10, .semibold))
            .foregroundStyle(tint)
            .lineLimit(1)
            .padding(.horizontal, 5)
            .frame(height: 16)
            .background(tint.opacity(opacity), in: RoundedRectangle(cornerRadius: 4))
            // Its whole width before the text beside it gets any, and "…"
            // only once that text is gone: in a narrow cell it shortens
            // rather than spilling into the next column.
            .layoutPriority(1)
    }
}

/// The 18-point buttons inside a row (`.btn.sm`): a ghost for the quiet
/// choice, an accent outline for the one that writes.
struct RowButtonStyle: ButtonStyle {
    enum Tone { case ghost, accent, neutral }

    var tone: Tone = .neutral

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Face.label)
            .foregroundStyle(tone == .accent ? Ink.accent : Ink.text2)
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 7)
            .frame(height: 18)
            .background(tone == .ghost ? Color.clear : Ink.card, in: shape)
            .overlay(shape.strokeBorder(border, lineWidth: 1))
            .contentShape(shape)
            .opacity(configuration.isPressed ? 0.7 : 1)
    }

    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: 5) }

    private var border: Color {
        switch tone {
        case .ghost: .clear
        case .accent: Ink.accent.opacity(0.4)
        case .neutral: Ink.line2
        }
    }
}

/// The trash icon at the end of the row under the pointer, the way the
/// SETUP tables show their archive icon (`SetupIconButton`): one click
/// deletes the row with the undo toast, as its context menu does. Quiet in
/// `text3` until the pointer is on the icon itself, then red, the color of
/// what cannot be taken back once the toast is gone.
///
/// VoiceOver never reaches it inside a closed row, which it reads as one
/// sentence: the row's own Delete action is the way there.
struct RowDeleteButton: View {
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: "trash")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(hovering ? Ink.negative : Ink.text3)
                .frame(width: 20, height: 20)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .help(String(localized: "Delete Transaction"))
        .accessibilityLabel(String(localized: "Delete Transaction"))
    }
}

extension View {
    /// The cell being typed into (`.fbox`): a 20-point box on the raised
    /// ground with a 1.5-point accent ring inside, reaching 5 points into the
    /// cell's padding so the text does not move when the ring appears.
    func cellFocusRing(_ active: Bool) -> some View {
        padding(.horizontal, 5)
            .frame(height: 20)
            .background {
                if active { RoundedRectangle(cornerRadius: 4).fill(Ink.raised) }
            }
            .overlay {
                if active { RoundedRectangle(cornerRadius: 4).strokeBorder(Ink.accent, lineWidth: 1.5) }
            }
            .padding(.horizontal, -5)
    }

    /// The selected cell, the cursor, while it is not being typed into: the
    /// same ring in the same box, on the line's own ground, so the raised
    /// fill is what says the caret is in it.
    func cellSelectionRing(_ active: Bool) -> some View {
        padding(.horizontal, 5)
            .frame(height: 20)
            .overlay {
                if active { RoundedRectangle(cornerRadius: 4).strokeBorder(Ink.accent, lineWidth: 1.5) }
            }
            .padding(.horizontal, -5)
    }

    /// A tooltip only where there is something to say: most cells of a
    /// column have none, and should not raise an empty one.
    @ViewBuilder
    func help(ifAny text: String?) -> some View {
        if let text { help(text) } else { self }
    }

    /// The hairline under every line of the grid, inside its 24 points so the
    /// rows keep the pitch the empty lines below them are drawn at.
    func rowRule() -> some View {
        overlay(alignment: .bottom) {
            Rectangle().fill(Ink.rowLine).frame(height: 1)
        }
    }
}

/// Below the last line, the sheet goes on as empty ruled lines, as a
/// spreadsheet does (`.fill`): the grid reads as a sheet, not as a list
/// that ran out.
struct EmptyRowLines: View {
    var body: some View {
        Canvas { context, size in
            var y = Metrics.rowHeight - 1
            while y < size.height {
                context.fill(Path(CGRect(x: 0, y: y, width: size.width, height: 1)), with: .color(Ink.rowLine))
                y += Metrics.rowHeight
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

// MARK: - The cursor on a line

/// Where the grid's cursor is on a line, for the line it is on: the cell
/// selected, and whether it is being typed into. Every other line gets `nil`
/// and draws no ring.
struct LineCursor: Equatable {
    let field: RowField
    let editing: Bool
}

/// A cell as the sheet draws it while it is not being typed into, which is
/// every cell but one: its text, the whole column wide, with the cursor's
/// ring when it is the selected cell (`cellSelectionRing`). A click selects
/// it and a second one types into it (`LedgerGrid.click`).
struct SheetCell<Content: View>: View {
    var width: CGFloat?
    var alignment: Alignment = .leading
    let selected: Bool
    let onClick: () -> Void
    @ViewBuilder var content: Content

    var body: some View {
        GridCell(width: width, alignment: alignment) {
            content
                .frame(maxWidth: .infinity, alignment: alignment)
                .cellSelectionRing(selected)
        }
        .onTapGesture(perform: onClick)
    }
}

// MARK: - An existing row

/// A stored row, drawn as text: every cell of it, including the rest of the
/// row whose cell is being typed into, which alone is a field. A click on a
/// cell selects it, two type into it (`LedgerGrid.click`); the keys that
/// move the cursor and start typing are the grid's.
struct LedgerRowView: View {
    let ordinal: Int
    let row: TransactionRow
    @Bindable var store: AppStore
    /// Draws the WALLET column when the View menu is showing it.
    var showsWallet = false
    /// The cursor, while it is on this row.
    let cursor: LineCursor?
    /// What the cell being typed into needs; only that cell reads it.
    let edited: EditedCell
    /// The grid's one completion list, for the CATEGORY cell.
    let completion: CategoryCompletionModel
    /// The grid's one envelope list, for the ENVELOPE cell.
    let envelopes: EnvelopeChoiceModel
    /// A click on a cell of the row.
    let onClick: (RowField) -> Void
    /// Deletes the row from its trash icon. Non-nil only while the icon
    /// shows: the row is under the pointer and may be deleted alone
    /// (`LedgerGrid.trashAction`).
    var onDelete: (() -> Void)?

    private var columns: LedgerColumnWidths { store.ledgerColumns }

    var body: some View {
        HStack(spacing: 0) {
            GridCell(width: GridColumn.ordinal, alignment: .trailing) {
                Text(ordinal, format: .number.grouping(.never))
                    .font(Face.small)
                    .foregroundStyle(Ink.text3)
            }
            // # holds no field of its own, but a click anywhere on a line
            // should land on it; DESCRIZIONE is the widest cell and the one
            // most often retyped, so it takes the cursor.
            .onTapGesture { onClick(.note) }
            dateCell
            envelopeCell
            categoryCell
            noteCell
            if showsWallet { walletCell }
            personCell
            amountCell
            RowActionTrack {
                if let onDelete { RowDeleteButton(action: onDelete) }
            }
        }
        .frame(height: Metrics.rowHeight)
        .contentShape(Rectangle())
    }

    private func key(_ field: RowField) -> CellFocus { CellFocus(row: row.id, field: field) }

    /// The editor only ever types into a cell that can be typed into
    /// (`LedgerCursor.isEditable`), so a field here is always one.
    private func isEdited(_ field: RowField) -> Bool { cursor == LineCursor(field: field, editing: true) }

    private func isSelected(_ field: RowField) -> Bool { cursor == LineCursor(field: field, editing: false) }

    /// `field`'s cell: `editor` while it is typed into, which a cell that is
    /// not `editable` never is; `shown` otherwise.
    @ViewBuilder
    private func cell<Editor: View, Shown: View>(
        _ field: RowField,
        width: CGFloat?,
        alignment: Alignment = .leading,
        editable: Bool = true,
        @ViewBuilder editor: () -> Editor,
        @ViewBuilder shown: () -> Shown
    ) -> some View {
        if editable, isEdited(field) {
            GridCell(width: width, alignment: alignment, content: editor)
        } else {
            SheetCell(width: width, alignment: alignment, selected: isSelected(field), onClick: { onClick(field) }, content: shown)
        }
    }

    private var dateCell: some View {
        cell(.date, width: columns[.date]) {
            CellField(field: .date, key: key(.date), edited: edited)
        } shown: {
            DayLabel(date: row.occurredAt, tint: ink(Ink.text), struck: row.voided)
        }
    }

    /// A transfer's envelopes are its two ends, as its wallets are
    /// (`walletCell`): the cell can be selected, never typed into.
    private var envelopeCell: some View {
        cell(.flow, width: columns[.flow], editable: !row.isTransfer) {
            EnvelopeSelectCell(placeholder: row.envelopeDisplay, store: store, choices: envelopes, key: key(.flow), edited: edited)
        } shown: {
            text(row.envelopeColumn, Ink.text2)
        }
    }

    private var categoryCell: some View {
        cell(.category, width: columns[.category]) {
            CategoryCell(placeholder: "", store: store, completion: completion, key: key(.category), edited: edited)
        } shown: {
            text(row.category, categoryTint)
        }
    }

    private var noteCell: some View {
        cell(.note, width: nil) {
            CellField(field: .note, key: key(.note), edited: edited)
        } shown: {
            HStack(spacing: 6) {
                if row.note.isEmpty {
                    text(TransactionRow.placeholder, Ink.text3)
                } else {
                    text(row.note, Ink.text)
                }
                if row.kind == .refund {
                    RowTag(text: String(localized: "refund"), tint: Ink.positive)
                }
                if let route = row.transferRoute {
                    RowTag(text: route, tint: ink(Ink.text2))
                }
            }
        }
    }

    /// A transfer's wallets are its two ends, not a field: `UpdateTransaction`
    /// moves them with `from_id`/`to_id`, which the grid does not edit, so the
    /// cell can be selected, never typed into.
    private var walletCell: some View {
        cell(.wallet, width: columns[.wallet], editable: !row.isTransfer) {
            CellField(field: .wallet, key: key(.wallet), edited: edited)
        } shown: {
            text(row.walletDisplay, Ink.text2)
        }
    }

    /// Who typed a row recorded for someone else is on hover only: the
    /// column stays one short name wide. A transfer is always its author's,
    /// so its cell is never typed into.
    private var personCell: some View {
        cell(.person, width: columns[.person], editable: row.isPersonEditable) {
            CellField(
                field: .person,
                key: key(.person),
                edited: edited,
                hint: row.isOnBehalf ? LedgerAccessibility.recordedBy(row.recordedBy) : ""
            )
        } shown: {
            text(row.person, Ink.text2)
        }
        .help(ifAny: row.isOnBehalf ? Self.recordedByHelp(row.recordedBy) : nil)
    }

    private var amountCell: some View {
        cell(.amount, width: columns[.amount], alignment: .trailing) {
            CellField(field: .amount, key: key(.amount), edited: edited, alignment: .trailing)
        } shown: {
            text(Self.amountText(row), amountTint)
        }
    }

    private func text(_ value: String, _ tint: Color) -> some View {
        Text(value)
            .font(Face.row)
            .foregroundStyle(ink(tint))
            .strikethrough(row.voided)
    }

    /// A voided row is all `text3`: still legible, plainly not counted.
    private func ink(_ tint: Color) -> Color { row.voided ? Ink.text3 : tint }

    /// "Senza categoria" is the absence of a choice, so it is drawn faint.
    private var categoryTint: Color {
        row.category == String(localized: "Uncategorized") ? Ink.text3 : Ink.text
    }

    /// The figure without the symbol, as the canvas writes the column. A
    /// refund carries a minus: it takes money off the month's expenses.
    static func amountText(_ row: TransactionRow) -> String {
        let bare = LedgerMoney.bare(row.absoluteAmount)
        return row.kind == .refund ? "\u{2212}\(bare)" : bare
    }

    /// Money coming in is green: income, and a refund in the USCITE list.
    /// Everything else is the ledger's default ink: in a month filtered to
    /// USCITE every row is an expense, so painting them all red would say
    /// nothing.
    private var amountTint: Color {
        switch row.kind {
        case .income, .refund: Ink.positive
        case .expense: Ink.text
        case .transferWallet, .transferFlow: Ink.text3
        }
    }

    /// "Registrata da Matteo", the tooltip of a row recorded for someone else.
    static func recordedByHelp(_ name: String) -> String {
        String(localized: "Recorded by \(name)")
    }
}

// MARK: - The cell being typed into

/// What the one cell being typed into needs from the grid: the editor its
/// text lives in (`LedgerEditor.text`, and `flowId` for ENVELOPE), the
/// grid's focus and the caret, and the grid's handler for the keys the cell
/// does not take for itself: ⇥ ⇧⇥ ↑ ↓ esc, and the keys typed while the
/// caret was still on its way into the field (`LedgerGrid.fieldKey`).
///
/// One value for the whole grid, handed to every line: only the line the
/// cursor is on reads it, in the one cell that is a field.
struct EditedCell {
    let editor: LedgerEditor
    let focus: FocusState<CellFocus?>.Binding
    let selection: Binding<TextSelection?>
    /// Typing started from a key pressed on the selected cell, rather than
    /// from ↩, F2 or a double click: the CATEGORY cell opens its list on
    /// what was typed.
    let typed: Bool
    let keys: (KeyPress) -> KeyPress.Result
    /// The field came into sight (`LedgerGrid.fieldAppeared`).
    let appeared: (CellFocus) -> Void

    var text: Binding<String> { Bindable(editor).text }
    var flowId: Binding<Uuid?> { Bindable(editor).flowId }
}

extension View {
    /// The keys every cell being typed into shares: ↩ saves it and keeps it
    /// selected, and what the cell leaves goes to the grid
    /// (`EditedCell.keys`). The field also tells the grid as it comes into
    /// sight, which gives it the keyboard back after a scroll took it away.
    ///
    /// SwiftUI gives a key to the handler applied last first, the one
    /// furthest from the field: the CATEGORY and ENVELOPE cells apply their
    /// lists' keys after these, so that a list, while open, takes ↑ ↓ ↩ ⇥
    /// and esc before the grid does (`GridEventMechanicsTests`).
    func cellKeys(_ edited: EditedCell, key: CellFocus) -> some View {
        onSubmit { edited.editor.confirm() }
            .onKeyPress(phases: [.down, .repeat], action: edited.keys)
            .onAppear { edited.appeared(key) }
    }
}

/// The cell being typed into, for the columns that are plain text:
/// DESCRIZIONE, WALLET, PERSONA, IMPORTO, and DATA, typed into from the
/// bare day number and saved as `29`, `29/8` or `29/8/2026`; a day alone
/// stays inside the month on screen, and text that means no date leaves the
/// day alone (`LedgerDate.parseDay`). Not typed into, DATA is `DayLabel`,
/// which a text field cannot draw.
///
/// `prompt` is what an empty cell says: the empty line's placeholders.
struct CellField: View {
    let field: RowField
    let key: CellFocus
    let edited: EditedCell
    var prompt = ""
    var alignment: TextAlignment = .leading
    var tint: Color = Ink.text
    /// What VoiceOver adds after the field's name and value.
    var hint = ""

    var body: some View {
        textField
            .textFieldStyle(.plain)
            .font(Face.row)
            .multilineTextAlignment(alignment)
            .foregroundStyle(tint)
            .accessibilityLabel(field.label)
            .accessibilityHint(hint)
            .focused(edited.focus, equals: key)
            .cellFocusRing(true)
            .cellKeys(edited, key: key)
    }

    @ViewBuilder
    private var textField: some View {
        if alignment == .trailing {
            // A title, not a prompt: a prompt ignores the trailing alignment
            // and would sit at the cell's left edge. Passed as a string so it
            // is shown as is and not looked up in the catalog.
            TextField(prompt, text: edited.text, selection: edited.selection)
        } else {
            TextField("", text: edited.text, selection: edited.selection, prompt: Text(prompt).foregroundStyle(Ink.text3))
        }
    }
}

// MARK: - A due recurring period

/// A recurring period that fell due this month, at its date among the rows:
/// hatched, in `text2`, with "Salta" and "Registra"
/// in the DESCRIZIONE cell. Nothing is written until one of the two is
/// pressed; a template never writes a transaction by itself.
///
/// Not a row: it has no number, cannot be opened, picked or summed, and the
/// status line's figures leave it out (`SheetStats`).
struct PendingRowView: View {
    let period: DuePeriod
    let store: AppStore
    var showsWallet = false

    /// A command in flight. The buttons wait for it: a double click would
    /// execute the same period twice, and the core would refuse the second.
    @State private var working = false

    private var template: RecurringView { period.template }

    private var columns: LedgerColumnWidths { store.ledgerColumns }

    var body: some View {
        HStack(spacing: 0) {
            GridCell(width: GridColumn.ordinal, alignment: .trailing) {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 10.5, weight: .medium))
                    .foregroundStyle(Ink.text3)
            }
            GridCell(width: columns[.date]) {
                DayLabel(date: CoreDate.localDay(period.date) ?? store.month.start(), tint: Ink.text2)
            }
            GridCell(width: columns[.flow]) { Text(store.envelopeName(template.flowId)) }
            GridCell(width: columns[.category]) { Text(category) }
            GridCell {
                HStack(spacing: 6) {
                    // The note and its tag while both fit, the note alone
                    // when they do not: the hatching and the two buttons
                    // already say the period is waiting. Offered the room
                    // before the spacer, or it would get only half of it.
                    ViewThatFits(in: .horizontal) {
                        HStack(spacing: 6) {
                            note
                            RowTag(text: String(localized: "to confirm"), tint: Ink.accent)
                        }
                        note
                    }
                    .layoutPriority(1)
                    Spacer(minLength: 6)
                    if !store.isReadOnly {
                        Button(String(localized: "Skip")) { run { await store.skipRecurring(template.id, periodDate: period.date) } }
                            .buttonStyle(RowButtonStyle(tone: .ghost))
                        // Off while the owner is someone who left the vault:
                        // the server would refuse the row. The row's style
                        // does not dim by itself, as the chrome's does.
                        Button(String(localized: "Record")) { run { await store.executeRecurring(template.id, periodDate: period.date) } }
                            .buttonStyle(RowButtonStyle(tone: .accent))
                            .disabled(ownerLeft)
                            .opacity(ownerLeft ? 0.4 : 1)
                            .help(ifAny: ownerLeft ? AppStore.ownerLeftExplanation : nil)
                    }
                }
                .disabled(working)
            }
            if showsWallet {
                GridCell(width: columns[.wallet]) { Text(store.walletName(template.walletId)) }
            }
            // The template's owner: the row is theirs, whoever records it.
            GridCell(width: columns[.person]) {
                Text(template.owner)
                    .foregroundStyle(ownerLeft ? Ink.negative : Ink.text3)
                    .help(ifAny: ownerLeft ? AppStore.noLongerAMember : nil)
            }
            GridCell(width: columns[.amount], alignment: .trailing) {
                Text(LedgerMoney.bare(template.amount))
            }
            RowActionTrack()
        }
        .font(Face.row)
        .foregroundStyle(Ink.text2)
        .frame(height: Metrics.rowHeight)
        .background(HatchedFill())
        // One sentence and two actions for VoiceOver, as a stored row reads
        // as one sentence with its menu's actions.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(LedgerAccessibility.label(for: period))
        .accessibilityActions {
            if !store.isReadOnly, !working {
                if !ownerLeft {
                    Button(String(localized: "Record")) { run { await store.executeRecurring(template.id, periodDate: period.date) } }
                }
                Button(String(localized: "Skip")) { run { await store.skipRecurring(template.id, periodDate: period.date) } }
            }
        }
    }

    @ViewBuilder
    private var note: some View {
        if let note = template.note, !note.isEmpty {
            Text(note)
        }
    }

    /// The template's owner left the vault: the period can be skipped, not
    /// recorded (`AppStore.ownerHasLeft`).
    private var ownerLeft: Bool { store.ownerHasLeft(template) }

    /// Free text on the template, resolved when it is registered; none is
    /// Uncategorized, as a blank cell is.
    private var category: String {
        guard let category = template.category, !category.isEmpty else { return String(localized: "Uncategorized") }
        return category
    }

    private func run(_ work: @escaping @MainActor () async -> Void) {
        working = true
        Task {
            await work()
            working = false
        }
    }
}

// MARK: - The empty line

/// Always the last line of the grid, filled cell by cell as any other line
/// and registered as the cursor leaves it with an amount in it
/// (`LedgerEditor.newLine`). An accent `+` in the # column says what it is
/// for.
///
/// Its cells are drawn as text like a stored row's, what was typed into them
/// or, while empty, what saving them would write: the sticky envelope and
/// wallet, the author, the category the history suggests for the note (in
/// `text2`, brighter than a plain placeholder, since it is what will be
/// written) and a zero.
struct NewRowView: View {
    @Bindable var store: AppStore
    /// The cursor, while it is on this line.
    let cursor: LineCursor?
    let edited: EditedCell
    /// The grid's one completion list, for the CATEGORY cell.
    let completion: CategoryCompletionModel
    /// The grid's one envelope list, for the ENVELOPE cell.
    let envelopes: EnvelopeChoiceModel
    /// With the column hidden the new row still lands on the sticky default
    /// wallet; showing it lets the wallet be picked per row.
    var showsWallet = false
    /// A click on a cell of the line.
    let onClick: (RowField) -> Void

    private var columns: LedgerColumnWidths { store.ledgerColumns }

    private var line: RowDraft { edited.editor.newLine }

    var body: some View {
        HStack(spacing: 0) {
            GridCell(width: GridColumn.ordinal, alignment: .trailing) {
                Image(systemName: "plus")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(Ink.accent)
                    .accessibilityHidden(true)
            }
            .onTapGesture { onClick(.note) }
            if isEdited(.date) {
                GridCell(width: columns[.date]) { CellField(field: .date, key: key(.date), edited: edited) }
            } else {
                cell(.date, width: columns[.date], spoken: LedgerDate.day(line.day)) {
                    DayLabel(date: line.day)
                }
            }
            if isEdited(.flow) {
                GridCell(width: columns[.flow]) {
                    EnvelopeSelectCell(
                        placeholder: String(localized: "Envelope"),
                        store: store,
                        choices: envelopes,
                        key: key(.flow),
                        edited: edited
                    )
                }
            } else {
                value(.flow, line.flowId.map { store.envelopeName($0) } ?? "", placeholder: envelopePlaceholder, width: columns[.flow])
            }
            if isEdited(.category) {
                GridCell(width: columns[.category]) {
                    CategoryCell(
                        placeholder: line.suggestedCategory ?? String(localized: "Category"),
                        placeholderTint: line.suggestedCategory == nil ? Ink.text3 : Ink.text2,
                        store: store,
                        completion: completion,
                        key: key(.category),
                        edited: edited
                    )
                }
            } else {
                value(
                    .category,
                    line.category,
                    placeholder: line.suggestedCategory ?? String(localized: "Category"),
                    placeholderTint: line.suggestedCategory == nil ? Ink.text3 : Ink.text2,
                    width: columns[.category]
                )
            }
            field(.note, line.note, placeholder: String(localized: "Description"), width: nil)
            if showsWallet {
                field(
                    .wallet,
                    line.wallet,
                    placeholder: store.defaultWalletName ?? String(localized: "Wallet"),
                    width: columns[.wallet]
                )
            }
            // The author until another name is typed; emptied, it still
            // means the author, and says so.
            field(.person, line.person, placeholder: store.currentAuthor, width: columns[.person])
            // A duplicated refund is saved as one, and says so in the green
            // the saved row will have.
            field(
                .amount,
                line.amount,
                placeholder: LedgerMoney.bare(0),
                width: columns[.amount],
                alignment: .trailing,
                tint: line.kind == .refund ? Ink.positive : Ink.text
            )
            RowActionTrack()
        }
        .frame(height: Metrics.rowHeight)
        // One group VoiceOver can name, with the cells inside it.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(String(localized: "New row"))
    }

    private func key(_ field: RowField) -> CellFocus { CellFocus(row: nil, field: field) }

    private func isEdited(_ field: RowField) -> Bool { cursor == LineCursor(field: field, editing: true) }

    /// No envelope picked yet: the line lands on the sticky one, which it
    /// names while the vault has one.
    private var envelopePlaceholder: String {
        store.defaultFlowId.map { store.envelopeName($0) } ?? String(localized: "Envelope")
    }

    /// A plain text cell: the field while it is typed into, its text or its
    /// placeholder otherwise.
    @ViewBuilder
    private func field(
        _ field: RowField,
        _ text: String,
        placeholder: String,
        width: CGFloat?,
        alignment: Alignment = .leading,
        tint: Color = Ink.text
    ) -> some View {
        if isEdited(field) {
            GridCell(width: width, alignment: alignment) {
                CellField(
                    field: field,
                    key: key(field),
                    edited: edited,
                    prompt: placeholder,
                    alignment: alignment == .trailing ? .trailing : .leading,
                    tint: tint
                )
            }
        } else {
            value(field, text, placeholder: placeholder, width: width, alignment: alignment, tint: tint)
        }
    }

    private func value(
        _ field: RowField,
        _ text: String,
        placeholder: String,
        placeholderTint: Color = Ink.text3,
        width: CGFloat?,
        alignment: Alignment = .leading,
        tint: Color = Ink.text
    ) -> some View {
        let empty = text.isEmpty
        return cell(field, width: width, alignment: alignment, spoken: empty ? placeholder : text) {
            Text(empty ? placeholder : text)
                .font(Face.row)
                .foregroundStyle(empty ? placeholderTint : tint)
        }
    }

    /// A cell not being typed into. VoiceOver reads it by its column with
    /// what it says, and pressing it types into it, as a double click does.
    private func cell<Content: View>(
        _ field: RowField,
        width: CGFloat?,
        alignment: Alignment = .leading,
        spoken: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        SheetCell(
            width: width,
            alignment: alignment,
            selected: cursor == LineCursor(field: field, editing: false),
            onClick: { onClick(field) }
        ) {
            content()
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(field.label)
        .accessibilityValue(spoken)
        .accessibilityAction { edited.editor.edit(key(field)) }
    }
}
