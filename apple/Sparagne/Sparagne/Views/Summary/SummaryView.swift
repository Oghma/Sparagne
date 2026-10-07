import SwiftUI
import SparagneCore

/// The RIEPILOGO view (`docs/v2/UI.md` §2.2): the year up to the month on
/// screen, the way the household's spreadsheet reads it.
///
/// Everything here is drawing: the arithmetic is `YearSummary` and
/// `MonthKPIs`, which are tested on their own, so the views of this folder
/// only decide colors and widths.
struct SummaryView: View {
    let year: YearSummary
    /// Kept for the chrome that will need the vault (names, filters); the
    /// numbers all come from `year`.
    let store: AppStore

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            heading

            if let kpis = year.kpis {
                MonthCards(kpis: kpis, month: year.upTo)
            }

            if !year.funds.isEmpty {
                FundCards(funds: year.funds)
            }

            if isBlank {
                Text(String(localized: "No activity this year"))
                    .font(Face.row)
                    .foregroundStyle(Ink.text3)
                    .padding(.top, 4)
            } else {
                YearTable(year: year)
                SummaryCharts(year: year)
            }
        }
        .padding(Metrics.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Ink.sheet)
    }

    /// A fresh vault: nothing carried in, nothing moved all year.
    private var isBlank: Bool {
        year.initial == 0 && year.months.allSatisfy {
            $0.income == 0 && $0.cashExpense == 0 && $0.fundExpense == 0 && $0.total == 0
        }
    }

    /// `2026  fino a ottobre`. A past year has no "up to": it is full.
    private var heading: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(verbatim: "\(year.year)")
                .font(Face.ui(18, .semibold))
                .foregroundStyle(Ink.text)
            if year.year == year.upTo.year {
                Text(String(localized: "up to \(SummaryText.monthName(year.upTo.month))"))
                    .font(Face.ui(13))
                    .foregroundStyle(Ink.text2)
            }
        }
    }
}
