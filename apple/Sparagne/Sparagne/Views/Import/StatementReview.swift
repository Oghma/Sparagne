import SwiftUI
import SparagneCore

/// The review page: every row of the file with what the core would do with
/// it. A new row can be ticked off (it goes to the core as `skip`) and its
/// category typed; a category typed here may create one, the bank's never
/// does (`core/src/statement/mod.rs`).
struct StatementReview: View {
    @Bindable var model: StatementImportModel

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                StatementCountsStrip(counts: model.counts)
                Spacer()
                if model.isPreviewing { ProgressView().controlSize(.small) }
            }
            .padding(.horizontal, Metrics.gutter)
            .frame(height: 34)
            Hairline()
            header
            Hairline()
            ScrollView {
                LazyVStack(spacing: 0) {
                    let names = NameBook(snapshot: model.store.snapshot)
                    ForEach(model.preview?.rows ?? [], id: \.line) { row in
                        StatementReviewRow(model: model, row: row, names: names)
                        Hairline()
                    }
                }
            }
            .scrollBounceBehavior(.basedOnSize)
        }
    }

    private var header: some View {
        HStack(spacing: 0) {
            ReviewCell(width: ReviewColumn.check) { Color.clear.frame(height: 1) }
            ReviewCell(width: ReviewColumn.line, alignment: .trailing) { SectionLabel(text: "#") }
            ReviewCell(width: ReviewColumn.date) { SectionLabel(text: String(localized: "Date")) }
            ReviewCell { SectionLabel(text: String(localized: "Description")) }
            ReviewCell(width: ReviewColumn.amount, alignment: .trailing) {
                SectionLabel(text: String(localized: "Amount"))
            }
            ReviewCell(width: ReviewColumn.kind) { SectionLabel(text: String(localized: "Kind")) }
            ReviewCell(width: ReviewColumn.status) { SectionLabel(text: String(localized: "Status")) }
            ReviewCell(width: ReviewColumn.category) { SectionLabel(text: String(localized: "Category")) }
        }
        .frame(height: 24)
    }
}

/// Column widths of the review; the description takes what is left. Kind
/// and status fit the longest Italian word at the table's 11 pt
/// ("trasferimento", "Saltata da una regola").
private enum ReviewColumn {
    static let check: CGFloat = 18
    static let line: CGFloat = 30
    static let date: CGFloat = 76
    static let amount: CGFloat = 80
    static let kind: CGFloat = 90
    static let status: CGFloat = 156
    static let category: CGFloat = 150
    static let font = Face.mono(11)
}

/// One previewed row. Rows that will not be imported are drawn in the dim
/// ink, their status badge excepted.
private struct StatementReviewRow: View {
    @Bindable var model: StatementImportModel
    let row: StatementRow
    let names: NameBook

    private var isNew: Bool { row.status == .new }
    private var included: Bool { model.isIncluded(row.line) }
    /// Imported if the user goes ahead.
    private var live: Bool { isNew && included }

    var body: some View {
        HStack(spacing: 0) {
            ReviewCell(width: ReviewColumn.check) {
                if isNew {
                    Toggle(String(localized: "Import"), isOn: Binding(
                        get: { included },
                        set: { model.setIncluded($0, line: row.line) }
                    ))
                    .labelsHidden()
                    .toggleStyle(.checkbox)
                    .controlSize(.small)
                } else {
                    // An empty cell still holds its width.
                    Color.clear.frame(height: 1)
                }
            }
            ReviewCell(width: ReviewColumn.line, alignment: .trailing) {
                Text(verbatim: String(row.line)).foregroundStyle(Ink.dim)
            }
            ReviewCell(width: ReviewColumn.date) { Text(verbatim: date).foregroundStyle(ink) }
            ReviewCell {
                HStack(spacing: 6) {
                    Text(verbatim: row.payee.isEmpty ? TransactionRow.placeholder : row.payee)
                        .foregroundStyle(ink)
                    if let original = row.original {
                        Text(verbatim: original).foregroundStyle(Ink.dim)
                    }
                }
                .help(row.payee)
            }
            ReviewCell(width: ReviewColumn.amount, alignment: .trailing) {
                Text(verbatim: LedgerMoney.bare(signedAmount)).foregroundStyle(live ? amountTint : Ink.dim)
            }
            ReviewCell(width: ReviewColumn.kind) {
                Text(verbatim: StatementText.kind(row.kind)).foregroundStyle(ink)
            }
            ReviewCell(width: ReviewColumn.status) {
                Text(verbatim: statusText)
                    .foregroundStyle(statusTint)
                    .help(StatementText.detail(row.status) ?? "")
            }
            ReviewCell(width: ReviewColumn.category) { category }
        }
        .font(ReviewColumn.font)
        .frame(height: Metrics.rowHeight)
    }

    private var ink: Color { live ? Ink.text : Ink.dim }

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

    private var statusTint: Color {
        switch row.status {
        case .new: included ? Ink.positive : Ink.warning
        case .alreadyImported: Ink.dim
        case .skipped: Ink.warning
        case .invalid: Ink.negative
        }
    }

    /// Editable on a new entry; the other wallet on a transfer; the bank's
    /// match, dimmed, on a row that will not be imported.
    @ViewBuilder
    private var category: some View {
        if StatementImportModel.takesCategory(row) {
            TextField(
                String(localized: "Category"),
                text: Binding(
                    get: { model.categoryText(for: row) },
                    set: { model.setCategory($0, for: row.line) }
                ),
                prompt: Text(String(localized: "category…")).foregroundStyle(Ink.dim)
            )
            .textFieldStyle(.plain)
            .foregroundStyle(ink)
            .disabled(!included)
        } else if row.kind == .transferWallet, let wallet = names.wallet(row.counterWalletId) {
            Text(verbatim: "\u{21C4} \(wallet)").foregroundStyle(Ink.dim)
        } else {
            Text(verbatim: row.matchedCategory ?? "").foregroundStyle(Ink.dim)
        }
    }
}

/// A cell of the review: fixed width or the rest, one line, tight padding.
private struct ReviewCell<Content: View>: View {
    var width: CGFloat?
    var alignment: Alignment = .leading
    @ViewBuilder var content: Content

    var body: some View {
        content
            .lineLimit(1)
            .truncationMode(.tail)
            .frame(width: width, alignment: alignment)
            .frame(maxWidth: width == nil ? .infinity : nil, alignment: alignment)
            .padding(.horizontal, ImportMetrics.cellPadding)
    }
}

/// `NEW 4  ALREADY IMPORTED 0  SKIPPED 4  INVALID 0  ROUNDED 0`: the
/// preview's counts, as the import will make them.
struct StatementCountsStrip: View {
    let counts: StatementImportModel.Counts

    var body: some View {
        HStack(spacing: 14) {
            item(String(localized: "New"), counts.new, Ink.positive)
            item(String(localized: "Already imported"), counts.alreadyImported, Ink.text)
            item(String(localized: "Skipped"), counts.skipped, Ink.warning)
            item(String(localized: "Invalid"), counts.invalid, Ink.negative)
            item(String(localized: "Rounded"), counts.rounded, Ink.text)
        }
    }

    private func item(_ label: String, _ value: Int, _ tint: Color) -> some View {
        HStack(spacing: 5) {
            SectionLabel(text: label)
            Text(verbatim: String(value))
                .font(Face.row)
                .foregroundStyle(value == 0 ? Ink.dim : tint)
        }
    }
}

/// What the import did: the counts, then every row the core refused, with
/// its line and the reason in the user's language.
struct StatementReportView: View {
    let report: StatementReport

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Panel {
                VStack(alignment: .leading, spacing: 12) {
                    SectionLabel(text: String(localized: "Import finished"))
                    HStack(alignment: .top, spacing: 32) {
                        figure(String(localized: "Imported"), report.executed, Ink.positive)
                        figure(String(localized: "Already imported"), report.deduplicated, Ink.text)
                        figure(String(localized: "Skipped"), report.skipped, Ink.text)
                        figure(String(localized: "Rounded"), report.rounded, Ink.text)
                        figure(String(localized: "Refused"), UInt32(report.rejected.count), Ink.negative)
                    }
                    if report.rounded > 0 {
                        Text(String(localized: "Rounded rows had more decimals than the currency keeps."))
                            .font(Face.footnote)
                            .foregroundStyle(Ink.dim)
                    }
                }
            }
            if !report.rejected.isEmpty {
                Panel(padding: 0) {
                    VStack(alignment: .leading, spacing: 0) {
                        SectionLabel(text: String(localized: "Refused"))
                            .padding(.horizontal, 14)
                            .frame(height: 28)
                        Hairline()
                        ForEach(Array(report.rejected.enumerated()), id: \.offset) { _, rejection in
                            HStack(spacing: 10) {
                                Text(String(localized: "Line \(Int(rejection.line))"))
                                    .foregroundStyle(Ink.dim)
                                    .frame(width: 70, alignment: .leading)
                                Text(ErrorMessages.summary(for: rejection.code))
                                    .foregroundStyle(Ink.negative)
                                Text(verbatim: rejection.message)
                                    .foregroundStyle(Ink.dim)
                                    .lineLimit(1)
                                    .truncationMode(.tail)
                                    .help(rejection.message)
                            }
                            .font(Face.row)
                            .padding(.horizontal, 14)
                            .frame(height: Metrics.rowHeight)
                            Hairline()
                        }
                    }
                }
            }
        }
        .padding(Metrics.gutter)
    }

    private func figure(_ label: String, _ value: UInt32, _ tint: Color) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            SectionLabel(text: label)
            Text(verbatim: String(value))
                .font(Face.headline)
                .foregroundStyle(value == 0 ? Ink.dim : tint)
        }
    }
}
