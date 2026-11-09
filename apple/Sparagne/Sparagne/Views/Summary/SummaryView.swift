import SwiftUI
import SparagneCore

/// The RIEPILOGO view: the year up to the month on
/// screen, the way the household's spreadsheet reads it.
///
/// Everything here is drawing: the arithmetic is `YearSummary` and the
/// models read with it (`AppStore.yearCategories`, `envelopeYear`, …), which
/// are tested on their own, so the views of this folder only decide colors
/// and widths.
///
/// Top to bottom: the things to do, the year's cards over the month's, the
/// flows, the month table, the people beside the
/// envelopes, the two charts, the cash flow from the income through the
/// flows to the categories, the categories beside the heaviest expenses,
/// and the months by categories. The month and the PERSONA filter are in
/// the top bar (`TopBar`).
struct SummaryView: View {
    let year: YearSummary
    /// The models read with the year, for the month and the PERSONA filter
    /// the top bar sets: the same `store.person` as the Mastro's, one for
    /// the window.
    let store: AppStore

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if !store.summaryTodo.isEmpty {
                SummaryTodoStrip(items: store.summaryTodo, store: store)
            }

            if let kpis = year.kpis {
                SummaryCards(year: year, kpis: kpis, fixedCosts: store.fixedCosts)
            }

            if isBlank {
                Text(String(localized: "No activity this year"))
                    .font(Face.row)
                    .foregroundStyle(Ink.text3)
                    .padding(.top, 4)
            } else {
                if !store.flowCards.isEmpty {
                    FlowCards(cards: store.flowCards, month: year.upTo)
                }
                YearTable(year: year)
                peopleAndEnvelopes
                SummaryCharts(year: year)
                if store.sankeyMonth != nil || store.sankeyYear != nil {
                    CashFlowSankeyCard(month: store.sankeyMonth, year: store.sankeyYear, upTo: year.upTo)
                }
                SideBySide {
                    CategoryYearCard(categories: store.yearCategories)
                } trailing: {
                    TopExpensesCard(expenses: store.topExpenses, store: store)
                }
                if let heat = store.categoryHeat, !heat.rows.isEmpty {
                    CategoryHeatCard(heat: heat)
                }
            }
        }
        .padding(Metrics.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Ink.sheet)
    }

    /// A fresh vault: not one live row, in any year. A year that is only
    /// empty still draws its table, of zeros and dashes, so every year reads
    /// the same: whether the year has rows after the month on screen, or
    /// before it in another year, changes nothing on screen.
    private var isBlank: Bool { store.peopleInRows.isEmpty }

    // MARK: - People and envelopes

    /// "Per persona" is there with two people or more and no filter: with
    /// one, its one column would repeat the cards. The envelope table then
    /// takes the whole width.
    private var showsPeople: Bool {
        year.person == nil && year.people.count >= 2 && !year.personFigures.isEmpty
    }

    /// The envelope table, `nil` without an envelope to list.
    private var envelopes: EnvelopeYear? {
        guard let envelopes = store.envelopeYear, !envelopes.rows.isEmpty else { return nil }
        return envelopes
    }

    @ViewBuilder
    private var peopleAndEnvelopes: some View {
        let table = envelopes.map { envelopes in
            EnvelopeYearTable(envelopes: envelopes, showsPeople: year.person == nil && envelopes.people.count > 1)
        }
        if showsPeople {
            // The card takes two fifths, the table the rest: the card has a
            // column per person, the table six and more. With no envelope
            // the card keeps its share, as a chart does alone on its line.
            SideBySide {
                PersonYearCard(year: year)
            } trailing: {
                if let table { table } else { Color.clear.frame(height: 0) }
            }
        } else if let table {
            table
        }
    }
}

/// Two cards on one line, the leading one two fifths of the page's width
/// and the trailing one the rest, as tall as the taller one so they end on
/// the same line. Two fifths of the window, not of the line: the window's
/// gutter is inside the difference, and at the narrowest window the table
/// still has the room its six columns and two people need.
private struct SideBySide<Leading: View, Trailing: View>: View {
    @ViewBuilder var leading: Leading
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            leading.containerRelativeFrame(.horizontal, count: 5, span: 2, spacing: 8)
            trailing.frame(maxWidth: .infinity)
        }
        .fixedSize(horizontal: false, vertical: true)
    }
}
