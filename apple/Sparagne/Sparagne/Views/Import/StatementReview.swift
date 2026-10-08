import SwiftUI
import SparagneCore

/// The review page: every row of the file with what the core would do with
/// it. A new row can be ticked off (it goes to the core as `skip`) and its
/// category typed; a category typed here may create one, the bank's never
/// does (`core/src/statement/mod.rs`).
///
/// Ruled like the window's tables (`SetupHeaderRow`, `SetupRow`,
/// `GridCell`), and pulled out by a cell's padding so the first column
/// lines up with the sheet's title.
struct StatementReview: View {
    @Bindable var model: StatementImportModel

    var body: some View {
        VStack(spacing: 0) {
            header
            ScrollView {
                LazyVStack(spacing: 0) {
                    let names = NameBook(snapshot: model.store.snapshot)
                    ForEach(model.preview?.rows ?? [], id: \.line) { row in
                        StatementReviewRow(model: model, row: row, names: names)
                    }
                }
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .padding(.horizontal, -Metrics.cellPad)
    }

    private var header: some View {
        SetupHeaderRow {
            GridCell(width: ReviewColumn.check) { Color.clear.frame(height: 1) }
            GridCell(width: ReviewColumn.line, alignment: .trailing) {
                heading("#", spoken: String(localized: "Number"))
            }
            GridCell(width: ReviewColumn.date) { heading(String(localized: "Date")) }
            GridCell { heading(String(localized: "Description")) }
            GridCell(width: ReviewColumn.amount, alignment: .trailing) { heading(String(localized: "Amount")) }
            GridCell(width: ReviewColumn.kind) { heading(String(localized: "Kind")) }
            GridCell(width: ReviewColumn.status) { heading(String(localized: "Status")) }
            GridCell(width: ReviewColumn.category) { heading(String(localized: "Category")) }
        }
    }

    /// A column heading, announced as one, and `#` as a word.
    private func heading(_ text: String, spoken: String? = nil) -> some View {
        Text(text)
            .accessibilityLabel(spoken ?? text)
            .accessibilityAddTraits(.isHeader)
    }
}

/// Column widths of the review, as a cell's content gets them (`GridCell`
/// pads each side); the description takes what is left, about 180 points
/// of the sheet. Kind and status fit the longest words they show
/// ("trasferimento", "Needs the other wallet" in the tag's 10 pt).
private enum ReviewColumn {
    static let check: CGFloat = 14
    static let line: CGFloat = 28
    static let date: CGFloat = 70
    static let amount: CGFloat = 76
    static let kind: CGFloat = 86
    static let status: CGFloat = 140
    static let category: CGFloat = 136
}

/// One previewed row. Rows that will not be imported are drawn in the dim
/// ink, their outcome tag excepted.
private struct StatementReviewRow: View {
    @Bindable var model: StatementImportModel
    let row: StatementRow
    let names: NameBook

    private var isNew: Bool { row.status == .new }
    private var included: Bool { model.isIncluded(row.line) }
    /// Imported if the user goes ahead.
    private var live: Bool { isNew && included }

    var body: some View {
        SetupRow {
            GridCell(width: ReviewColumn.check) {
                if isNew {
                    IncludeBox(isOn: included) { model.setIncluded($0, line: row.line) }
                } else {
                    // An empty cell still holds its width.
                    Color.clear.frame(height: 1)
                }
            }
            GridCell(width: ReviewColumn.line, alignment: .trailing) {
                Text(verbatim: String(row.line)).foregroundStyle(Ink.text3)
            }
            GridCell(width: ReviewColumn.date) { Text(verbatim: date).foregroundStyle(ink) }
            GridCell {
                HStack(spacing: 6) {
                    Text(verbatim: row.payee.isEmpty ? TransactionRow.placeholder : row.payee)
                        .foregroundStyle(ink)
                    if let original = row.original {
                        Text(verbatim: original).foregroundStyle(Ink.text3)
                    }
                }
                .help(row.payee)
            }
            GridCell(width: ReviewColumn.amount, alignment: .trailing) {
                Text(verbatim: LedgerMoney.bare(signedAmount)).foregroundStyle(live ? amountTint : Ink.text3)
            }
            GridCell(width: ReviewColumn.kind) {
                Text(verbatim: StatementText.kind(row.kind)).foregroundStyle(ink)
            }
            GridCell(width: ReviewColumn.status) {
                OutcomeTag(text: statusText, tone: tone)
                    .help(StatementText.detail(row.status) ?? "")
            }
            GridCell(width: ReviewColumn.category) { category }
        }
        .font(Face.row)
    }

    private var ink: Color { live ? Ink.text : Ink.text3 }

    private var date: String {
        guard let stamp = row.occurredAt, let date = CoreDate.date(stamp) else { return TransactionRow.placeholder }
        return CoreDate.day(date)
    }

    /// Expenses out, everything else in, as the ledger's totals sign them.
    private var signedAmount: Int64 { row.kind == .expense ? -row.amount : row.amount }

    private var amountTint: Color {
        switch row.kind {
        case .expense: Ink.negative
        case .income, .refund: Ink.positive
        default: Ink.text
        }
    }

    /// A new row the user ticked off reads as the core will count it.
    private var statusText: String {
        isNew && !included ? StatementText.skipped("skipped_by_you") : StatementText.status(row.status)
    }

    private var tone: OutcomeTag.Tone {
        switch row.status {
        case .new: included ? .new : .quiet
        case .alreadyImported, .skipped: .quiet
        case .invalid: .invalid
        }
    }

    /// Editable on a new entry; the other wallet on a transfer; the bank's
    /// match, dimmed, on a row that will not be imported.
    ///
    /// The placeholder of an empty field is drawn here in the dim ink, not
    /// left to the field: the field's own prompt takes the row's text ink
    /// and would read as a category.
    @ViewBuilder
    private var category: some View {
        if StatementImportModel.takesCategory(row) {
            let text = model.categoryText(for: row)
            ZStack(alignment: .leading) {
                if text.isEmpty {
                    Text(String(localized: "category…"))
                        .foregroundStyle(Ink.text3)
                        .allowsHitTesting(false)
                }
                TextField(
                    String(localized: "Category"),
                    text: Binding(
                        get: { model.categoryText(for: row) },
                        set: { model.setCategory($0, for: row.line) }
                    ),
                    prompt: Text(verbatim: "")
                )
                .textFieldStyle(.plain)
                .foregroundStyle(ink)
            }
            .disabled(!included)
        } else if row.kind == .transferWallet, let wallet = names.wallet(row.counterWalletId) {
            Text(verbatim: "\u{21C4} \(wallet)").foregroundStyle(Ink.text3)
        } else {
            Text(verbatim: row.matchedCategory ?? "").foregroundStyle(Ink.text3)
        }
    }
}

/// What the core will do with a row, as a small rounded tag: soft accent on
/// a row that will be imported, `raised` and dim on one that will not
/// (already in the vault, skipped by a rule, its status or the user), soft
/// negative on one the core cannot read. The ledger's and setup's tags,
/// so the three tables speak the same way.
private struct OutcomeTag: View {
    enum Tone { case new, quiet, invalid }

    let text: String
    let tone: Tone

    var body: some View {
        switch tone {
        case .new: RowTag(text: text, tint: Ink.accent)
        case .quiet: SetupTag(text: text)
        case .invalid: RowTag(text: text, tint: Ink.negative)
        }
    }
}

/// The exclude box of a new row, drawn by hand like the form's switch: the
/// accent with a dark tick while the row goes in, an outline once it is
/// ticked off. VoiceOver gets the system checkbox it stands for, named
/// "Import" as before.
private struct IncludeBox: View {
    let isOn: Bool
    let set: (Bool) -> Void

    var body: some View {
        Button {
            set(!isOn)
        } label: {
            ZStack {
                RoundedRectangle(cornerRadius: 3)
                    .fill(isOn ? Ink.accent : Ink.card)
                RoundedRectangle(cornerRadius: 3)
                    .strokeBorder(isOn ? Ink.accent : Ink.line2, lineWidth: 1)
                if isOn {
                    Image(systemName: "checkmark")
                        .font(.system(size: 8, weight: .bold))
                        .foregroundStyle(Color(hex: 0x140A00))
                }
            }
            .frame(width: 13, height: 13)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityRepresentation {
            Toggle(String(localized: "Import"), isOn: Binding(get: { isOn }, set: set))
        }
    }
}

/// What the import did: the counts, then every row the core refused, with
/// its line and the reason in the user's language.
struct StatementReportView: View {
    let report: StatementReport

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            FormGroup(String(localized: "Import finished")) {
                Panel {
                    HStack(alignment: .top, spacing: 0) {
                        figure(String(localized: "Imported"), report.executed, Ink.positive)
                        figure(String(localized: "Already imported"), report.deduplicated, Ink.text)
                        figure(String(localized: "Skipped"), report.skipped, Ink.text)
                        figure(String(localized: "Rounded"), report.rounded, Ink.text)
                        figure(String(localized: "Refused"), UInt32(report.rejected.count), Ink.negative)
                    }
                }
                if report.rounded > 0 {
                    FormNote(String(localized: "Rounded rows had more decimals than the currency keeps."), indented: false)
                }
            }
            if !report.rejected.isEmpty {
                FormGroup(String(localized: "Refused")) {
                    VStack(spacing: 0) {
                        ForEach(Array(report.rejected.enumerated()), id: \.offset) { _, rejection in
                            SetupRow {
                                GridCell(width: Self.lineColumn) {
                                    Text(String(localized: "Line \(Int(rejection.line))"))
                                        .foregroundStyle(Ink.text3)
                                }
                                GridCell {
                                    HStack(spacing: 10) {
                                        Text(ErrorMessages.summary(for: rejection.code))
                                            .foregroundStyle(Ink.negative)
                                            .layoutPriority(1)
                                        Text(verbatim: rejection.message)
                                            .foregroundStyle(Ink.text3)
                                            .help(rejection.message)
                                    }
                                }
                            }
                        }
                    }
                    .font(Face.row)
                    .padding(.horizontal, -Metrics.cellPad)
                }
            }
        }
        .padding(.bottom, 4)
    }

    /// "Riga 1234" with room to spare.
    private static let lineColumn: CGFloat = 70

    /// A figure of the report, as the summary's cards show one: the label
    /// over the number, the number dimmed at zero.
    private func figure(_ label: String, _ value: UInt32, _ tint: Color) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(Face.label)
                .foregroundStyle(Ink.text3)
            Text(verbatim: String(value))
                .font(Face.display)
                .foregroundStyle(value == 0 ? Ink.text3 : tint)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}
