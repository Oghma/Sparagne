import SwiftUI
import SparagneCore

/// The RIEPILOGO view (`docs/v2/UI.md` §2.2): four cards, the person matrix
/// and who spent what on the left, the year's bars and the month's heaviest
/// expenses on the right.
struct SummaryView: View {
    let summary: LedgerSummary
    let store: AppStore

    var body: some View {
        VStack(spacing: 10) {
            cards
            HStack(alignment: .top, spacing: 10) {
                VStack(spacing: 10) {
                    PersonMatrix(summary: summary, store: store)
                    SpendByPerson(summary: summary)
                    Spacer(minLength: 0)
                }
                VStack(spacing: 10) {
                    IncomeVersusExpenses(summary: summary)
                    TopExpenses(top: summary.top)
                    Spacer(minLength: 0)
                }
            }
        }
        .padding(10)
        .background(Ink.bg)
    }

    private var cards: some View {
        HStack(spacing: 10) {
            StatCard(
                title: String(localized: "Income"),
                value: LedgerMoney.amount(summary.totals.income),
                tint: Ink.positive,
                caption: "\(summary.people.count) \(String(localized: "people"))"
            )
            StatCard(
                title: String(localized: "Expenses"),
                value: LedgerMoney.amount(summary.totals.netExpense),
                tint: Ink.negative,
                caption: refundCaption
            )
            StatCard(
                title: String(localized: "Savings"),
                value: LedgerMoney.amount(summary.savings),
                tint: Ink.text,
                caption: deltaCaption
            )
            StatCard(
                title: String(localized: "Rate"),
                value: LedgerMoney.percent(summary.savings, of: summary.totals.income) ?? "—",
                tint: Ink.text,
                caption: nil,
                meter: rate
            )
        }
    }

    private var refundCaption: String? {
        guard summary.totals.refund > 0 else { return nil }
        return "\(String(localized: "net of refunds")) \(LedgerMoney.bare(summary.totals.refund))"
    }

    private var deltaCaption: String? {
        guard let delta = LedgerMoney.delta(current: summary.savings, previous: summary.previousSavings) else {
            return nil
        }
        let previous = summary.month.adding(months: -1)
        return "\(delta) \(String(localized: "vs")) \(LedgerDate.fullMonth(previous.month).lowercased())"
    }

    /// Clamped: a month that saved more than it earned (a refunded expense
    /// from an earlier month) would otherwise overflow the bar.
    private var rate: Double? {
        guard summary.totals.income > 0 else { return nil }
        return Double(summary.savings) / Double(summary.totals.income)
    }
}

/// One of the four numbers across the top.
struct StatCard: View {
    let title: String
    let value: String
    let tint: Color
    var caption: String?
    var meter: Double?

    var body: some View {
        Panel(padding: 16) {
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(text: title)
                Text(value)
                    .font(Face.display)
                    .foregroundStyle(tint)
                if let meter {
                    MeterBar(fraction: meter, tint: Ink.positive, height: 4)
                        .padding(.top, 2)
                } else {
                    Text(caption ?? " ")
                        .font(Face.footnote)
                        .foregroundStyle(Ink.dim)
                }
            }
        }
    }
}

/// The stacked bar of "chi ha speso cosa": one segment per person, widest
/// first, with the share written underneath.
struct SpendByPerson: View {
    let summary: LedgerSummary

    private var shares: [(person: String, amount: Int64)] { summary.spendByPerson }
    private var total: Int64 { shares.reduce(0) { $0 + $1.amount } }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 10) {
                SectionLabel(text: String(localized: "Who spent what"))
                if total == 0 {
                    Text(String(localized: "Nothing spent this month"))
                        .font(Face.row)
                        .foregroundStyle(Ink.dim)
                } else {
                    GeometryReader { geometry in
                        HStack(spacing: 1) {
                            ForEach(Array(shares.enumerated()), id: \.offset) { index, share in
                                Rectangle()
                                    .fill(index == 0 ? Ink.accent : Ink.muted)
                                    .frame(width: geometry.size.width * fraction(share.amount))
                            }
                        }
                    }
                    .frame(height: 22)

                    HStack {
                        ForEach(Array(shares.enumerated()), id: \.offset) { index, share in
                            if index > 0 { Spacer(minLength: 12) }
                            Text(caption(share))
                                .font(Face.footnote)
                                .foregroundStyle(Ink.dim)
                        }
                    }
                }
            }
        }
    }

    private func fraction(_ amount: Int64) -> Double {
        total > 0 ? Double(amount) / Double(total) : 0
    }

    private func caption(_ share: (person: String, amount: Int64)) -> String {
        let percent = LedgerMoney.percent(share.amount, of: total) ?? ""
        return "\(share.person) \(LedgerMoney.bare(share.amount)) \u{00B7} \(percent)"
    }
}

/// Income against expenses for the twelve months of the year, the month on
/// screen picked out.
struct IncomeVersusExpenses: View {
    let summary: LedgerSummary

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 12) {
                SectionLabel(text: "\(String(localized: "Income vs expenses")) \u{00B7} \(summary.month.year)")
                PairedBars(
                    pairs: summary.year.map { (Double($0.income), Double($0.netExpense)) },
                    labels: (1...12).map { LedgerDate.shortMonth($0) },
                    highlighted: summary.month.month - 1
                )
                .frame(height: 190)
            }
        }
    }
}

/// Two bars per slot, sharing one scale.
struct PairedBars: View {
    let pairs: [(Double, Double)]
    let labels: [String]
    var highlighted: Int?

    private var scale: Double {
        let peak = pairs.flatMap { [$0.0, $0.1] }.max() ?? 0
        return peak > 0 ? peak : 1
    }

    var body: some View {
        HStack(alignment: .bottom, spacing: 6) {
            ForEach(Array(pairs.enumerated()), id: \.offset) { index, pair in
                let active = index == highlighted
                VStack(spacing: 5) {
                    GeometryReader { geometry in
                        HStack(alignment: .bottom, spacing: 2) {
                            bar(pair.0, in: geometry.size.height, tint: active ? Ink.positive : Ink.mutedPositive)
                            bar(pair.1, in: geometry.size.height, tint: active ? Ink.negative : Ink.muted)
                        }
                        .frame(maxHeight: .infinity, alignment: .bottom)
                    }
                    Text(labels.indices.contains(index) ? labels[index] : "")
                        .font(Face.mono(9))
                        .foregroundStyle(active ? Ink.text : Ink.dim)
                }
            }
        }
    }

    private func bar(_ value: Double, in height: CGFloat, tint: Color) -> some View {
        Rectangle()
            .fill(tint)
            .frame(height: max(height * (value / scale), value == 0 ? 0 : 1))
            .frame(maxWidth: .infinity, alignment: .bottom)
    }
}

/// The month's heaviest expenses, straight from `top_expenses`.
struct TopExpenses: View {
    let top: [TopExpense]

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 9) {
                SectionLabel(text: String(localized: "Top expenses this month"))
                if top.isEmpty {
                    Text(String(localized: "Nothing spent this month"))
                        .font(Face.row)
                        .foregroundStyle(Ink.dim)
                } else {
                    ForEach(top, id: \.transactionId) { expense in
                        HStack(spacing: 10) {
                            Text(expense.note ?? TransactionRow.placeholder)
                                .font(Face.row)
                                .foregroundStyle(Ink.text)
                                .lineLimit(1)
                            Spacer(minLength: 12)
                            Text(expense.category)
                                .font(Face.row)
                                .foregroundStyle(Ink.dim)
                                .lineLimit(1)
                            Text(LedgerMoney.bare(expense.amount))
                                .font(Face.row)
                                .foregroundStyle(Ink.text)
                                .frame(width: 92, alignment: .trailing)
                        }
                        .frame(height: 22)
                    }
                }
            }
        }
    }
}
