import SwiftUI
import SparagneCore

/// "Prossimi 30 giorni" (`docs/v2/UI.md` §2.5): one tile per period ahead,
/// from tomorrow, with what the window adds up to on each side. Read only:
/// a period can be recorded once it is due, never before.
struct RecurringAgendaCard: View {
    let store: AppStore

    private static let columns = [GridItem(.adaptive(minimum: 112), spacing: 6)]

    var body: some View {
        let agenda = RecurringAgenda(tiles: store.upcomingRecurring)
        Panel(padding: 0) {
            VStack(alignment: .leading, spacing: 0) {
                RecurringCardHeader(title: String(localized: "Next 30 days")) {
                    totals(agenda)
                }
                if agenda.tiles.isEmpty {
                    Text(String(localized: "Nothing in the next 30 days"))
                        .font(Face.ui(12))
                        .foregroundStyle(Ink.text3)
                        .padding(.bottom, 2)
                } else {
                    LazyVGrid(columns: Self.columns, alignment: .leading, spacing: 6) {
                        ForEach(agenda.tiles) { tile($0) }
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
        }
    }

    private func totals(_ agenda: RecurringAgenda) -> some View {
        HStack(spacing: 4) {
            Text(String(localized: "Expected expenses")).foregroundStyle(Ink.text3)
            Text(LedgerMoney.bare(agenda.expenses)).foregroundStyle(Ink.text)
            Text(String(localized: "Expected income"))
                .foregroundStyle(Ink.text3)
                .padding(.leading, 10)
            Text(LedgerMoney.bare(agenda.income)).foregroundStyle(Ink.positive)
        }
        .font(Face.ui(11.5, .medium))
        .accessibilityElement(children: .combine)
    }

    private func tile(_ period: UpcomingPeriod) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(RecurringDayText.weekday(period.date))
                .font(Face.ui(11))
                .foregroundStyle(Ink.text3)
            Text(RecurringTitle.of(period.template))
                .font(Face.ui(12.5, .medium))
                .foregroundStyle(Ink.text)
            Text(RecurringAmount.signed(period.template))
                .font(Face.ui(12))
                .foregroundStyle(RecurringAmount.color(period.template))
        }
        .lineLimit(1)
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Ink.sheet, in: RoundedRectangle(cornerRadius: 6))
        .accessibilityElement(children: .combine)
    }
}
