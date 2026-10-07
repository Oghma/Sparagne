import SwiftUI
import SparagneCore

/// The column to the right of the grid (`.side`, `docs/v2/UI.md` §2.1): the
/// month's numbers, always next to the rows that produce them. Four cards,
/// most important first: what was saved, who moved what, where the spending
/// went, and how this month sits in the last twelve.
///
/// The panel is on the chrome's ground, a step below the sheet, so the grid
/// stays the brightest surface and the cards read as a margin note to it.
struct SummaryPanel: View {
    let summary: LedgerSummary
    let store: AppStore

    static let width: CGFloat = 300

    var body: some View {
        ScrollView {
            VStack(spacing: 8) {
                SavingsHero(summary: summary)
                PersonMatrix(summary: summary, store: store)
                CategoryBreakdown(summary: summary)
                TrailingMonths(summary: summary)
            }
            .padding(10)
        }
        .scrollBounceBehavior(.basedOnSize)
        .frame(width: Self.width)
        .background(Ink.bg)
        .overlay(alignment: .leading) {
            Rectangle().fill(Ink.line).frame(width: 1)
        }
    }
}

// MARK: - Shared pieces

/// A card's title line (`.ch`): the title on the left, an optional tag on
/// the right.
private struct CardHeader<Trailing: View>: View {
    let title: String
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(spacing: 8) {
            Text(title)
                .font(Face.ui(11.5, .semibold))
                .foregroundStyle(Ink.text2)
                .lineLimit(1)
                .accessibilityAddTraits(.isHeader)
            Spacer(minLength: 0)
            trailing
        }
        .padding(.bottom, 8)
    }
}

extension CardHeader where Trailing == EmptyView {
    init(title: String) {
        self.init(title: title) { EmptyView() }
    }
}

/// Figures as the panel writes them: no symbol (the hero carries it once),
/// and a true minus sign, which is as wide as a digit's neighbours and reads
/// as one.
enum PanelMoney {
    static func bare(_ minorUnits: Int64) -> String {
        LedgerMoney.bare(minorUnits).replacingOccurrences(of: "-", with: "\u{2212}")
    }

    /// The month as a sentence writes it: "ottobre" in Italian, "October" in
    /// English.
    static func monthName(_ month: MonthKey, locale: Locale = .autoupdatingCurrent) -> String {
        month.start().formatted(Date.FormatStyle(locale: locale).month(.wide))
    }
}

// MARK: - Savings

/// The month's savings (`income − net expense`), large: the one figure the
/// ledger exists to produce. Under it, the share of income it is and how it
/// moved since the month before, then income and expenses side by side
/// (`DISTILLATO_V1.md` §3.5).
struct SavingsHero: View {
    let summary: LedgerSummary

    private var isCurrent: Bool { summary.month == MonthKey(Date()) }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 0) {
                CardHeader(title: String(localized: "Savings in \(PanelMoney.monthName(summary.month))")) {
                    if isCurrent {
                        Text(String(localized: "in progress"))
                            .font(Face.ui(10, .semibold))
                            .foregroundStyle(Ink.text3)
                            .padding(.horizontal, 5)
                            .frame(height: 16)
                            .background(Ink.raised, in: RoundedRectangle(cornerRadius: 4))
                    }
                }

                HStack(alignment: .firstTextBaseline, spacing: 5) {
                    Text(PanelMoney.bare(summary.savings))
                        .font(Face.headline)
                        .tracking(-0.36)
                        .foregroundStyle(summary.savings < 0 ? Ink.negative : Ink.positive)
                    Text(verbatim: "€")
                        .font(Face.ui(14, .medium))
                        .foregroundStyle(Ink.text3)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(
                    AccessibilityText.figure(
                        String(localized: "Savings in \(PanelMoney.monthName(summary.month))"),
                        amount: LedgerMoney.amount(summary.savings)
                    )
                )

                if share != nil || delta != nil {
                    HStack(spacing: 10) {
                        if let share {
                            Text(String(localized: "\(share) of income"))
                                .foregroundStyle(Ink.text2)
                        }
                        if let delta {
                            Text(delta.text).foregroundStyle(delta.tint)
                        }
                    }
                    .font(Face.ui(11.5))
                    .lineLimit(1)
                    .padding(.top, 4)
                }

                HStack(alignment: .top, spacing: 6) {
                    figure(String(localized: "Income"), summary.totals.income)
                    figure(String(localized: "Expenses"), summary.totals.netExpense)
                }
                .padding(.top, 9)
                .overlay(alignment: .top) { Hairline() }
                .padding(.top, 10)
            }
        }
    }

    /// `"66%"`; nothing when there was no income to divide by.
    private var share: String? {
        LedgerMoney.percent(summary.savings, of: summary.totals.income, decimals: 0)
    }

    /// `"▼ 180,14 su settembre"`: the change in savings since the month
    /// before, as an amount, green when it grew and red when it shrank.
    /// Nothing when the month before had no activity to compare with.
    private var delta: (text: String, tint: Color)? {
        let previous = summary.previous
        guard previous.income != 0 || previous.expense != 0 || previous.refund != 0 else { return nil }
        let change = summary.savings - summary.previousSavings
        let arrow = change > 0 ? "\u{25B2}" : change < 0 ? "\u{25BC}" : "="
        let tint = change > 0 ? Ink.positive : change < 0 ? Ink.negative : Ink.text2
        let month = PanelMoney.monthName(summary.month.adding(months: -1))
        let amount = "\(arrow) \(LedgerMoney.bare(abs(change)))"
        return (String(localized: "\(amount) vs \(month)"), tint)
    }

    private func figure(_ label: String, _ amount: Int64) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(label)
                .font(Face.ui(11))
                .foregroundStyle(Ink.text3)
            Text(PanelMoney.bare(amount))
                .font(Face.ui(13, .medium))
                .foregroundStyle(Ink.text)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(AccessibilityText.figure(label, amount: LedgerMoney.amount(amount)))
    }
}

// MARK: - Person x envelope

/// `Entrate · Uscite · <busta> … · Risparmio`, one column per person
/// (`.pm`). Envelopes that did not move this month are left out; the total
/// is the hero card's.
struct PersonMatrix: View {
    let summary: LedgerSummary
    let store: AppStore

    private var people: [String] { summary.people }

    /// Two people fit the canvas's 72-point columns; more share the room.
    private var columnWidth: CGFloat {
        switch people.count {
        case ...2: 72
        case 3: 58
        default: 48
        }
    }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 0) {
                CardHeader(title: String(localized: "By person"))
                if people.isEmpty {
                    Text(String(localized: "No activity this month"))
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                } else {
                    Grid(alignment: .trailing, horizontalSpacing: 0, verticalSpacing: 5) {
                        GridRow {
                            Color.clear.gridCellUnsizedAxes([.horizontal, .vertical])
                            ForEach(people, id: \.self) { person in
                                Text(person)
                                    .font(Face.ui(11))
                                    .foregroundStyle(Ink.text3)
                                    .lineLimit(1)
                                    .frame(width: columnWidth, alignment: .trailing)
                            }
                        }
                        row(String(localized: "Income"), values: people.map { summary.totals(for: $0).income })
                        ForEach(envelopes, id: \.id) { flow in
                            row(
                                String(localized: "Expenses \u{00B7} \(store.flowName(flow))"),
                                values: people.map { summary.netExpense(flow: flow.id, person: $0) }
                            )
                        }
                        Hairline()
                            .gridCellColumns(people.count + 1)
                            .gridCellUnsizedAxes(.horizontal)
                        row(
                            String(localized: "Savings"),
                            values: people.map { summary.totals(for: $0).savings },
                            emphasis: true
                        )
                    }
                }
            }
        }
    }

    /// Envelopes that moved this month, in the vault's order. Unallocated only
    /// shows when it actually carried something.
    ///
    /// Archived ones too, from the snapshot rather than `store.flows`: an
    /// envelope archived after this month's expenses still carried them, and
    /// the Savings line counts them, so without its row the lines above
    /// would not add up to it.
    private var envelopes: [FlowView] {
        let touched = Set(summary.flowPerson.filter { $0.netExpense != 0 }.map(\.flowId))
        return (store.snapshot?.flows ?? []).filter { touched.contains($0.id) }
    }

    /// Plain figures, a dash for nothing; the savings line in bold, green or
    /// red by its sign, since that is the line the card is read for.
    private func row(_ label: String, values: [Int64], emphasis: Bool = false) -> some View {
        GridRow {
            Text(label)
                .font(emphasis ? Face.ui(12, .semibold) : Face.row)
                .foregroundStyle(emphasis ? Ink.text : Ink.text2)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
                .gridColumnAlignment(.leading)
            ForEach(Array(values.enumerated()), id: \.offset) { index, value in
                Text(value == 0 ? "\u{2014}" : PanelMoney.bare(value))
                    .font(emphasis ? Face.ui(12, .semibold) : Face.row)
                    .foregroundStyle(tint(value, emphasis: emphasis))
                    .lineLimit(1)
                    .frame(width: columnWidth, alignment: .trailing)
                    // The figure has a row label and a column heading; the
                    // person's name has to travel with the amount for
                    // VoiceOver to make sense of it (`docs/v2/UI.md` §2.1).
                    .accessibilityLabel(
                        AccessibilityText.figure(
                            label,
                            person: people.indices.contains(index) ? people[index] : nil,
                            amount: LedgerMoney.amount(value)
                        )
                    )
            }
        }
    }

    private func tint(_ value: Int64, emphasis: Bool) -> Color {
        if value == 0 { return Ink.text3 }
        guard emphasis else { return Ink.text }
        return value < 0 ? Ink.negative : Ink.positive
    }
}

// MARK: - Categories

/// Where the month's spending went (`.cat`): the six heaviest categories,
/// each with its share of the month's expenses and a bar against the
/// heaviest one.
struct CategoryBreakdown: View {
    let summary: LedgerSummary
    var limit = 6

    private var shown: [CategoryTotals] {
        Array(summary.categories.filter { $0.netExpense > 0 }.prefix(limit))
    }

    /// What the categories spent between them, the base of each share. Not
    /// the month's net expense: a refund filed in a category with nothing
    /// spent (a reimbursement of last month's bill) lowers that total but no
    /// category's figure, and the shares would add up to more than 100%.
    private var spent: Int64 {
        summary.categories.reduce(0) { $0 + max($1.netExpense, 0) }
    }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 0) {
                CardHeader(title: String(localized: "Expenses by category"))
                if shown.isEmpty {
                    Text(String(localized: "Nothing spent this month"))
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                } else {
                    VStack(spacing: 7) {
                        ForEach(shown, id: \.categoryId) { category in
                            line(category)
                        }
                    }
                }
            }
        }
    }

    private func line(_ category: CategoryTotals) -> some View {
        let name = Self.name(category)
        let share = LedgerMoney.percent(category.netExpense, of: spent, decimals: 0)
        return VStack(spacing: 3) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(name)
                    .foregroundStyle(Ink.text)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
                Text(PanelMoney.bare(category.netExpense))
                    .foregroundStyle(Ink.text)
                Text(share ?? "")
                    .font(Face.ui(11))
                    .foregroundStyle(Ink.text3)
                    .frame(width: 32, alignment: .trailing)
            }
            .font(Face.row)
            CategoryBar(fraction: fraction(category))
        }
        // The bar is decoration for the same figure, not a second one: one
        // stop, "Casa: €480,00".
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(AccessibilityText.figure(name, amount: LedgerMoney.amount(category.netExpense)))
        .accessibilityValue(share ?? "")
    }

    private func fraction(_ category: CategoryTotals) -> Double {
        let heaviest = shown.first?.netExpense ?? 0
        guard heaviest > 0 else { return 0 }
        return Double(category.netExpense) / Double(heaviest)
    }

    /// The core names its two system categories in English; the UI says them
    /// in the user's language, as the grid does (`TransactionRow`).
    static func name(_ category: CategoryTotals) -> String {
        guard category.isSystem else { return category.name }
        switch category.name.lowercased() {
        case "uncategorized": return String(localized: "Uncategorized")
        case "opening": return String(localized: "Opening")
        default: return category.name
        }
    }
}

/// A 4-point bar with rounded ends on a faint track (`.bar`).
private struct CategoryBar: View {
    let fraction: Double

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(Color(hex: 0x222228))
                Capsule()
                    .fill(Ink.chartExpense)
                    .frame(width: geometry.size.width * min(max(fraction, 0), 1))
            }
        }
        .frame(height: 4)
        .accessibilityHidden(true)
    }
}

// MARK: - Twelve months

/// The savings of the last twelve months (`.months`): the month on screen
/// at full strength, the others at half, months in the red hanging below the
/// baseline.
struct TrailingMonths: View {
    let summary: LedgerSummary

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 0) {
                CardHeader(title: heading)
                SavingsBars(
                    values: summary.trailing.map(LedgerSummary.savings),
                    labels: summary.trailingMonths.map { LedgerDate.monthInitial($0.month) },
                    highlighted: summary.trailing.count - 1
                )
                // Twelve bars with a one-letter label each: a screen reader
                // gets the whole strip as one figure instead.
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(heading)
                .accessibilityValue(monthlySavings)
            }
        }
    }

    private var heading: String { String(localized: "Savings \u{00B7} last 12 months") }

    /// "ott €600,00, nov €200,00, …": the month's short name, unlike the
    /// strip's single letters.
    private var monthlySavings: String {
        zip(summary.trailingMonths, summary.trailing.map(LedgerSummary.savings))
            .map { month, value in "\(LedgerDate.shortMonth(month.month)) \(LedgerMoney.amount(value))" }
            .joined(separator: ", ")
    }
}

/// Twelve columns, 56 points of bar above a one-letter label. The baseline
/// sits where zero falls between the highest month and the lowest: at the
/// bottom when nothing is negative, higher when a month went into the red.
struct SavingsBars: View {
    let values: [Int64]
    let labels: [String]
    var highlighted: Int?

    private static let zone: CGFloat = 56

    /// How far the tallest month reaches above zero, and the deepest below.
    private var peak: Double { max(0, values.map(Double.init).max() ?? 0) }
    private var trough: Double { min(0, values.map(Double.init).min() ?? 0) }

    /// The baseline's distance from the top of the zone.
    private var baseline: CGFloat {
        let range = peak - trough
        return range > 0 ? Self.zone * peak / range : Self.zone
    }

    var body: some View {
        HStack(alignment: .top, spacing: 4) {
            ForEach(Array(values.enumerated()), id: \.offset) { index, value in
                VStack(spacing: 4) {
                    bar(value, current: index == highlighted)
                        .frame(height: Self.zone, alignment: .top)
                    Text(labels.indices.contains(index) ? labels[index] : "")
                        .font(Face.ui(10, index == highlighted ? .semibold : .regular))
                        .foregroundStyle(index == highlighted ? Ink.text : Ink.text3)
                }
                .frame(maxWidth: .infinity)
            }
        }
        .overlay(alignment: .top) {
            Rectangle()
                .fill(Ink.line2)
                .frame(height: 1)
                .offset(y: baseline)
        }
        .frame(height: 74, alignment: .top)
    }

    /// A positive month grows up from the baseline, a negative one hangs
    /// down from it; never thinner than a point, so an empty month still
    /// shows where it is.
    private func bar(_ value: Int64, current: Bool) -> some View {
        let amount = Double(value)
        let height: CGFloat
        let top: CGFloat
        let fill: Color
        if value >= 0 {
            height = max(1, peak > 0 ? baseline * amount / peak : 0)
            top = baseline - height
            fill = current ? Ink.chartSavings : Ink.chartSavings.opacity(0.5)
        } else {
            height = max(1, trough < 0 ? (Self.zone - baseline) * amount / trough : 0)
            top = baseline
            fill = Ink.negative.opacity(0.7)
        }
        return RoundedRectangle(cornerRadius: 2)
            .fill(fill)
            .frame(height: height)
            .offset(y: top)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}
