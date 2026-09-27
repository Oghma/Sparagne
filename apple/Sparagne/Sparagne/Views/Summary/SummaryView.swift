import Charts
import SwiftUI
import SparagneCore

/// The RIEPILOGO view (`docs/v2/UI.md` §2.2): the year up to the month on
/// screen, the way the household's spreadsheet reads it.
///
/// Everything here is drawing: the arithmetic is `YearSummary.build`, which is
/// tested on its own, so this file only decides colors and widths.
struct SummaryView: View {
    let year: YearSummary
    /// Kept for the chrome that will need the vault (names, filters); the
    /// numbers all come from `year`.
    let store: AppStore

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let summary = store.summary {
                MonthCards(summary: summary)
            }
            heading

            if !year.funds.isEmpty {
                FundGauges(funds: year.funds)
            }

            initialLine

            if isBlank {
                Text(String(localized: "No activity this year"))
                    .font(Face.row)
                    .foregroundStyle(Ink.dim)
                    .padding(.top, 4)
            } else {
                monthTable
                charts
            }
        }
        .padding(10)
    }

    /// A fresh vault: nothing carried in, nothing moved all year.
    private var isBlank: Bool {
        year.initial == 0 && year.months.allSatisfy {
            $0.income == 0 && $0.cashExpense == 0 && $0.fundExpense == 0 && $0.total == 0
        }
    }

    // MARK: - Heading

    /// `2026 · fino a settembre`. A past year has no "up to": it is full.
    private var heading: some View {
        HStack(spacing: 6) {
            SectionLabel(text: "\(year.year)", tint: Ink.accent)
            if year.year == year.upTo.year {
                SectionLabel(
                    text: "\u{00B7} \(String(localized: "up to")) \(LedgerDate.fullMonth(year.upTo.month))"
                )
            }
        }
    }

    // MARK: - Opening cash fund

    /// FONDO CASSA INIZIALE: everything that happened before January, per
    /// person. One person needs no total; it would repeat the only column.
    private var initialLine: some View {
        HStack(spacing: 18) {
            SectionLabel(text: String(localized: "Opening cash fund"))
            ForEach(Array(year.people.enumerated()), id: \.offset) { index, person in
                pair(person, year.initialByPerson[index], tint: Ink.text)
            }
            if year.people.count > 1 {
                pair(String(localized: "Total"), year.initial, tint: Ink.positive)
            }
            Spacer(minLength: 0)
        }
    }

    private func pair(_ label: String, _ value: Int64, tint: Color) -> some View {
        HStack(spacing: 6) {
            SectionLabel(text: label)
            Text(LedgerMoney.bare(value))
                .font(Face.row)
                .foregroundStyle(value == 0 ? Ink.dim : tint)
        }
    }

    // MARK: - The month table

    private var monthTable: some View {
        Panel {
            ScrollView(.horizontal) {
                VStack(spacing: 0) {
                    tableHeader
                    Hairline().padding(.vertical, 5)
                    ForEach(Array(year.months.enumerated()), id: \.offset) { index, month in
                        monthRow(month, previousTotal: index == 0 ? year.initial : year.months[index - 1].total)
                    }
                    Hairline().padding(.vertical, 5)
                    yearRow
                }
            }
            .scrollBounceBehavior(.basedOnSize)
        }
    }

    private var tableHeader: some View {
        HStack(spacing: 0) {
            SectionLabel(text: String(localized: "Month"))
                .frame(width: YearColumn.month, alignment: .leading)
            headerCell(String(localized: "Income"), width: YearColumn.income)
            headerCell(String(localized: "Expenses"), width: YearColumn.expenses)
            headerCell(String(localized: "Savings"), width: YearColumn.savings)
            headerCell(String(localized: "Cash fund"), width: YearColumn.carried)
            headerCell(String(localized: "Fund expenses"), width: YearColumn.fundExpenses)
            headerCell(String(localized: "Total"), width: YearColumn.total)
            ForEach(year.people, id: \.self) { person in
                headerCell(person, width: YearColumn.person)
            }
        }
        .frame(height: 20)
    }

    private func headerCell(_ text: String, width: CGFloat) -> some View {
        SectionLabel(text: text)
            .lineLimit(1)
            .frame(width: width, alignment: .trailing)
    }

    /// One month. A future month keeps its name and shows dashes, so the year
    /// reads as twelve rows whatever month is on screen.
    private func monthRow(_ month: YearMonth, previousTotal: Int64) -> some View {
        let onScreen = month.month == year.upTo

        return HStack(spacing: 0) {
            Text(LedgerDate.shortMonth(month.month.month))
                .font(Face.row)
                .foregroundStyle(month.isFuture ? Ink.dim : Ink.text)
                .frame(width: YearColumn.month, alignment: .leading)

            if month.isFuture {
                ForEach(Array(valueColumns.enumerated()), id: \.offset) { _, width in
                    Text(TransactionRow.placeholder)
                        .font(Face.row)
                        .foregroundStyle(Ink.dim)
                        .frame(width: width, alignment: .trailing)
                }
            } else {
                cell(month.income, width: YearColumn.income, tint: Ink.positive)
                cell(month.cashExpense, width: YearColumn.expenses)
                cell(month.savings, width: YearColumn.savings)
                cell(month.carried, width: YearColumn.carried)
                cell(month.fundExpense, width: YearColumn.fundExpenses)
                // TOTALE carries the verdict of the month: down on the month
                // before is the one thing worth seeing at a glance.
                cell(
                    month.total,
                    width: YearColumn.total,
                    tint: month.total >= previousTotal ? Ink.positive : Ink.negative,
                    weight: .semibold
                )
                ForEach(Array(month.totalByPerson.enumerated()), id: \.offset) { _, value in
                    cell(value, width: YearColumn.person)
                }
            }
        }
        .frame(height: 22)
        .background(onScreen ? Ink.raised : Color.clear)
        // One row, one stop: "September, Income €9,200.00, Expenses …" rather
        // than a dozen separate cells with no idea which month they belong to.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(monthRowLabel(month))
    }

    /// "September, Income €9,200.00, Expenses €3,100.00, …", or "October,
    /// future month" for a month past the one on screen (`docs/v2/UI.md`
    /// §2.2: a future month is drawn blank).
    private func monthRowLabel(_ month: YearMonth) -> String {
        let name = LedgerDate.fullMonth(month.month.month)
        guard !month.isFuture else {
            return String(localized: "\(name), future month")
        }
        var figures: [(String, String)] = [
            (String(localized: "Income"), LedgerMoney.amount(month.income)),
            (String(localized: "Expenses"), LedgerMoney.amount(month.cashExpense)),
            (String(localized: "Savings"), LedgerMoney.amount(month.savings)),
            (String(localized: "Cash fund"), LedgerMoney.amount(month.carried)),
            (String(localized: "Fund expenses"), LedgerMoney.amount(month.fundExpense)),
            (String(localized: "Total"), LedgerMoney.amount(month.total)),
        ]
        for (index, person) in year.people.enumerated() where month.totalByPerson.indices.contains(index) {
            figures.append((person, LedgerMoney.amount(month.totalByPerson[index])))
        }
        return AccessibilityText.monthRow(month: name, figures: figures)
    }

    /// The widths after MESE, in order: the six figures plus one per person.
    private var valueColumns: [CGFloat] {
        [
            YearColumn.income,
            YearColumn.expenses,
            YearColumn.savings,
            YearColumn.carried,
            YearColumn.fundExpenses,
            YearColumn.total,
        ] + Array(repeating: YearColumn.person, count: year.people.count)
    }

    private func cell(_ value: Int64, width: CGFloat, tint: Color = Ink.text, weight: Font.Weight = .regular) -> some View {
        Text(LedgerMoney.bare(value))
            .font(Face.mono(12, weight))
            .foregroundStyle(value == 0 ? Ink.dim : tint)
            .frame(width: width, alignment: .trailing)
    }

    /// The year's sums. Only the flows add up over a year: FONDO CASSA, TOTALE
    /// and the person columns are running balances, so they stay blank.
    private var yearRow: some View {
        let income = year.months.reduce(0) { $0 + $1.income }
        let expenses = year.months.reduce(0) { $0 + $1.cashExpense }
        let fundExpenses = year.months.reduce(0) { $0 + $1.fundExpense }

        return HStack(spacing: 0) {
            Text("\(year.year)")
                .font(Face.mono(12, .semibold))
                .foregroundStyle(Ink.dim)
                .frame(width: YearColumn.month, alignment: .leading)
            cell(income, width: YearColumn.income, tint: Ink.positive, weight: .semibold)
            cell(expenses, width: YearColumn.expenses, weight: .semibold)
            cell(income - expenses, width: YearColumn.savings, weight: .semibold)
            Color.clear.frame(width: YearColumn.carried, height: 1)
            cell(fundExpenses, width: YearColumn.fundExpenses, weight: .semibold)
            Color.clear.frame(width: YearColumn.total, height: 1)
            ForEach(year.people, id: \.self) { _ in
                Color.clear.frame(width: YearColumn.person, height: 1)
            }
        }
        .frame(height: 22)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            AccessibilityText.monthRow(
                month: "\(year.year)",
                figures: [
                    (String(localized: "Income"), LedgerMoney.amount(income)),
                    (String(localized: "Expenses"), LedgerMoney.amount(expenses)),
                    (String(localized: "Savings"), LedgerMoney.amount(income - expenses)),
                    (String(localized: "Fund expenses"), LedgerMoney.amount(fundExpenses)),
                ]
            )
        )
    }

    // MARK: - Charts

    private var charts: some View {
        HStack(alignment: .top, spacing: 10) {
            Panel {
                VStack(alignment: .leading, spacing: 8) {
                    SectionLabel(text: String(localized: "Cash flow"))
                    CashFlowChart(months: past)
                    ChartLegend(
                        entries: [
                            (String(localized: "Income"), Ink.positive),
                            (String(localized: "Expenses"), Ink.negative),
                            (String(localized: "Savings"), Ink.text),
                        ]
                    )
                }
            }
            Panel {
                VStack(alignment: .leading, spacing: 8) {
                    SectionLabel(text: String(localized: "Cash fund"))
                    CashFundChart(months: past)
                    ChartLegend(entries: [(String(localized: "Total"), Ink.positive)])
                }
            }
        }
    }

    /// A future month has no line to draw, only an empty slot on the axis.
    private var past: [YearMonth] { year.months.filter { !$0.isFuture } }
}

// MARK: - The month's cards

/// The four numbers of the month on screen, kept from the first summary at
/// the user's request (2026-09-12): income, expenses, savings and the rate,
/// with the month named so they do not read as the year's.
private struct MonthCards: View {
    let summary: LedgerSummary

    var body: some View {
        HStack(spacing: 10) {
            StatCard(
                title: "\(String(localized: "Income")) \u{00B7} \(month)",
                value: LedgerMoney.amount(summary.totals.income),
                tint: Ink.positive,
                caption: "\(summary.people.count) \(String(localized: "people"))"
            )
            StatCard(
                title: "\(String(localized: "Expenses")) \u{00B7} \(month)",
                value: LedgerMoney.amount(summary.totals.netExpense),
                tint: Ink.negative,
                caption: refundCaption
            )
            StatCard(
                title: "\(String(localized: "Savings")) \u{00B7} \(month)",
                value: LedgerMoney.amount(summary.savings),
                tint: Ink.text,
                caption: deltaCaption
            )
            StatCard(
                title: "\(String(localized: "Rate")) \u{00B7} \(month)",
                value: LedgerMoney.percent(summary.savings, of: summary.totals.income) ?? TransactionRow.placeholder,
                tint: Ink.text,
                caption: nil,
                meter: rate
            )
        }
    }

    private var month: String { LedgerDate.fullMonth(summary.month.month) }

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

    /// Clamped by the bar: a month that saved more than it earned (a refund
    /// of an earlier month) would otherwise overflow it.
    private var rate: Double? {
        guard summary.totals.income > 0 else { return nil }
        return Double(summary.savings) / Double(summary.totals.income)
    }
}

/// One of the four numbers across the top.
private struct StatCard: View {
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
        // One card, one stop: the title already names the month, so VoiceOver
        // reads it as "Income · September, €1,234.56, 2 people" instead of
        // three separate swipes.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(AccessibilityText.card(title: title, value: value, caption: caption))
    }
}

// MARK: - Geometry

/// Column widths, shared by the header, the month rows and the year's sums so
/// the three agree without a layout pass (as `GridColumn` does for the ledger).
private enum YearColumn {
    static let month: CGFloat = 56
    static let income: CGFloat = 100
    static let expenses: CGFloat = 100
    static let savings: CGFloat = 104
    static let carried: CGFloat = 112
    static let fundExpenses: CGFloat = 112
    static let total: CGFloat = 112
    static let person: CGFloat = 104
}

// MARK: - Gauges

/// One ring per capped envelope (`docs/v2/UI.md` §2.2). Four fit side by side
/// at the window's minimum width; beyond that the row scrolls.
private struct FundGauges: View {
    let funds: [FundGauge]

    private static let sideBySide = 4
    private static let fixedWidth: CGFloat = 220

    var body: some View {
        if funds.count <= Self.sideBySide {
            HStack(alignment: .top, spacing: 10) {
                ForEach(funds) { gauge(for: $0) }
            }
        } else {
            ScrollView(.horizontal) {
                HStack(alignment: .top, spacing: 10) {
                    ForEach(funds) { gauge(for: $0).frame(width: Self.fixedWidth) }
                }
            }
            .scrollBounceBehavior(.basedOnSize)
        }
    }

    private func gauge(for fund: FundGauge) -> some View {
        Panel {
            VStack(spacing: 8) {
                SectionLabel(text: fund.name)
                ring(fund)
                Text("\(LedgerMoney.bare(fund.filled)) / \(LedgerMoney.bare(fund.cap))")
                    .font(Face.footnote)
                    .foregroundStyle(Ink.dim)
            }
            .frame(maxWidth: .infinity)
        }
        // The ring itself is a drawn arc with no text of its own to read; the
        // gauge's name is the label, and the fill fraction its value, exactly
        // as `docs/v2/UI.md` §2.2 defines it.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(fund.name)
        .accessibilityValue(
            AccessibilityText.fundGauge(
                percent: LedgerMoney.percent(fund.filled, of: fund.cap) ?? TransactionRow.placeholder,
                filled: LedgerMoney.amount(fund.filled),
                cap: LedgerMoney.amount(fund.cap)
            )
        )
    }

    private func ring(_ fund: FundGauge) -> some View {
        ZStack {
            Circle()
                .stroke(Ink.line, lineWidth: 8)
            Circle()
                .trim(from: 0, to: fund.fraction)
                .stroke(Ink.positive, style: StrokeStyle(lineWidth: 8, lineCap: .round))
                // Trim starts at three o'clock; a gauge reads from the top.
                .rotationEffect(.degrees(-90))
            Text(LedgerMoney.percent(fund.filled, of: fund.cap) ?? TransactionRow.placeholder)
                .font(Face.headline)
                .foregroundStyle(Ink.text)
        }
        .frame(width: 96, height: 96)
        .padding(.vertical, 4)
    }
}

// MARK: - Charts

/// ENTRATE, USCITE and RISPARMIO month by month.
private struct CashFlowChart: View {
    let months: [YearMonth]

    var body: some View {
        Chart {
            series(months.map { ChartPoint($0.month.month, $0.income) }, key: "income", tint: Ink.positive)
            series(months.map { ChartPoint($0.month.month, $0.cashExpense) }, key: "expenses", tint: Ink.negative)
            series(months.map { ChartPoint($0.month.month, $0.savings) }, key: "savings", tint: Ink.text)
        }
        .chartXScale(domain: 1...12)
        .chartXAxis { YearAxis.months() }
        .chartYAxis { YearAxis.money() }
        .chartLegend(.hidden)
        .frame(height: 200)
        // No `accessibilityChartDescriptor`: with three series and up to a
        // year of points it would be a lot of machinery for what a single
        // spoken transcript already gives a screen-reader user.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(String(localized: "Cash flow"))
        .accessibilityValue(summary)
    }

    /// "Jan Income €9,200.00, Expenses €3,100.00, Savings €6,100.00; Feb …".
    private var summary: String {
        months.map { month in
            AccessibilityText.monthRow(
                month: LedgerDate.shortMonth(month.month.month),
                figures: [
                    (String(localized: "Income"), LedgerMoney.amount(month.income)),
                    (String(localized: "Expenses"), LedgerMoney.amount(month.cashExpense)),
                    (String(localized: "Savings"), LedgerMoney.amount(month.savings)),
                ]
            )
        }.joined(separator: "; ")
    }

    /// `key` keeps the three lines apart: without a series of its own, Charts
    /// joins every `LineMark` of a chart into one path.
    @ChartContentBuilder
    private func series(_ points: [ChartPoint], key: String, tint: Color) -> some ChartContent {
        ForEach(points) { point in
            LineMark(
                x: .value("", point.id),
                y: .value("", point.value),
                series: .value("", key)
            )
            .foregroundStyle(tint)
            PointMark(x: .value("", point.id), y: .value("", point.value))
                .symbolSize(18)
                .foregroundStyle(tint)
        }
    }
}

/// One month of one line. Tuples cannot carry a key path, and `ForEach` needs
/// one.
private struct ChartPoint: Identifiable {
    /// The month, 1...12.
    let id: Int
    let value: Double

    init(_ month: Int, _ minorUnits: Int64) {
        id = month
        value = YearAxis.major(minorUnits)
    }
}

/// The wallets' balance at the end of each month.
private struct CashFundChart: View {
    let months: [YearMonth]

    var body: some View {
        Chart {
            ForEach(months, id: \.month) { month in
                LineMark(
                    x: .value("", month.month.month),
                    y: .value("", YearAxis.major(month.total))
                )
                .foregroundStyle(Ink.positive)
                PointMark(
                    x: .value("", month.month.month),
                    y: .value("", YearAxis.major(month.total))
                )
                .symbolSize(18)
                .foregroundStyle(Ink.positive)
            }
        }
        .chartXScale(domain: 1...12)
        .chartXAxis { YearAxis.months() }
        .chartYAxis { YearAxis.money() }
        .chartLegend(.hidden)
        .frame(height: 200)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(String(localized: "Cash fund"))
        .accessibilityValue(summary)
    }

    /// "Jan €32,221.00, Feb €34,000.00, …": the TOTALE line, one point per
    /// spoken word instead of a shape with no numbers of its own.
    private var summary: String {
        months
            .map { "\(LedgerDate.shortMonth($0.month.month)) \(LedgerMoney.amount($0.total))" }
            .joined(separator: ", ")
    }
}

/// The axes both charts share, in the palette: Swift Charts' defaults are the
/// system's, which are far too bright on this ground.
private enum YearAxis {
    /// Minor units are the model's truth; a chart only has to be the right
    /// height, so this is the one place a `Double` is allowed.
    static func major(_ minorUnits: Int64) -> Double { Double(minorUnits) / 100 }

    /// `12k`, `1,5k`, `840`: built with integers, so no rounding surprises.
    static func compact(_ major: Int) -> String {
        let sign = major < 0 ? "-" : ""
        let value = abs(major)
        guard value >= 1_000 else { return "\(sign)\(value)" }
        let thousands = value / 1_000
        let tenth = (value % 1_000) / 100
        return tenth == 0 ? "\(sign)\(thousands)k" : "\(sign)\(thousands),\(tenth)k"
    }

    static func months() -> some AxisContent {
        AxisMarks(values: Array(1...12)) { value in
            AxisGridLine().foregroundStyle(Ink.line)
            AxisValueLabel {
                if let month = value.as(Int.self) {
                    Text(LedgerDate.shortMonth(month))
                        .font(Face.footnote)
                        .foregroundStyle(Ink.dim)
                }
            }
        }
    }

    static func money() -> some AxisContent {
        AxisMarks { value in
            AxisGridLine().foregroundStyle(Ink.line)
            AxisValueLabel {
                if let amount = value.as(Double.self) {
                    Text(compact(Int(amount)))
                        .font(Face.footnote)
                        .foregroundStyle(Ink.dim)
                }
            }
        }
    }
}

/// The charts hide Swift Charts' own legend, which cannot be styled; this is
/// the palette's version of it.
private struct ChartLegend: View {
    let entries: [(String, Color)]

    var body: some View {
        HStack(spacing: 14) {
            ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                HStack(spacing: 5) {
                    Rectangle().fill(entry.1).frame(width: 8, height: 2)
                    Text(entry.0.uppercased())
                        .font(Face.footnote)
                        .tracking(0.6)
                        .foregroundStyle(Ink.dim)
                }
            }
            Spacer(minLength: 0)
        }
    }
}
