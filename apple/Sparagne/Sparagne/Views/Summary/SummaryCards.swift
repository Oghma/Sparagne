import SwiftUI

// MARK: - The month's cards

/// The five numbers of the month on screen: income,
/// expenses, savings and the rate, each with a line under it that compares it
/// with the month before or the year so far, and what the recurring expenses
/// commit every month. Only savings is colored: expenses are the ordinary
/// run of a month, not an alarm.
struct MonthCards: View {
    let kpis: MonthKPIs
    let month: MonthKey
    /// `nil` without an enabled recurring expense: the fifth card says so.
    let fixedCosts: FixedCosts?

    var body: some View {
        // Always five across: an adaptive grid opens a sixth, empty column on
        // a wide window, and the window is never narrower than five cards.
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(minimum: 150), spacing: 8), count: 5), spacing: 8) {
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
                title: String(localized: "Savings rate"),
                value: rate(kpis.savings, of: kpis.income),
                caption: String(localized: "\(rate(kpis.yearSavings, of: kpis.yearIncome)) year to date")
            )
            KPICard(
                title: String(localized: "Fixed costs \u{00B7} monthly"),
                value: fixedCosts.map { "\u{2248} " + LedgerMoney.bare($0.monthly) } ?? TransactionRow.placeholder,
                caption: fixedCaption
            )
        }
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

    private var name: String { SummaryText.monthName(month.month) }

    private var previousName: String { SummaryText.monthName(month.adding(months: -1).month) }

    /// "▼ 180,14 vs September": the arrow says which way, the figure how much.
    /// January has no month to compare with, so no line.
    private var deltaCaption: String? {
        guard let delta = kpis.savingsDelta else { return nil }
        let arrow = delta >= 0 ? "\u{25B2}" : "\u{25BC}"
        return String(localized: "\(arrow) \(LedgerMoney.bare(abs(delta))) vs \(previousName)")
    }

    /// A rate over no income is a dash, never a fake 0%.
    private func rate(_ savings: Int64, of income: Int64) -> String {
        LedgerMoney.savingsRate(savings, of: income) ?? TransactionRow.placeholder
    }
}

/// One of the five numbers across the top.
private struct KPICard: View {
    let title: String
    let value: String
    var tint: Color = Ink.text
    /// `nil` leaves the line blank, so the cards keep their height.
    let caption: String?
    var captionTint: Color = Ink.text2

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
        .accessibilityLabel(AccessibilityText.card(title: title, value: value, caption: caption ?? ""))
    }
}

// MARK: - Funds

/// One card per capped envelope. Nothing is drawn for
/// a vault without caps.
struct FundCards: View {
    let funds: [FundGauge]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(String(localized: "Funds"))
                    .font(Face.label)
                    .foregroundStyle(Ink.text2)
                Text(String(localized: "envelopes with a cap"))
                    .font(Face.label)
                    .foregroundStyle(Ink.text3)
                Spacer(minLength: 0)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 240), spacing: 8)], spacing: 8) {
                ForEach(funds) { FundCard(fund: $0) }
            }
        }
    }
}

private struct FundCard: View {
    let fund: FundGauge

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    Text(fund.name)
                        .font(Face.ui(12, .semibold))
                        .foregroundStyle(Ink.text)
                        .lineLimit(1)
                    CapTag(kind: fund.kind)
                }
                HStack(alignment: .firstTextBaseline) {
                    Text(LedgerMoney.bare(fund.filled))
                        .font(Face.ui(15, .semibold))
                        .foregroundStyle(Ink.text)
                    Spacer(minLength: 8)
                    Text(String(localized: "of \(LedgerMoney.bare(fund.cap)) \u{00B7} \(percent)"))
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                }
                bar
            }
        }
        // The bar is a drawn shape with no text of its own: the fund's name
        // is the label, and the fill its value.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(fund.name)
        .accessibilityValue(
            AccessibilityText.fundGauge(
                percent: percent,
                filled: LedgerMoney.amount(fund.filled),
                cap: LedgerMoney.amount(fund.cap)
            )
        )
    }

    private var percent: String {
        LedgerMoney.percent(fund.filled, of: fund.cap, decimals: 0) ?? TransactionRow.placeholder
    }

    private var bar: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(Ink.line)
                Capsule()
                    .fill(Ink.chartIncome)
                    .frame(width: geometry.size.width * fund.fraction)
            }
        }
        .frame(height: 5)
    }
}

/// What a fund's cap is measured on, the small tag after its name: on a
/// fund card and on the fund's line of the envelope table, so the two read
/// the same.
struct CapTag: View {
    let kind: FundGauge.Kind

    var body: some View {
        Text(Self.text(kind))
            .font(Face.ui(10, .semibold))
            .foregroundStyle(Ink.text3)
            .lineLimit(1)
            .padding(.horizontal, 5)
            .frame(height: 16)
            .background(Ink.raised, in: RoundedRectangle(cornerRadius: 4))
    }

    static func text(_ kind: FundGauge.Kind) -> String {
        switch kind {
        case .balance: String(localized: "cap on balance")
        case .income: String(localized: "cap on income")
        }
    }
}
