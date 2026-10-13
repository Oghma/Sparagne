import SwiftUI

/// "Per persona": income, expenses, savings, rate and growth of the year so
/// far, one column per person. Not drawn with one person, nor under the
/// PERSONA filter (`SummaryView`).
///
/// The figures are the table's: expenses are USCITE, the envelopes without
/// a cap, so a person's column and the month table never disagree.
struct PersonYearCard: View {
    let year: YearSummary

    private static let labelWidth: CGFloat = 92
    private static let figureWidth: CGFloat = 92

    /// One line of the card: its name, and how one person's figure is
    /// written and tinted.
    private struct Line: Identifiable {
        let id: String
        let name: String
        let text: (PersonYear) -> String
        let tint: (PersonYear) -> Color
        /// What VoiceOver reads for one person: an amount with its symbol.
        let spoken: (PersonYear) -> String
    }

    private var lines: [Line] {
        [
            Line(
                id: "income", name: String(localized: "Income"),
                text: { LedgerMoney.bare($0.income) }, tint: { _ in Ink.text },
                spoken: { LedgerMoney.amount($0.income) }
            ),
            Line(
                id: "expenses", name: String(localized: "Expenses"),
                text: { LedgerMoney.bare($0.expenses) }, tint: { _ in Ink.text },
                spoken: { LedgerMoney.amount($0.expenses) }
            ),
            // Tinted as the savings card is: the one verdict of the card.
            Line(
                id: "savings", name: String(localized: "Savings"),
                text: { LedgerMoney.bare($0.savings) }, tint: { $0.savings >= 0 ? Ink.positive : Ink.negative },
                spoken: { LedgerMoney.amount($0.savings) }
            ),
            Line(
                id: "rate", name: String(localized: "Rate"),
                text: { SummaryText.rate($0.rate) ?? TransactionRow.placeholder },
                tint: { $0.rate == nil ? Ink.text3 : Ink.text2 },
                spoken: { SummaryText.rate($0.rate) ?? TransactionRow.placeholder }
            ),
            Line(
                id: "growth", name: String(localized: "Growth"),
                text: { SummaryText.signed(LedgerMoney.bare($0.growth), positive: $0.growth >= 0) },
                tint: { _ in Ink.text },
                spoken: { SummaryText.signed(LedgerMoney.amount($0.growth), positive: $0.growth >= 0) }
            ),
        ]
    }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                Text(String(localized: "By person"))
                    .font(Face.label)
                    .foregroundStyle(Ink.text2)
                    .accessibilityAddTraits(.isHeader)
                VStack(spacing: 0) {
                    header
                    ForEach(lines) { line in
                        row(line)
                    }
                }
            }
            // As tall as the envelope table beside it, so the two cards end
            // on the same line.
            .frame(maxHeight: .infinity, alignment: .top)
        }
    }

    private var header: some View {
        HStack(spacing: 0) {
            Text(verbatim: "")
                .frame(width: Self.labelWidth, alignment: .leading)
                .padding(.horizontal, Metrics.cellPad)
            ForEach(year.personFigures, id: \.person) { figures in
                Text(figures.person)
                    .lineLimit(1)
                    .padding(.horizontal, Metrics.cellPad)
                    .frame(minWidth: Self.figureWidth, maxWidth: .infinity, alignment: .trailing)
            }
        }
        .font(Face.ui(11, .medium))
        .foregroundStyle(Ink.text3)
        .frame(height: Metrics.headerHeight)
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
        .accessibilityHidden(true)
    }

    private func row(_ line: Line) -> some View {
        HStack(spacing: 0) {
            Text(line.name)
                .foregroundStyle(Ink.text2)
                .lineLimit(1)
                .frame(width: Self.labelWidth, alignment: .leading)
                .padding(.horizontal, Metrics.cellPad)
            ForEach(year.personFigures, id: \.person) { figures in
                Text(line.text(figures))
                    .foregroundStyle(line.tint(figures))
                    .lineLimit(1)
                    .padding(.horizontal, Metrics.cellPad)
                    .frame(minWidth: Self.figureWidth, maxWidth: .infinity, alignment: .trailing)
            }
        }
        .font(Face.ui(12))
        .frame(height: Metrics.rowHeight)
        .overlay(alignment: .bottom) {
            if line.id != lines.last?.id {
                Rectangle().fill(Ink.rowLine).frame(height: 1)
            }
        }
        // One row, one stop: "Income, elisa €9,200.00, matteo €7,400.00".
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            AccessibilityText.monthRow(
                month: line.name,
                figures: year.personFigures.map { ($0.person, line.spoken($0)) }
            )
        )
    }
}
