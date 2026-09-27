import SwiftUI
import SparagneCore

/// The column to the right of the grid: the month's numbers, always next to
/// the rows that produce them (`docs/v2/UI.md` §2.1).
struct SummaryPanel: View {
    let summary: LedgerSummary
    let store: AppStore

    var body: some View {
        ScrollView {
            VStack(spacing: 10) {
                PersonMatrix(summary: summary, store: store, compact: true)
                SavingsCard(summary: summary)
                CategoryBreakdown(categories: summary.categories, limit: 6)
                TrailingMonths(summary: summary)
            }
            .padding(10)
        }
        .frame(width: Metrics.sidebarWidth)
        .background(Ink.bg)
    }
}

// MARK: - Person x envelope

/// `Entrate / Uscite cash / Uscite varie / … / Risparmio`, one column per
/// person plus a total. The panel beside the grid drops the total column for
/// width; the summary view shows it.
struct PersonMatrix: View {
    let summary: LedgerSummary
    let store: AppStore
    var compact = false

    private var people: [String] { summary.people }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(
                    text: "\(String(localized: "Summary")) \(LedgerDate.fullMonth(summary.month.month))",
                    tint: Ink.accent
                )
                .accessibilityAddTraits(.isHeader)

                if people.isEmpty {
                    Text(String(localized: "No activity this month"))
                        .font(Face.row)
                        .foregroundStyle(Ink.dim)
                } else {
                    VStack(spacing: 0) {
                        header
                        Hairline().padding(.vertical, 5)
                        row(String(localized: "Income"), values: people.map { summary.totals(for: $0).income }, tint: Ink.positive)
                        ForEach(envelopes, id: \.id) { flow in
                            row(
                                "\(String(localized: "Expenses")) \(store.flowName(flow).lowercased())",
                                values: people.map { summary.netExpense(flow: flow.id, person: $0) }
                            )
                        }
                        Hairline().padding(.vertical, 5)
                        row(
                            String(localized: "Savings"),
                            values: people.map { summary.totals(for: $0).savings },
                            tint: Ink.positive,
                            emphasis: true
                        )
                    }
                }
            }
        }
    }

    /// Envelopes that moved this month, in the vault's order. Unallocated only
    /// shows when it actually carried something.
    private var envelopes: [FlowView] {
        let touched = Set(summary.flowPerson.filter { $0.netExpense != 0 }.map(\.flowId))
        return store.flows.filter { touched.contains($0.id) }
    }

    private var header: some View {
        HStack(spacing: 0) {
            Spacer(minLength: 0)
            ForEach(people, id: \.self) { person in
                SectionLabel(text: person)
                    .frame(width: columnWidth, alignment: .trailing)
            }
            if showsTotal {
                SectionLabel(text: String(localized: "Total"), tint: Ink.text)
                    .frame(width: columnWidth, alignment: .trailing)
            }
        }
    }

    /// The total column carries the tint; the per-person ones stay neutral, so
    /// the eye lands on the sum rather than on the columns it is made of.
    private func row(_ label: String, values: [Int64], tint: Color = Ink.text, emphasis: Bool = false) -> some View {
        let font = emphasis ? Face.mono(12, .semibold) : Face.row
        return HStack(spacing: 0) {
            Text(label)
                .font(font)
                .foregroundStyle(emphasis ? Ink.text : Ink.text.opacity(0.85))
                .lineLimit(1)
            Spacer(minLength: 6)
            ForEach(Array(values.enumerated()), id: \.offset) { index, value in
                Text(LedgerMoney.bare(value))
                    .font(font)
                    .foregroundStyle(value == 0 ? Ink.dim : (compact ? tint : Ink.text))
                    .frame(width: columnWidth, alignment: .trailing)
                    // The grid gives every other figure a row and a column;
                    // this one only has a row label, so the person's name has
                    // to travel with the amount for VoiceOver to make sense
                    // of it (`docs/v2/UI.md` §2.1).
                    .accessibilityLabel(
                        AccessibilityText.figure(
                            label,
                            person: people.indices.contains(index) ? people[index] : nil,
                            amount: LedgerMoney.amount(value)
                        )
                    )
            }
            if showsTotal {
                let total = values.reduce(0, +)
                Text(LedgerMoney.bare(total))
                    .font(font)
                    .foregroundStyle(total == 0 ? Ink.dim : tint)
                    .frame(width: columnWidth, alignment: .trailing)
                    .accessibilityLabel(
                        AccessibilityText.figure(label, person: String(localized: "Total"), amount: LedgerMoney.amount(total))
                    )
            }
        }
        .frame(height: 21)
    }

    /// One person needs no total: it would repeat the only column there is.
    private var showsTotal: Bool { !compact && people.count > 1 }

    private var columnWidth: CGFloat { compact ? 78 : 100 }
}

// MARK: - Savings

/// The month's savings, the share of income it represents and the change on
/// the month before (`DISTILLATO_V1.md` §3.5 for the MoM formula).
struct SavingsCard: View {
    let summary: LedgerSummary

    var body: some View {
        HStack(spacing: 0) {
            Rectangle().fill(Ink.positive).frame(width: 3)
            VStack(alignment: .leading, spacing: 4) {
                SectionLabel(text: String(localized: "Total savings"))
                    .accessibilityAddTraits(.isHeader)
                Text(LedgerMoney.amount(summary.savings))
                    .font(Face.headline)
                    .foregroundStyle(Ink.positive)
                    .accessibilityLabel(
                        AccessibilityText.figure(String(localized: "Total savings"), amount: LedgerMoney.amount(summary.savings))
                    )
                Text(subtitle)
                    .font(Face.footnote)
                    .foregroundStyle(Ink.dim)
            }
            .padding(12)
            Spacer(minLength: 0)
        }
        .background(Ink.panel)
        .overlay(Rectangle().strokeBorder(Ink.line, lineWidth: 1))
    }

    private var subtitle: String {
        var parts: [String] = []
        if let share = LedgerMoney.percent(summary.savings, of: summary.totals.income) {
            parts.append("\(share) \(String(localized: "of income"))")
        }
        if let delta = LedgerMoney.delta(current: summary.savings, previous: summary.previousSavings) {
            let previous = summary.month.adding(months: -1)
            parts.append("\(delta) \(String(localized: "vs")) \(LedgerDate.fullMonth(previous.month).lowercased())")
        }
        return parts.joined(separator: " \u{00B7} ")
    }
}

// MARK: - Categories

/// Where the month's spending went, as bars relative to the heaviest category.
struct CategoryBreakdown: View {
    let categories: [CategoryTotals]
    var limit: Int?

    private var shown: [CategoryTotals] {
        let spending = categories.filter { $0.netExpense > 0 }
        guard let limit else { return spending }
        return Array(spending.prefix(limit))
    }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 9) {
                SectionLabel(text: String(localized: "Expenses by category"))
                    .accessibilityAddTraits(.isHeader)
                if shown.isEmpty {
                    Text(String(localized: "Nothing spent this month"))
                        .font(Face.row)
                        .foregroundStyle(Ink.dim)
                } else {
                    ForEach(shown, id: \.categoryId) { category in
                        VStack(spacing: 4) {
                            HStack {
                                Text(category.name)
                                    .font(Face.row)
                                    .foregroundStyle(Ink.text)
                                    .lineLimit(1)
                                Spacer(minLength: 8)
                                Text(LedgerMoney.bare(category.netExpense))
                                    .font(Face.row)
                                    .foregroundStyle(Ink.text)
                            }
                            MeterBar(fraction: fraction(category))
                        }
                        // The bar under the name is decoration for the same
                        // figure, not a second one: one stop, "Casa: €480.00".
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(
                            AccessibilityText.figure(category.name, amount: LedgerMoney.amount(category.netExpense))
                        )
                    }
                }
            }
        }
    }

    private func fraction(_ category: CategoryTotals) -> Double {
        let heaviest = shown.first?.netExpense ?? 0
        guard heaviest > 0 else { return 0 }
        return Double(category.netExpense) / Double(heaviest)
    }
}

// MARK: - Twelve months

/// The savings of the last twelve months, the current one picked out.
struct TrailingMonths: View {
    let summary: LedgerSummary

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 10) {
                SectionLabel(text: heading)
                    .accessibilityAddTraits(.isHeader)
                BarStrip(
                    values: summary.trailing.map(LedgerSummary.savings),
                    labels: summary.trailingMonths.map { LedgerDate.monthInitial($0.month) },
                    highlighted: summary.trailing.count - 1
                )
                .frame(height: 74)
                // Twelve bars with a one-letter label each: a screen reader
                // gets the whole strip as one figure instead.
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(heading)
                .accessibilityValue(monthlySavings)
            }
        }
    }

    private var heading: String {
        "\(String(localized: "Last 12 months")) \u{00B7} \(String(localized: "Savings"))"
    }

    /// "Oct €600.00, Nov €200.00, …": the full month name, unlike the bar
    /// strip's own single-letter labels.
    private var monthlySavings: String {
        zip(summary.trailingMonths, summary.trailing.map(LedgerSummary.savings))
            .map { month, value in "\(LedgerDate.shortMonth(month.month)) \(LedgerMoney.amount(value))" }
            .joined(separator: ", ")
    }
}

/// A row of bars with a one-letter label under each. Negative values hang
/// below a baseline placed proportionally inside the strip, so a month in
/// the red reads as one; when every value is zero or positive the baseline
/// sits at the bottom and the strip looks exactly as it always has.
struct BarStrip: View {
    let values: [Int64]
    let labels: [String]
    var highlighted: Int?
    var tint: Color = Ink.positive

    /// At least 0: how far the tallest positive bar reaches.
    private var peak: Double { max(0, values.map(Double.init).max() ?? 0) }
    /// At most 0: how far the deepest negative bar reaches.
    private var trough: Double { min(0, values.map(Double.init).min() ?? 0) }
    private var hasNegative: Bool { trough < 0 }
    /// Share of the strip's height above the baseline; 1 when nothing is
    /// negative, so the baseline stays pinned to the bottom.
    private var positiveFraction: Double {
        let range = peak - trough
        return range > 0 ? peak / range : 1
    }

    var body: some View {
        HStack(alignment: .bottom, spacing: 3) {
            ForEach(Array(values.enumerated()), id: \.offset) { index, value in
                VStack(spacing: 4) {
                    GeometryReader { geometry in
                        let height = geometry.size.height
                        let positiveZone = height * positiveFraction
                        let negativeZone = height - positiveZone
                        let doubleValue = Double(value)
                        let opacity = index == highlighted ? 1.0 : 0.45

                        ZStack(alignment: .top) {
                            if doubleValue > 0, peak > 0 {
                                let barHeight = max(positiveZone * doubleValue / peak, 1)
                                Rectangle()
                                    .fill(tint.opacity(opacity))
                                    .frame(maxWidth: .infinity)
                                    .frame(height: barHeight)
                                    .offset(y: positiveZone - barHeight)
                            }
                            if doubleValue < 0, trough < 0 {
                                let barHeight = max(negativeZone * doubleValue / trough, 1)
                                Rectangle()
                                    .fill(Ink.negative.opacity(opacity))
                                    .frame(maxWidth: .infinity)
                                    .frame(height: barHeight)
                                    .offset(y: positiveZone)
                            }
                            if hasNegative {
                                Rectangle()
                                    .fill(Ink.line)
                                    .frame(maxWidth: .infinity)
                                    .frame(height: 1)
                                    .offset(y: positiveZone)
                            }
                        }
                        .frame(width: geometry.size.width, height: height, alignment: .topLeading)
                    }
                    Text(labels.indices.contains(index) ? labels[index] : "")
                        .font(Face.mono(8))
                        .foregroundStyle(index == highlighted ? Ink.text : Ink.dim)
                }
            }
        }
    }
}
