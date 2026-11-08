import SwiftUI

// MARK: - The cards

/// Two rows of five: the year so far above and the month on screen under
/// it, so a column reads one figure twice. Income, expenses, savings and
/// the rate in both; the year's row ends with how many months the money
/// would last, the month's with what the recurring expenses commit every
/// month. A line under a month's figure compares it with the month before,
/// the one under the year's rate gives the last twelve months. Only savings
/// is colored: expenses are the ordinary run of a month, not an alarm.
struct SummaryCards: View {
    let year: YearSummary
    let kpis: MonthKPIs
    /// `nil` without an enabled recurring expense: its card says so.
    let fixedCosts: FixedCosts?

    var body: some View {
        // Always five across: an adaptive grid opens a sixth, empty column on
        // a wide window, and the window is never narrower than five cards.
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(minimum: 150), spacing: 8), count: 5), spacing: 8) {
            yearCards
            monthCards
        }
    }

    @ViewBuilder
    private var yearCards: some View {
        KPICard(title: String(localized: "Income \u{00B7} \(yearName)"), value: LedgerMoney.bare(year.yearIncome))
        KPICard(title: String(localized: "Expenses \u{00B7} \(yearName)"), value: LedgerMoney.bare(year.yearExpense))
        KPICard(
            title: String(localized: "Savings \u{00B7} \(yearName)"),
            value: LedgerMoney.bare(year.yearSavings),
            tint: year.yearSavings >= 0 ? Ink.positive : Ink.negative
        )
        KPICard(
            title: String(localized: "Savings rate \u{00B7} \(yearName)"),
            value: rate(year.yearSavings, of: year.yearIncome),
            caption: String(localized: "\(rate(kpis.trailingSavings, of: kpis.trailingIncome)) in the last 12 months"),
            help: SummaryText.trailingRateHelp
        )
        KPICard(
            title: String(localized: "Autonomy"),
            value: year.autonomy.map { "\u{2248} " + String(localized: "\(SummaryText.months($0.months)) months") }
                ?? TransactionRow.placeholder,
            caption: year.autonomy.map { String(localized: "\(LedgerMoney.bare($0.monthlyExpense)) of expenses a month") },
            help: SummaryText.autonomyHelp
        )
    }

    @ViewBuilder
    private var monthCards: some View {
        KPICard(
            title: String(localized: "Income \u{00B7} \(name)"),
            value: LedgerMoney.bare(kpis.income),
            caption: kpis.previous.map { String(localized: "\(LedgerMoney.bare($0.income)) in \(previousName)") }
        )
        KPICard(
            title: String(localized: "Expenses \u{00B7} \(name)"),
            value: LedgerMoney.bare(kpis.expenses),
            caption: kpis.previous.map { String(localized: "\(LedgerMoney.bare($0.expenses)) in \(previousName)") }
        )
        KPICard(
            title: String(localized: "Savings \u{00B7} \(name)"),
            value: LedgerMoney.bare(kpis.savings),
            tint: kpis.savings >= 0 ? Ink.positive : Ink.negative,
            caption: deltaCaption,
            captionTint: (kpis.savingsDelta ?? 0) >= 0 ? Ink.positive : Ink.negative
        )
        KPICard(
            title: String(localized: "Savings rate \u{00B7} \(name)"),
            value: rate(kpis.savings, of: kpis.income),
            caption: kpis.previous.map { String(localized: "\(rate($0.savings, of: $0.income)) in \(previousName)") }
        )
        KPICard(
            title: String(localized: "Fixed costs \u{00B7} monthly"),
            value: fixedCosts.map { "\u{2248} " + LedgerMoney.bare($0.monthly) } ?? TransactionRow.placeholder,
            caption: fixedCaption
        )
    }

    /// "≈" as the Ricorrenze status line writes it: a weekly template is
    /// spread over the average month, so the figure is near, not exact. The
    /// line under it is the slice of an average month's income that is;
    /// blank with no income to measure it on.
    private var fixedCaption: String? {
        guard let fixedCosts else { return String(localized: "no recurring expenses") }
        guard let share = fixedCosts.shareOfIncome else { return nil }
        return String(localized: "\(SummaryText.percent(share)) of average income")
    }

    /// `"2026"`, as a string: a number would be grouped, `"2.026"`.
    private var yearName: String { String(year.year) }

    private var name: String { SummaryText.monthName(year.upTo.month) }

    private var previousName: String { SummaryText.monthName(year.upTo.adding(months: -1).month) }

    /// January has no month to compare with, so no line.
    private var deltaCaption: String? {
        kpis.savingsDelta.map { SummaryText.savingsChange($0, since: year.upTo.adding(months: -1).month) }
    }

    /// A rate over no income is a dash, never a fake 0%.
    private func rate(_ savings: Int64, of income: Int64) -> String {
        LedgerMoney.savingsRate(savings, of: income) ?? TransactionRow.placeholder
    }
}

/// One of the ten numbers across the top.
private struct KPICard: View {
    let title: String
    let value: String
    var tint: Color = Ink.text
    /// `nil` leaves the line blank, so the cards keep their height.
    var caption: String?
    var captionTint: Color = Ink.text2
    /// What the figure counts, on hover.
    var help: String?

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(Face.footnote)
                    .foregroundStyle(Ink.text3)
                Text(value)
                    .font(Face.display)
                    .foregroundStyle(tint)
                Text(caption ?? " ")
                    .font(Face.row)
                    .foregroundStyle(captionTint)
            }
        }
        // One card, one stop: the title already names the month, so VoiceOver
        // reads "Income · September, 1,234.56, 4,250.00 in August" instead of
        // three separate swipes.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(AccessibilityText.card(title: title, value: value, caption: caption))
        .help(ifAny: help)
    }
}

// MARK: - Flows

/// One card per active flow but Unallocated, in `FlowCard.cards` order: the
/// capped ones still filling, the open ones, the full ones.
struct FlowCards: View {
    let cards: [FlowCard]
    /// The month on screen, which every card's last line is about.
    let month: MonthKey

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                SectionLabel(text: String(localized: "Flows"), tint: Ink.text2)
                Text(String(localized: "balance at month end \u{00B7} in and out in the month, moves included"))
                    .font(Face.label)
                    .foregroundStyle(Ink.text3)
                Spacer(minLength: 0)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 240), spacing: 8)], spacing: 8) {
                ForEach(cards) { FlowCardView(card: $0, month: month) }
            }
        }
    }
}

/// A capped flow and an open one draw the same four lines, the bar's left
/// empty on an open one, so the cards of a row end on the same line.
private struct FlowCardView: View {
    let card: FlowCard
    let month: MonthKey

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    Text(card.name)
                        .font(Face.ui(12, .semibold))
                        .foregroundStyle(Ink.text)
                        .lineLimit(1)
                    if let gauge = card.gauge {
                        CapTag(kind: gauge.kind)
                        if card.isComplete {
                            SetupTag(text: String(localized: "complete"), fixed: false)
                        }
                    }
                }
                HStack(alignment: .firstTextBaseline) {
                    Text(LedgerMoney.bare(figure))
                        .font(Face.ui(15, .semibold))
                        .foregroundStyle(figure < 0 ? Ink.negative : Ink.text)
                    Spacer(minLength: 8)
                    if let gauge = card.gauge {
                        Text(String(localized: "of \(LedgerMoney.bare(gauge.cap)) \u{00B7} \(percent(gauge))"))
                            .font(Face.row)
                            .foregroundStyle(Ink.text3)
                    }
                }
                if let gauge = card.gauge {
                    MeterBar(
                        fraction: gauge.fraction,
                        tint: card.isComplete ? Ink.positive : Ink.chartIncome,
                        height: 5,
                        rounded: true
                    )
                } else {
                    Color.clear.frame(height: 5)
                }
                Text(moves)
                    .font(Face.row)
                    .foregroundStyle(Ink.text3)
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
            }
        }
        // The bar is a drawn shape with no text of its own: the flow's name
        // is the label, and the figures its value.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(card.name)
        .accessibilityValue(spoken)
    }

    /// The balance, or what fills the cap: the cumulative income on a cap
    /// on the income.
    private var figure: Int64 { card.gauge?.filled ?? card.balance }

    /// "+400,00 / −312,00 a ottobre": what came in and what went out.
    private var moves: String {
        let monthName = SummaryText.monthName(month.month)
        guard card.monthIncome != 0 || card.monthExpense != 0 else {
            return String(localized: "no moves in \(monthName)")
        }
        let figures = SummaryText.signed(PanelMoney.bare(card.monthIncome), positive: true)
            + " / " + PanelMoney.bare(-card.monthExpense)
        return String(localized: "\(figures) in \(monthName)")
    }

    private var spoken: String {
        let figures = card.gauge.map {
            AccessibilityText.fundGauge(
                percent: percent($0),
                filled: LedgerMoney.amount($0.filled),
                cap: LedgerMoney.amount($0.cap)
            )
        } ?? LedgerMoney.amount(card.balance)
        let complete = card.isComplete ? String(localized: "complete") : nil
        return [figures, complete, moves].compactMap(\.self).joined(separator: ", ")
    }

    private func percent(_ gauge: FundGauge) -> String {
        LedgerMoney.percent(gauge.filled, of: gauge.cap, decimals: 0) ?? TransactionRow.placeholder
    }
}

/// What a fund's cap is measured on, the small tag after its name: on a
/// fund card and on the fund's line of the envelope table, so the two read
/// the same.
struct CapTag: View {
    let kind: FundGauge.Kind

    var body: some View {
        SetupTag(text: Self.text(kind), fixed: false)
    }

    static func text(_ kind: FundGauge.Kind) -> String {
        switch kind {
        case .balance: String(localized: "cap on balance")
        case .income: String(localized: "cap on income")
        }
    }
}
