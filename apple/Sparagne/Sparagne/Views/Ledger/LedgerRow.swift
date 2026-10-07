import SwiftUI
import SparagneCore

/// Column geometry, shared by the header, the rows and the empty line so the
/// three agree without a layout pass (`docs/v2/UI.md` §5).
///
/// The widths are what a cell's content gets: the canvas's tracks
/// (`# 34 · Data 60 · Busta 84 · Categoria 124 · Descrizione · Wallet 84 ·
/// Persona 84 · Importo 96`) less the padding `GridCell` adds on both sides.
enum GridColumn {
    /// A cell's horizontal padding, on each side (`Metrics.cellPad`).
    static let padding: CGFloat = Metrics.cellPad
    static let ordinal: CGFloat = 34 - 2 * padding
    static let date: CGFloat = 60 - 2 * padding
    static let flow: CGFloat = 84 - 2 * padding
    static let category: CGFloat = 124 - 2 * padding
    /// The optional WALLET column (`docs/v2/UI.md` §3), between DESCRIZIONE
    /// and PERSONA when the View menu turns it on.
    static let wallet: CGFloat = 84 - 2 * padding
    static let person: CGFloat = 84 - 2 * padding
    static let amount: CGFloat = 96 - 2 * padding
}

/// One cell: fixed width, one line, the grid's padding. Shared with the
/// setup tables (`Views/Setup`), which are drawn as the same kind of sheet.
struct GridCell<Content: View>: View {
    var width: CGFloat?
    var alignment: Alignment = .leading
    @ViewBuilder var content: Content

    var body: some View {
        content
            .lineLimit(1)
            .truncationMode(.tail)
            .frame(width: width, alignment: alignment)
            .frame(maxWidth: width == nil ? .infinity : nil, alignment: alignment)
            .padding(.horizontal, GridColumn.padding)
            // The whole column is the click target. SwiftUI does not hit-test
            // the transparent part of a frame, so without this a short "Casa"
            // or the "—" of an empty note would leave most of its cell dead
            // and the row would refuse to open (`docs/v2/UI.md` §2.1).
            .contentShape(Rectangle())
    }
}

/// The column headings: small, medium weight, `text3`, on the sheet's own
/// ground (`.r.hd`). The grid draws the stronger rule under them.
struct GridHeader: View {
    /// The optional WALLET column, off by default.
    var showsWallet = false

    var body: some View {
        HStack(spacing: 0) {
            GridCell(width: GridColumn.ordinal, alignment: .trailing) {
                heading("#", spoken: String(localized: "Number"))
            }
            GridCell(width: GridColumn.date) { heading(RowField.date.label) }
            GridCell(width: GridColumn.flow) { heading(RowField.flow.label) }
            GridCell(width: GridColumn.category) { heading(RowField.category.label) }
            GridCell { heading(RowField.note.label) }
            if showsWallet {
                GridCell(width: GridColumn.wallet) { heading(RowField.wallet.label) }
            }
            GridCell(width: GridColumn.person) { heading(String(localized: "Person")) }
            GridCell(width: GridColumn.amount, alignment: .trailing) { heading(RowField.amount.label) }
        }
        .frame(height: Metrics.headerHeight)
        .background(Ink.sheet)
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
        if showsWallet, row.walletDisplay != TransactionRow.placeholder {
            parts.append(String(localized: "wallet \(row.walletDisplay)"))
        }
        parts.append(String(localized: "by \(row.person)"))
        if row.voided { parts.append(String(localized: "voided")) }
        return parts.joined(separator: ", ")
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
struct RowTag: View {
    let text: String
    let tint: Color

    var body: some View {
        Text(text)
            .font(Face.ui(10, .semibold))
            .foregroundStyle(tint)
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 5)
            .frame(height: 16)
            .background(tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 4))
    }
}

/// The 18-point buttons inside a row (`.btn.sm`): a ghost for the quiet
/// choice, an accent outline for the one that writes.
struct RowButtonStyle: ButtonStyle {
    enum Tone { case ghost, accent, neutral }

    var tone: Tone = .neutral

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Face.ui(11, .medium))
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

extension View {
    /// The focused cell (`.fbox`): a 20-point box on the raised ground with
    /// a 1.5-point accent ring inside, reaching 5 points into the cell's
    /// padding so the text does not move when the ring appears.
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

// MARK: - An existing row

struct LedgerRowView: View {
    let ordinal: Int
    let row: TransactionRow
    @Bindable var store: AppStore
    /// Draws the WALLET column when the View menu is showing it.
    var showsWallet = false
    /// Non-nil while this row is the one being edited.
    let draft: Binding<RowDraft>?
    @FocusState.Binding var focus: CellFocus?
    /// The grid's one completion list, for the CATEGORY cell.
    let completion: CategoryCompletionModel
    let onOpen: (RowField) -> Void
    let onCommit: () -> Void
    let onCancel: () -> Void

    private var editing: Bool { draft != nil }

    var body: some View {
        HStack(spacing: 0) {
            GridCell(width: GridColumn.ordinal, alignment: .trailing) {
                Text(ordinal, format: .number.grouping(.never))
                    .font(Face.small)
                    .foregroundStyle(Ink.text3)
            }
            // # and PERSONA hold no field of their own, but a click anywhere
            // on a line should open it; DESCRIZIONE is the widest cell and the
            // one most often retyped, so it takes the caret.
            .onTapGesture { onOpen(.note) }

            if let draft {
                DayCell(day: draft.day, month: store.month, focus: $focus, key: key(.date))
                textCell(.flow, width: GridColumn.flow)
                GridCell(width: GridColumn.category) {
                    CategoryCell(
                        text: draft.category,
                        placeholder: "",
                        store: store,
                        completion: completion,
                        focus: $focus,
                        key: key(.category),
                        onSubmit: onCommit
                    )
                }
                textCell(.note, width: nil)
                if showsWallet { walletCell }
                personCell
                amountCell
            } else {
                display
            }
        }
        .frame(height: Metrics.rowHeight)
        .contentShape(Rectangle())
        .onKeyPress(.escape) {
            guard editing else { return .ignored }
            onCancel()
            return .handled
        }
    }

    private func key(_ field: RowField) -> CellFocus { CellFocus(row: row.id, field: field) }

    // MARK: Read-only cells

    @ViewBuilder
    private var display: some View {
        GridCell(width: GridColumn.date) {
            DayLabel(date: row.occurredAt, tint: ink(Ink.text), struck: row.voided)
        }
        .onTapGesture { onOpen(.date) }
        GridCell(width: GridColumn.flow) { text(row.envelopeDisplay, Ink.text2) }
            .onTapGesture { onOpen(.flow) }
        GridCell(width: GridColumn.category) { text(row.category, categoryTint) }
            .onTapGesture { onOpen(.category) }
        GridCell {
            HStack(spacing: 6) {
                if row.note.isEmpty {
                    text(TransactionRow.placeholder, Ink.text3)
                } else {
                    text(row.note, Ink.text)
                }
                if row.kind == .refund {
                    RowTag(text: String(localized: "refund"), tint: Ink.positive)
                }
            }
        }
        .onTapGesture { onOpen(.note) }
        if showsWallet {
            GridCell(width: GridColumn.wallet) { text(row.walletDisplay, Ink.text2) }
                .onTapGesture { onOpen(row.isTransfer ? .note : .wallet) }
        }
        GridCell(width: GridColumn.person) { text(row.person, Ink.text2) }
            .onTapGesture { onOpen(.note) }
        GridCell(width: GridColumn.amount, alignment: .trailing) {
            text(amountText, amountTint)
        }
        .onTapGesture { onOpen(.amount) }
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
    private var amountText: String {
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

    // MARK: Editing cells

    private func textCell(_ field: RowField, width: CGFloat?) -> some View {
        GridCell(width: width) {
            TextField("", text: binding(field))
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .accessibilityLabel(field.label)
                .focused($focus, equals: key(field))
                .onSubmit(onCommit)
                .cellFocusRing(focus == key(field))
        }
    }

    private var amountCell: some View {
        GridCell(width: GridColumn.amount, alignment: .trailing) {
            TextField("", text: binding(.amount))
                .textFieldStyle(.plain)
                .font(Face.row)
                .multilineTextAlignment(.trailing)
                .foregroundStyle(Ink.text)
                .accessibilityLabel(RowField.amount.label)
                .focused($focus, equals: key(.amount))
                .onSubmit(onCommit)
                .cellFocusRing(focus == key(.amount))
        }
    }

    /// A transfer's wallets are its two ends, not a field: `UpdateTransaction`
    /// moves them with `from_id`/`to_id`, which the grid does not edit, so the
    /// cell stays read-only for those rows.
    @ViewBuilder
    private var walletCell: some View {
        if row.isTransfer {
            GridCell(width: GridColumn.wallet) {
                Text(row.walletDisplay)
                    .font(Face.row)
                    .foregroundStyle(Ink.text3)
            }
        } else {
            textCell(.wallet, width: GridColumn.wallet)
        }
    }

    private var personCell: some View {
        GridCell(width: GridColumn.person) {
            Text(row.person)
                .font(Face.row)
                .foregroundStyle(Ink.text2)
        }
    }

    private func binding(_ field: RowField) -> Binding<String> {
        guard let draft else { return .constant("") }
        switch field {
        case .flow: return draft.flow
        case .category: return draft.category
        case .note: return draft.note
        case .wallet: return draft.wallet
        case .amount: return draft.amount
        case .date: return .constant("")
        }
    }
}

// MARK: - A due recurring period

/// A recurring period that fell due this month, at its date among the rows
/// (`docs/v2/UI.md` §2.5): hatched, in `text2`, with "Salta" and "Registra"
/// in the DESCRIZIONE cell. Nothing is written until one of the two is
/// pressed; a template never writes a transaction by itself
/// (`DISTILLATO_V1.md` §2.3).
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

    var body: some View {
        HStack(spacing: 0) {
            GridCell(width: GridColumn.ordinal, alignment: .trailing) {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 10.5, weight: .medium))
                    .foregroundStyle(Ink.text3)
            }
            GridCell(width: GridColumn.date) {
                DayLabel(date: CoreDate.localDay(period.date) ?? store.month.start(), tint: Ink.text2)
            }
            GridCell(width: GridColumn.flow) { Text(store.envelopeName(template.flowId)) }
            GridCell(width: GridColumn.category) { Text(category) }
            GridCell {
                HStack(spacing: 6) {
                    if let note = template.note, !note.isEmpty {
                        Text(note)
                    }
                    RowTag(text: String(localized: "to confirm"), tint: Ink.accent)
                    Spacer(minLength: 6)
                    if !store.isReadOnly {
                        Button(String(localized: "Skip")) { run { await store.skipRecurring(template.id, periodDate: period.date) } }
                            .buttonStyle(RowButtonStyle(tone: .ghost))
                        Button(String(localized: "Record")) { run { await store.executeRecurring(template.id, periodDate: period.date) } }
                            .buttonStyle(RowButtonStyle(tone: .accent))
                    }
                }
                .disabled(working)
            }
            if showsWallet {
                GridCell(width: GridColumn.wallet) { Text(store.walletName(template.walletId)) }
            }
            // Whoever registers it is its author, as for any row written here.
            GridCell(width: GridColumn.person) {
                Text(store.currentAuthor).foregroundStyle(Ink.text3)
            }
            GridCell(width: GridColumn.amount, alignment: .trailing) {
                Text(LedgerMoney.bare(template.amount))
            }
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
                Button(String(localized: "Record")) { run { await store.executeRecurring(template.id, periodDate: period.date) } }
                Button(String(localized: "Skip")) { run { await store.skipRecurring(template.id, periodDate: period.date) } }
            }
        }
    }

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

/// Always the last line of the grid: fill it left to right and press ↩. An
/// accent `+` in the # column says what it is for.
struct NewRowView: View {
    @Bindable var store: AppStore
    @Binding var draft: RowDraft
    @FocusState.Binding var focus: CellFocus?
    /// The grid's one completion list, for the CATEGORY cell.
    let completion: CategoryCompletionModel
    let onCommit: () -> Void
    /// With the column hidden the new row still lands on the sticky default
    /// wallet; showing it lets the wallet be picked per row (`UI.md` §3).
    var showsWallet = false

    var body: some View {
        HStack(spacing: 0) {
            GridCell(width: GridColumn.ordinal, alignment: .trailing) {
                Image(systemName: "plus")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(Ink.accent)
                    .accessibilityHidden(true)
            }
            DayCell(day: $draft.day, month: store.month, focus: $focus, key: CellFocus(row: nil, field: .date))
            field($draft.flow, .flow, String(localized: "Envelope"), width: GridColumn.flow)
            GridCell(width: GridColumn.category) {
                CategoryCell(
                    text: $draft.category,
                    // The history's guess for the note, saved if the cell is
                    // left empty (`RowDraft.suggestedCategory`); brighter than
                    // a plain placeholder, since it is what will be written.
                    placeholder: draft.suggestedCategory ?? String(localized: "Category"),
                    placeholderTint: draft.suggestedCategory == nil ? Ink.text3 : Ink.text2,
                    store: store,
                    completion: completion,
                    focus: $focus,
                    key: CellFocus(row: nil, field: .category),
                    onSubmit: onCommit
                )
            }
            field($draft.note, .note, String(localized: "Description"), width: nil)
            if showsWallet {
                field(
                    $draft.wallet,
                    .wallet,
                    store.defaultWalletName ?? String(localized: "Wallet"),
                    width: GridColumn.wallet
                )
            }
            GridCell(width: GridColumn.person) {
                Text(store.currentAuthor)
                    .font(Face.row)
                    .foregroundStyle(Ink.text2)
            }
            GridCell(width: GridColumn.amount, alignment: .trailing) {
                let key = CellFocus(row: nil, field: .amount)
                // A title, not a prompt: a prompt ignores the trailing
                // alignment and would sit at the cell's left edge.
                TextField("0,00", text: $draft.amount)
                    .textFieldStyle(.plain)
                    .font(Face.row)
                    .multilineTextAlignment(.trailing)
                    .accessibilityLabel(RowField.amount.label)
                    // A duplicated refund is saved as one, and says so in the
                    // green the saved row will have.
                    .foregroundStyle(draft.kind == .refund ? Ink.positive : Ink.text)
                    .focused($focus, equals: key)
                    .onSubmit(onCommit)
                    .cellFocusRing(focus == key)
            }
        }
        .frame(height: Metrics.rowHeight)
        // One group VoiceOver can name, with the cells inside it.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(String(localized: "New row"))
        .onKeyPress(.escape) {
            draft = RowDraft.blank(in: store)
            focus = nil
            return .handled
        }
    }

    private func prompt(_ text: String) -> Text {
        Text(text).foregroundStyle(Ink.text3)
    }

    private func field(
        _ text: Binding<String>,
        _ field: RowField,
        _ placeholder: String,
        width: CGFloat?
    ) -> some View {
        let key = CellFocus(row: nil, field: field)
        return GridCell(width: width) {
            TextField("", text: text, prompt: prompt(placeholder))
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .accessibilityLabel(field.label)
                .focused($focus, equals: key)
                .onSubmit(onCommit)
                .cellFocusRing(focus == key)
        }
    }
}

// MARK: - The date cell

/// Shows `01 gio` and accepts `29`, `29/8` or `29/8/2026` while editing: a
/// day alone stays inside the month on screen, which is how a ledger is filled
/// in. Text that means no date leaves the day alone and the cell snaps back to
/// it.
///
/// The field is empty while it does not have the caret and `DayLabel` is drawn
/// over it: a text field cannot set the weekday smaller and fainter than the
/// day. The label lets clicks through, so a click still lands on the field.
struct DayCell: View {
    @Binding var day: Date
    let month: MonthKey
    @FocusState.Binding var focus: CellFocus?
    let key: CellFocus

    @State private var text = ""

    init(day: Binding<Date>, month: MonthKey, focus: FocusState<CellFocus?>.Binding, key: CellFocus) {
        _day = day
        self.month = month
        _focus = focus
        self.key = key
    }

    var body: some View {
        GridCell(width: GridColumn.date) {
            TextField("", text: $text)
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .accessibilityLabel(RowField.date.label)
                .accessibilityValue(focus == key ? text : LedgerDate.day(day))
                .focused($focus, equals: key)
                .overlay(alignment: .leading) {
                    if focus != key {
                        DayLabel(date: day).allowsHitTesting(false)
                    }
                }
                .cellFocusRing(focus == key)
                // Clicking the DATA cell of a closed row focuses it in the
                // same update that builds this view, so `onChange(of: focus)`
                // never fires: without this the cell would open empty, and
                // typing a day would not start from the one it had.
                .onAppear { text = focus == key ? Self.number(day) : "" }
                .onChange(of: focus) { old, new in
                    if new == key {
                        // Editing starts from the bare day number.
                        text = Self.number(day)
                    } else if old == key {
                        // Only when the focus leaves this cell: `focus` is the
                        // whole grid's, and every other cell changes it too.
                        text = ""
                    }
                }
                .onChange(of: text) { _, new in
                    // The day is read back as it is typed, not on blur, so a
                    // row committed from somewhere else (⇥ off the line, a
                    // click on another row) still carries what this cell says.
                    guard focus == key, let parsed = LedgerDate.parseDay(new, in: month) else { return }
                    day = parsed
                }
        }
    }

    /// The bare day number, which is what editing starts from.
    private static func number(_ date: Date, calendar: Calendar = .current) -> String {
        "\(calendar.component(.day, from: date))"
    }
}
