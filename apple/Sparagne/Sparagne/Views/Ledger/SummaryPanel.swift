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
/// person. In the panel the TOTALE column is dropped for width; the summary
/// view shows it.
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
                        emphasis: true
                    )
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
        }
    }

    private func row(_ label: String, values: [Int64], tint: Color = Ink.text, emphasis: Bool = false) -> some View {
        HStack(spacing: 0) {
            Text(label)
                .font(emphasis ? Face.mono(12, .semibold) : Face.row)
                .foregroundStyle(emphasis ? Ink.text : Ink.text.opacity(0.85))
                .lineLimit(1)
            Spacer(minLength: 6)
            ForEach(Array(values.enumerated()), id: \.offset) { _, value in
                Text(LedgerMoney.bare(value))
                    .font(emphasis ? Face.mono(12, .semibold) : Face.row)
                    .foregroundStyle(value == 0 ? Ink.dim : tint)
                    .frame(width: columnWidth, alignment: .trailing)
            }
        }
        .frame(height: 21)
    }

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
                Text(LedgerMoney.amount(summary.savings))
                    .font(Face.headline)
                    .foregroundStyle(Ink.positive)
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
                SectionLabel(text: "\(String(localized: "Last 12 months")) \u{00B7} \(String(localized: "Savings"))")
                BarStrip(
                    values: summary.trailing.map(LedgerSummary.savings),
                    labels: summary.trailingMonths.map { LedgerDate.monthInitial($0.month) },
                    highlighted: summary.trailing.count - 1
                )
                .frame(height: 74)
            }
        }
    }
}

/// A row of bars with a one-letter label under each. Negative values hang
/// below the baseline, so a month in the red reads as one.
struct BarStrip: View {
    let values: [Int64]
    let labels: [String]
    var highlighted: Int?
    var tint: Color = Ink.positive

    private var scale: Double {
        let peak = values.map { abs(Double($0)) }.max() ?? 0
        return peak > 0 ? peak : 1
    }

    var body: some View {
        HStack(alignment: .bottom, spacing: 3) {
            ForEach(Array(values.enumerated()), id: \.offset) { index, value in
                VStack(spacing: 4) {
                    GeometryReader { geometry in
                        let height = geometry.size.height * (abs(Double(value)) / scale)
                        VStack {
                            Spacer(minLength: 0)
                            Rectangle()
                                .fill(index == highlighted ? tint : tint.opacity(0.45))
                                .frame(height: max(height, value == 0 ? 0 : 1))
                        }
                    }
                    Text(labels.indices.contains(index) ? labels[index] : "")
                        .font(Face.mono(8))
                        .foregroundStyle(index == highlighted ? Ink.text : Ink.dim)
                }
            }
        }
    }
}
