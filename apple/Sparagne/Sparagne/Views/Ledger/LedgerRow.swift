import SwiftUI
import SparagneCore

/// Column geometry, shared by the header, the rows and the empty line so the
/// three agree without a layout pass (`docs/v2/UI.md` §5).
enum GridColumn {
    static let ordinal: CGFloat = 46
    static let date: CGFloat = 78
    static let flow: CGFloat = 98
    static let category: CGFloat = 118
    static let person: CGFloat = 92
    static let amount: CGFloat = 112
    /// DESCRIZIONE takes whatever is left, down to this.
    static let noteMinimum: CGFloat = 160
    static let padding: CGFloat = 10
}

/// One cell: fixed width, one line, the grid's padding.
private struct Cell<Content: View>: View {
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
    }
}

/// The column headings.
struct GridHeader: View {
    var body: some View {
        HStack(spacing: 0) {
            Cell(width: GridColumn.ordinal, alignment: .trailing) { SectionLabel(text: "#") }
            Cell(width: GridColumn.date) { SectionLabel(text: String(localized: "Date")) }
            Cell(width: GridColumn.flow) { SectionLabel(text: String(localized: "Envelope")) }
            Cell(width: GridColumn.category) { SectionLabel(text: String(localized: "Category")) }
            Cell { SectionLabel(text: String(localized: "Description")) }
            Cell(width: GridColumn.person) { SectionLabel(text: String(localized: "Person")) }
            Cell(width: GridColumn.amount, alignment: .trailing) { SectionLabel(text: String(localized: "Amount")) }
        }
        .frame(height: 24)
        .background(Ink.bg)
    }
}

/// The keyboard legend under the grid.
struct KeyHints: View {
    var body: some View {
        HStack(spacing: 18) {
            hint("\u{21E5}", String(localized: "next field"))
            hint("\u{21A9}", String(localized: "save row"))
            hint("esc", String(localized: "cancel"))
            hint("\u{2318}D", String(localized: "duplicate last"))
            Spacer()
        }
        .padding(.horizontal, GridColumn.padding)
        .frame(height: 26)
        .background(Ink.bg)
    }

    private func hint(_ key: String, _ label: String) -> some View {
        HStack(spacing: 5) {
            Text(key).font(Face.footnote).foregroundStyle(Ink.text)
            Text(label).font(Face.footnote).foregroundStyle(Ink.dim)
        }
    }
}

// MARK: - An existing row

struct LedgerRowView: View {
    let ordinal: Int
    let row: TransactionRow
    @Bindable var store: AppStore
    /// Non-nil while this row is the one being edited.
    let draft: Binding<RowDraft>?
    @FocusState.Binding var focus: CellFocus?
    let onOpen: (RowField) -> Void
    let onCommit: () -> Void
    let onCancel: () -> Void

    private var editing: Bool { draft != nil }

    var body: some View {
        HStack(spacing: 0) {
            Cell(width: GridColumn.ordinal, alignment: .trailing) {
                Text(String(format: "%03d", ordinal))
                    .font(Face.row)
                    .foregroundStyle(Ink.dim)
            }

            if let draft {
                DayCell(day: draft.day, month: store.month, focus: $focus, key: key(.date))
                textCell(.flow, width: GridColumn.flow)
                textCell(.category, width: GridColumn.category)
                textCell(.note, width: nil)
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
        Cell(width: GridColumn.date) { text(LedgerDate.day(row.occurredAt)) }
            .onTapGesture { onOpen(.date) }
        Cell(width: GridColumn.flow) { text(row.envelopeDisplay) }
            .onTapGesture { onOpen(.flow) }
        Cell(width: GridColumn.category) { text(row.category) }
            .onTapGesture { onOpen(.category) }
        Cell { text(row.note.isEmpty ? TransactionRow.placeholder : row.note) }
            .onTapGesture { onOpen(.note) }
        Cell(width: GridColumn.person) {
            Text(row.person)
                .font(Face.row)
                .foregroundStyle(Ink.dim)
        }
        Cell(width: GridColumn.amount, alignment: .trailing) {
            Text(LedgerMoney.amount(row.absoluteAmount))
                .font(Face.row)
                .foregroundStyle(amountTint)
                .strikethrough(row.voided)
        }
        .onTapGesture { onOpen(.amount) }
    }

    private func text(_ value: String) -> some View {
        Text(value)
            .font(Face.row)
            .foregroundStyle(Ink.text)
            .strikethrough(row.voided)
    }

    /// Income green, everything else the ledger's default ink: in a month
    /// filtered to USCITE every row is an expense, so painting them all red
    /// would say nothing.
    private var amountTint: Color {
        if row.voided { return Ink.dim }
        switch row.kind {
        case .income: return Ink.positive
        case .refund: return Ink.positive
        case .expense: return Ink.text
        case .transferWallet, .transferFlow: return Ink.dim
        }
    }

    // MARK: Editing cells

    private func textCell(_ field: RowField, width: CGFloat?) -> some View {
        Cell(width: width) {
            TextField("", text: binding(field))
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .focused($focus, equals: key(field))
                .onSubmit(onCommit)
        }
    }

    private var amountCell: some View {
        Cell(width: GridColumn.amount, alignment: .trailing) {
            TextField("", text: binding(.amount))
                .textFieldStyle(.plain)
                .font(Face.row)
                .multilineTextAlignment(.trailing)
                .foregroundStyle(Ink.text)
                .focused($focus, equals: key(.amount))
                .onSubmit(onCommit)
        }
    }

    private var personCell: some View {
        Cell(width: GridColumn.person) {
            Text(row.person)
                .font(Face.row)
                .foregroundStyle(Ink.dim)
        }
    }

    private func binding(_ field: RowField) -> Binding<String> {
        guard let draft else { return .constant("") }
        switch field {
        case .flow: return draft.flow
        case .category: return draft.category
        case .note: return draft.note
        case .amount: return draft.amount
        case .date: return .constant("")
        }
    }
}

// MARK: - The empty line

/// Always the last line of the grid: fill it left to right and press ↩.
struct NewRowView: View {
    @Bindable var store: AppStore
    @Binding var draft: RowDraft
    @FocusState.Binding var focus: CellFocus?
    let onCommit: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            Cell(width: GridColumn.ordinal, alignment: .trailing) { Text("").font(Face.row) }
            DayCell(day: $draft.day, month: store.month, focus: $focus, key: CellFocus(row: nil, field: .date))
            field($draft.flow, .flow, String(localized: "Envelope"), width: GridColumn.flow)
            field($draft.category, .category, String(localized: "Category"), width: GridColumn.category)
            field($draft.note, .note, String(localized: "description…"), width: nil)
            Cell(width: GridColumn.person) {
                Text(store.currentAuthor)
                    .font(Face.row)
                    .foregroundStyle(Ink.dim)
            }
            Cell(width: GridColumn.amount, alignment: .trailing) {
                TextField("0,00", text: $draft.amount)
                    .textFieldStyle(.plain)
                    .font(Face.row)
                    .multilineTextAlignment(.trailing)
                    .foregroundStyle(Ink.text)
                    .focused($focus, equals: CellFocus(row: nil, field: .amount))
                    .onSubmit(onCommit)
            }
        }
        .frame(height: Metrics.rowHeight)
        .onKeyPress(.escape) {
            draft = RowDraft.blank(in: store)
            focus = nil
            return .handled
        }
    }

    private func field(
        _ text: Binding<String>,
        _ field: RowField,
        _ placeholder: String,
        width: CGFloat?
    ) -> some View {
        Cell(width: width) {
            TextField(placeholder, text: text)
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .focused($focus, equals: CellFocus(row: nil, field: field))
                .onSubmit(onCommit)
        }
    }
}

// MARK: - The date cell

/// Shows `01 ago` and accepts `29`, `29/8` or `29/8/2026` while editing: a day
/// alone stays inside the month on screen, which is how a ledger is filled in.
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
        Cell(width: GridColumn.date) {
            TextField("", text: $text)
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .focused($focus, equals: key)
                .onAppear { text = LedgerDate.day(day) }
                .onChange(of: day) { _, new in
                    if focus != key { text = LedgerDate.day(new) }
                }
                .onChange(of: focus) { _, new in
                    if new == key {
                        // Editing starts from the bare day number.
                        text = "\(Calendar.current.component(.day, from: day))"
                    } else {
                        commit()
                    }
                }
        }
    }

    private func commit() {
        if let parsed = LedgerDate.parseDay(text, in: month) {
            day = parsed
        }
        text = LedgerDate.day(day)
    }
}
