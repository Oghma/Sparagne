import SwiftUI

/// Months x categories: each month's net expense per category, shaded
/// against the heaviest cell. Full width: twelve columns want the room.
///
/// The lines are the year's category list, in its order, so a category
/// reads across from its total in the card above. A month after the one on
/// screen is a dash, as in the month table: not yet, rather than nothing.
/// Not drawn without a line (`SummaryView`).
struct CategoryHeatCard: View {
    let heat: CategoryHeat

    private static let nameWidth: CGFloat = 120
    private static let cellWidth: CGFloat = 68

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                Text(String(localized: "Months \u{00D7} categories"))
                    .font(Face.label)
                    .foregroundStyle(Ink.text2)
                    .accessibilityAddTraits(.isHeader)
                // Wide enough, the months share the card; too narrow, they
                // keep their widths and scroll, as the month table does.
                ViewThatFits(in: .horizontal) {
                    grid(flexible: true)
                    ScrollView(.horizontal) { grid(flexible: false) }
                        .scrollBounceBehavior(.basedOnSize)
                }
            }
        }
    }

    private func grid(flexible: Bool) -> some View {
        VStack(spacing: 0) {
            header(flexible: flexible)
            ForEach(heat.rows) { row in
                line(row, flexible: flexible)
            }
        }
        .frame(minWidth: Self.nameWidth + 2 * Metrics.cellPad + 12 * Self.cellWidth)
    }

    private func header(flexible: Bool) -> some View {
        HStack(spacing: 0) {
            Text(String(localized: "Category"))
                .lineLimit(1)
                .padding(.horizontal, Metrics.cellPad)
                .frame(width: Self.nameWidth + 2 * Metrics.cellPad, alignment: .leading)
            ForEach(1...12, id: \.self) { month in
                Text(LedgerDate.shortMonth(month))
                    .lineLimit(1)
                    .padding(.horizontal, Metrics.cellPad)
                    .modifier(MonthColumn(flexible: flexible))
            }
        }
        .font(Face.ui(11, .medium))
        .foregroundStyle(Ink.text3)
        .frame(height: Metrics.headerHeight)
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
        .accessibilityHidden(true)
    }

    private func line(_ row: CategoryHeatRow, flexible: Bool) -> some View {
        let name = SummaryText.categoryName(row.name, isSystem: row.isSystem, isOther: row.isOther)
        return HStack(spacing: 0) {
            Text(name)
                .font(Face.ui(12))
                .foregroundStyle(row.isOther ? Ink.text2 : Ink.text)
                .lineLimit(1)
                .truncationMode(.tail)
                .padding(.horizontal, Metrics.cellPad)
                .frame(width: Self.nameWidth + 2 * Metrics.cellPad, alignment: .leading)
            ForEach(1...12, id: \.self) { month in
                cell(row.cells.indices.contains(month - 1) ? row.cells[month - 1] : 0, month: month)
                    .modifier(MonthColumn(flexible: flexible))
            }
        }
        .frame(height: Metrics.rowHeight)
        // One row, one stop: "Spesa, January €320.00, February €280.00, …",
        // up to the month on screen.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label(name: name, row: row))
    }

    /// A shaded tile, darker the closer it comes to the heaviest cell of the
    /// grid; a month with nothing spent is left unshaded, so the eye reads
    /// the year's pattern and not a wall of tiles.
    private func cell(_ value: Int64, month: Int) -> some View {
        Group {
            if month > heat.elapsedMonths {
                Text(TransactionRow.placeholder)
                    .foregroundStyle(Ink.text3)
                    .padding(.horizontal, Metrics.cellPad)
            } else {
                Text(LedgerMoney.bare(value))
                    .foregroundStyle(value > 0 ? Ink.text : Ink.text3)
                    .padding(.horizontal, 6)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .trailing)
                    .background {
                        if value > 0 {
                            RoundedRectangle(cornerRadius: 3)
                                .fill(Ink.chartExpense.opacity(0.08 + 0.5 * heat.intensity(value)))
                        }
                    }
                    .padding(.horizontal, 2)
                    .padding(.vertical, 1)
            }
        }
        .font(Face.ui(11))
        .lineLimit(1)
    }

    private func label(name: String, row: CategoryHeatRow) -> String {
        let figures = (1...12).compactMap { month -> (String, String)? in
            guard month <= heat.elapsedMonths, row.cells.indices.contains(month - 1) else { return nil }
            return (SummaryText.monthName(month), LedgerMoney.amount(row.cells[month - 1]))
        }
        return AccessibilityText.monthRow(month: name, figures: figures)
    }

    /// A month's column: a share of what the names leave when the grid fits
    /// the card, its own width when it scrolls.
    private struct MonthColumn: ViewModifier {
        let flexible: Bool

        func body(content: Content) -> some View {
            content.frame(
                minWidth: CategoryHeatCard.cellWidth,
                maxWidth: flexible ? .infinity : CategoryHeatCard.cellWidth,
                alignment: .trailing
            )
        }
    }
}
