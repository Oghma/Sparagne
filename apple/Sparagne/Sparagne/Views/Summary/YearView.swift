import SwiftUI
import SparagneCore

/// The ANNO view (`docs/v2/UI.md` §2.3): the same three lines as the summary,
/// with the twelve months of the year as columns and the year's total last.
struct YearView: View {
    let summary: LedgerSummary
    let store: AppStore

    private var months: [PeriodTotals] { summary.year }
    private var year: Int { summary.month.year }

    var body: some View {
        VStack(spacing: 10) {
            Panel {
                VStack(alignment: .leading, spacing: 12) {
                    SectionLabel(text: "\(year)", tint: Ink.accent)
                    ScrollView(.horizontal) {
                        VStack(spacing: 0) {
                            header
                            Hairline().padding(.vertical, 6)
                            line(String(localized: "Income"), values: months.map(\.income), tint: Ink.positive)
                            line(String(localized: "Expenses"), values: months.map(\.netExpense), tint: Ink.negative)
                            Hairline().padding(.vertical, 6)
                            line(
                                String(localized: "Savings"),
                                values: months.map(LedgerSummary.savings),
                                tint: Ink.text,
                                emphasis: true
                            )
                        }
                    }
                    .scrollBounceBehavior(.basedOnSize)
                }
            }

            Panel {
                VStack(alignment: .leading, spacing: 12) {
                    SectionLabel(text: "\(String(localized: "Income vs expenses")) \u{00B7} \(year)")
                    PairedBars(
                        pairs: months.map { (Double($0.income), Double($0.netExpense)) },
                        labels: (1...12).map { LedgerDate.shortMonth($0) },
                        highlighted: summary.month.month - 1
                    )
                    .frame(height: 220)
                }
            }

            Spacer(minLength: 0)
        }
        .padding(10)
        .background(Ink.bg)
    }

    private var header: some View {
        HStack(spacing: 0) {
            Text("")
                .frame(width: labelWidth, alignment: .leading)
            ForEach(1...12, id: \.self) { month in
                SectionLabel(
                    text: LedgerDate.shortMonth(month),
                    tint: month == summary.month.month ? Ink.text : Ink.dim
                )
                .frame(width: columnWidth, alignment: .trailing)
            }
            SectionLabel(text: String(localized: "Total"))
                .frame(width: columnWidth + 16, alignment: .trailing)
        }
    }

    private func line(_ label: String, values: [Int64], tint: Color, emphasis: Bool = false) -> some View {
        let font = emphasis ? Face.mono(12, .semibold) : Face.row
        let total = values.reduce(0, +)
        return HStack(spacing: 0) {
            Text(label)
                .font(font)
                .foregroundStyle(Ink.text)
                .frame(width: labelWidth, alignment: .leading)
            ForEach(Array(values.enumerated()), id: \.offset) { index, value in
                Text(LedgerMoney.bare(value))
                    .font(font)
                    .foregroundStyle(value == 0 ? Ink.dim : tint)
                    .frame(width: columnWidth, alignment: .trailing)
            }
            Text(LedgerMoney.bare(total))
                .font(Face.mono(12, .semibold))
                .foregroundStyle(total == 0 ? Ink.dim : tint)
                .frame(width: columnWidth + 16, alignment: .trailing)
        }
        .frame(height: 24)
    }

    private var labelWidth: CGFloat { 110 }
    private var columnWidth: CGFloat { 86 }
}
