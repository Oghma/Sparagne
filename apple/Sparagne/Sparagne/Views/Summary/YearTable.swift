import SwiftUI

/// The RIEPILOGO's month by month table (`docs/v2/UI.md` §2.2): the opening
/// cash fund, twelve months, the year's sums. It is drawing only; which row
/// is on screen, blank or going down comes from `YearSummary.tableRows`.
struct YearTable: View {
    let year: YearSummary

    private static let monthWidth: CGFloat = 116
    private static let figureWidth: CGFloat = 92

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .firstTextBaseline) {
                    Text(String(localized: "Month by month"))
                        .font(Face.label)
                        .foregroundStyle(Ink.text2)
                    Spacer(minLength: 8)
                    Text(String(localized: "Total = savings + cash fund \u{2212} fund expenses"))
                        .font(Face.label)
                        .foregroundStyle(Ink.text3)
                        .lineLimit(1)
                }
                // Wide enough, the columns share the card; too narrow, the
                // table keeps its widths and scrolls.
                ViewThatFits(in: .horizontal) {
                    table(flexible: true)
                    ScrollView(.horizontal) { table(flexible: false) }
                        .scrollBounceBehavior(.basedOnSize)
                }
            }
        }
    }

    private func table(flexible: Bool) -> some View {
        VStack(spacing: 0) {
            header(flexible: flexible)
            ForEach(Array(year.tableRows.enumerated()), id: \.offset) { _, row in
                line(row, flexible: flexible)
            }
        }
        .frame(minWidth: Self.monthWidth + 2 * Metrics.cellPad + CGFloat(6 + year.people.count) * Self.figureWidth)
    }

    // MARK: - Header

    private func header(flexible: Bool) -> some View {
        var titles = [
            String(localized: "Income"),
            String(localized: "Expenses"),
            String(localized: "Savings"),
            String(localized: "Cash fund"),
            String(localized: "Fund expenses"),
            String(localized: "Total"),
        ]
        titles.append(contentsOf: year.people)
        return HStack(spacing: 0) {
            Text(String(localized: "Month"))
                .frame(width: Self.monthWidth, alignment: .leading)
                .padding(.horizontal, Metrics.cellPad)
            ForEach(Array(titles.enumerated()), id: \.offset) { _, title in
                Text(title)
                    .lineLimit(1)
                    .padding(.horizontal, Metrics.cellPad)
                    .frame(minWidth: Self.figureWidth, maxWidth: flexible ? .infinity : Self.figureWidth, alignment: .trailing)
            }
        }
        .font(Face.ui(11, .medium))
        .foregroundStyle(Ink.text3)
        .frame(height: Metrics.headerHeight)
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
    }

    // MARK: - Rows

    private func line(_ row: YearTableRow, flexible: Bool) -> some View {
        let dim = row.isFuture
        let isSum = row.kind == .sum
        let weight: Font.Weight = isSum ? .semibold : .regular

        return HStack(spacing: 0) {
            nameCell(row)
                .frame(width: Self.monthWidth, alignment: .leading)
                .padding(.horizontal, Metrics.cellPad)
            figure(row.income, row: row, flexible: flexible, weight: weight)
            figure(row.cashExpense, row: row, flexible: flexible, weight: weight)
            figure(
                row.savings, row: row, flexible: flexible, weight: weight,
                tint: isSum ? ((row.savings ?? 0) >= 0 ? Ink.positive : Ink.negative) : ((row.savings ?? 0) < 0 ? Ink.negative : Ink.text)
            )
            figure(row.carried, row: row, flexible: flexible, weight: weight, tint: Ink.text2)
            figure(row.fundExpense, row: row, flexible: flexible, weight: weight)
            totalCell(row, flexible: flexible)
            ForEach(Array(year.people.enumerated()), id: \.offset) { index, _ in
                figure(
                    row.totalByPerson.indices.contains(index) ? row.totalByPerson[index] : nil,
                    row: row, flexible: flexible, weight: weight, tint: isSum ? Ink.text : Ink.text2
                )
            }
        }
        .font(Face.ui(12, weight))
        .foregroundStyle(row.kind == .opening ? Ink.text2 : (dim ? Ink.text3 : Ink.text))
        .frame(height: isSum ? 28 : Metrics.rowHeight)
        .background {
            if row.isCurrent {
                RoundedRectangle(cornerRadius: 4).fill(Ink.raised)
            }
        }
        .overlay(alignment: isSum ? .top : .bottom) {
            // The sum is closed off by a stronger rule above it; the rows
            // above share the faint one, except the month on screen, whose
            // background already sets it apart.
            if isSum {
                Rectangle().fill(Ink.line2).frame(height: 1)
            } else if !row.isCurrent {
                Rectangle().fill(Ink.rowLine).frame(height: 1)
            }
        }
        // One row, one stop: "September, Income 9,200.00, Expenses …" rather
        // than a dozen separate cells with no idea which row they belong to.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label(row))
    }

    @ViewBuilder
    private func nameCell(_ row: YearTableRow) -> some View {
        switch row.kind {
        case .opening:
            Text(String(localized: "Start of year"))
        case .month(let month):
            HStack(spacing: 6) {
                Text(SummaryText.capitalized(SummaryText.monthName(month.month)))
                    .lineLimit(1)
                if row.isCurrent {
                    Text(String(localized: "current"))
                        .font(Face.ui(10, .semibold))
                        .foregroundStyle(Ink.accent)
                        .padding(.horizontal, 5)
                        .frame(height: 16)
                        .background(Ink.accent.opacity(0.14), in: RoundedRectangle(cornerRadius: 4))
                }
            }
        case .sum:
            Text("\(year.year)")
        }
    }

    /// An empty cell on the opening row and the sum, a dim dash on a future
    /// month: the two read differently ("nothing here" against "not yet").
    private func figure(
        _ value: Int64?, row: YearTableRow, flexible: Bool, weight: Font.Weight, tint: Color? = nil
    ) -> some View {
        Group {
            if let value {
                Text(LedgerMoney.bare(value))
                    .foregroundStyle(value == 0 ? Ink.text3 : (tint ?? Ink.text))
            } else if row.isFuture {
                Text(TransactionRow.placeholder).foregroundStyle(Ink.text3)
            } else {
                Text(verbatim: "")
            }
        }
        .lineLimit(1)
        .padding(.horizontal, Metrics.cellPad)
        .frame(minWidth: Self.figureWidth, maxWidth: flexible ? .infinity : Self.figureWidth, alignment: .trailing)
    }

    /// TOTALE carries the verdict of the month: down on the line before is the
    /// one thing worth seeing at a glance.
    private func totalCell(_ row: YearTableRow, flexible: Bool) -> some View {
        Group {
            if let total = row.total {
                let tint = row.trendUp ? Ink.positive : Ink.negative
                HStack(spacing: 4) {
                    if row.kind != .opening {
                        Text(row.trendUp ? "\u{25B2}" : "\u{25BC}")
                            .font(Face.ui(9))
                            .foregroundStyle(tint)
                    }
                    Text(LedgerMoney.bare(total))
                        .foregroundStyle(row.kind == .opening ? Ink.text : tint)
                }
                .fontWeight(.semibold)
            } else if row.isFuture {
                Text(TransactionRow.placeholder).foregroundStyle(Ink.text3)
            } else {
                Text(verbatim: "")
            }
        }
        .lineLimit(1)
        .padding(.horizontal, Metrics.cellPad)
        .frame(minWidth: Self.figureWidth, maxWidth: flexible ? .infinity : Self.figureWidth, alignment: .trailing)
    }

    // MARK: - Accessibility

    /// "Start of year, Total €750.00, elisa €200.00", "September, Income …",
    /// "October, future month" (`docs/v2/UI.md` §2.2: a future month is drawn
    /// blank), "2026, Income …".
    private func label(_ row: YearTableRow) -> String {
        let name: String
        switch row.kind {
        case .opening: name = String(localized: "Start of year")
        case .month(let month): name = SummaryText.monthName(month.month)
        case .sum: name = "\(year.year)"
        }
        guard !row.isFuture else {
            return String(localized: "\(name), future month")
        }
        var figures: [(String, String)] = []
        func add(_ title: String, _ value: Int64?) {
            if let value { figures.append((title, LedgerMoney.amount(value))) }
        }
        add(String(localized: "Income"), row.income)
        add(String(localized: "Expenses"), row.cashExpense)
        add(String(localized: "Savings"), row.savings)
        add(String(localized: "Cash fund"), row.carried)
        add(String(localized: "Fund expenses"), row.fundExpense)
        add(String(localized: "Total"), row.total)
        for (index, person) in year.people.enumerated() where row.totalByPerson.indices.contains(index) {
            figures.append((person, LedgerMoney.amount(row.totalByPerson[index])))
        }
        return AccessibilityText.monthRow(month: name, figures: figures)
    }
}
