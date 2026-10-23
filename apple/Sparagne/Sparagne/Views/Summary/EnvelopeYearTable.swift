import SwiftUI

/// "Buste": every active envelope, Unallocated first, with its balance at
/// the end of the month on screen, what came in and went out in the year,
/// moves between envelopes included, and who took it out. An envelope with
/// a cap carries the tag its card has, so the two read as the same envelope.
///
/// Not drawn without an envelope (`SummaryView`); the split by person goes
/// under the PERSONA filter, where it would be one column repeating USCITE.
struct EnvelopeYearTable: View {
    let envelopes: EnvelopeYear
    /// One column per person of `envelopes.people`.
    let showsPeople: Bool

    /// A name and a fund's tag, `tetto sulle entrate` the longest of them,
    /// and the cell's padding. Scrolling, the column keeps it, so every
    /// row's figures start at the same place whatever its name.
    private static let nameWidth: CGFloat = 168 + 2 * Metrics.cellPad
    private static let figureWidth: CGFloat = 84

    private var people: [String] { showsPeople ? envelopes.people : [] }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .firstTextBaseline) {
                    SectionLabel(text: String(localized: "Envelopes"), tint: Ink.text2)
                        .accessibilityAddTraits(.isHeader)
                    Spacer(minLength: 8)
                    Text(String(localized: "balance at month end \u{00B7} in and out in the year, moves included"))
                        .font(Face.label)
                        .foregroundStyle(Ink.text3)
                        .lineLimit(1)
                }
                FittedTable { table(flexible: $0) }
            }
            // As tall as the person card beside it, so the two cards end on
            // the same line.
            .frame(maxHeight: .infinity, alignment: .top)
        }
    }

    private func table(flexible: Bool) -> some View {
        VStack(spacing: 0) {
            header(flexible: flexible)
            ForEach(envelopes.rows) { row in
                line(row, flexible: flexible)
            }
        }
        .frame(minWidth: Self.nameWidth + CGFloat(3 + people.count) * Self.figureWidth)
    }

    // MARK: - Header

    private func header(flexible: Bool) -> some View {
        var titles = [
            String(localized: "Balance"),
            String(localized: "Income"),
            String(localized: "Expenses"),
        ]
        titles.append(contentsOf: people)
        return SetupHeaderRow {
            Text(String(localized: "Envelope"))
                .fittedCell(Self.nameWidth, flexible: flexible)
            ForEach(Array(titles.enumerated()), id: \.offset) { _, title in
                Text(title)
                    .tableCell(width: Self.figureWidth, alignment: .trailing)
            }
        }
    }

    // MARK: - Rows

    private func line(_ row: EnvelopeYearRow, flexible: Bool) -> some View {
        HStack(spacing: 0) {
            HStack(spacing: 6) {
                Text(name(row))
                    .foregroundStyle(row.isUnallocated ? Ink.text2 : Ink.text)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if let kind = row.capKind {
                    // The name gives way first: a cut tag says nothing.
                    CapTag(kind: kind).fixedSize()
                }
            }
            .fittedCell(Self.nameWidth, flexible: flexible)
            figure(row.balance, tint: row.balance < 0 ? Ink.negative : Ink.text)
            figure(row.income, tint: Ink.text)
            figure(row.expense, tint: Ink.text)
            ForEach(Array(people.enumerated()), id: \.offset) { index, _ in
                figure(
                    row.expenseByPerson.indices.contains(index) ? row.expenseByPerson[index] : 0,
                    tint: Ink.text2
                )
            }
        }
        .font(Face.row)
        .frame(height: Metrics.rowHeight)
        .overlay(alignment: .bottom) {
            if row.id != envelopes.rows.last?.id {
                Rectangle().fill(Ink.rowLine).frame(height: 1)
            }
        }
        // One row, one stop: "Vacanze, cap on balance, Balance €1,200.00,
        // Income …, Expenses …, elisa €80.00".
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label(row))
    }

    /// A zero reads faint, as in the month table: an envelope nobody spent
    /// from is not news.
    private func figure(_ value: Int64, tint: Color) -> some View {
        Text(LedgerMoney.bare(value))
            .foregroundStyle(value == 0 ? Ink.text3 : tint)
            .tableCell(width: Self.figureWidth, alignment: .trailing)
    }

    /// Unallocated is named in the user's language, as everywhere else
    /// (`NameBook`).
    private func name(_ row: EnvelopeYearRow) -> String {
        row.isUnallocated ? NameBook.unallocatedLabel : row.name
    }

    private func label(_ row: EnvelopeYearRow) -> String {
        var title = name(row)
        if let kind = row.capKind { title += ", \(CapTag.text(kind))" }
        var figures = [
            (String(localized: "Balance"), LedgerMoney.amount(row.balance)),
            (String(localized: "Income"), LedgerMoney.amount(row.income)),
            (String(localized: "Expenses"), LedgerMoney.amount(row.expense)),
        ]
        for (index, person) in people.enumerated() where row.expenseByPerson.indices.contains(index) {
            figures.append((person, LedgerMoney.amount(row.expenseByPerson[index])))
        }
        return AccessibilityText.monthRow(month: title, figures: figures)
    }
}
