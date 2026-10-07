import Charts
import SwiftUI

/// The two charts under the table (`docs/v2/UI.md` §2.2), side by side while
/// they fit and stacked when they do not.
struct SummaryCharts: View {
    let year: YearSummary

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 420), spacing: 8, alignment: .top)], spacing: 8) {
            CashFlowCard(year: year)
            CashFundCard(year: year)
        }
    }
}

// MARK: - Cash flow

/// ENTRATE, USCITE and RISPARMIO month by month. The line into the month on
/// screen is dashed: that month is not over, so the segment is a promise and
/// not a fact.
private struct CashFlowCard: View {
    let year: YearSummary

    @State private var selected: Double?

    private struct Series: Identifiable {
        let id: String
        let name: String
        let tint: Color
        let value: (YearMonth) -> Int64
    }

    private var series: [Series] {
        [
            Series(id: "income", name: String(localized: "Income"), tint: Ink.chartIncome, value: { $0.income }),
            Series(id: "expenses", name: String(localized: "Expenses"), tint: Ink.chartExpense, value: { $0.cashExpense }),
            Series(id: "savings", name: String(localized: "Savings"), tint: Ink.chartSavings, value: { $0.savings }),
        ]
    }

    private var months: [YearMonth] { year.elapsed }

    /// Whether the last month drawn is the one on screen, still going.
    private var endsUnfinished: Bool { months.last?.month == year.upTo }

    /// Finished months, then the segment into the unfinished one.
    private var solid: [YearMonth] { endsUnfinished ? Array(months.dropLast()) : months }
    private var dashed: [YearMonth] { endsUnfinished && months.count >= 2 ? Array(months.suffix(2)) : [] }

    private var scale: (lower: Double, upper: Double, step: Double) {
        ChartScale.nice(
            months.flatMap { month in series.map { ChartScale.major($0.value(month)) } },
            includeZero: true
        )
    }

    private var hovered: YearMonth? {
        guard let selected, !months.isEmpty else { return nil }
        let index = min(max(Int(selected.rounded()), 1), 12)
        return months.first { $0.month.month == index }
    }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .firstTextBaseline) {
                    Text(String(localized: "Cash flow"))
                        .font(Face.label)
                        .foregroundStyle(Ink.text2)
                    Spacer(minLength: 8)
                    ChartLegend(entries: series.map { ($0.name, $0.tint) })
                }
                chart
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(String(localized: "Cash flow"))
        .accessibilityValue(spoken)
    }

    private var chart: some View {
        let scale = scale
        return Chart {
            if scale.lower < 0 {
                RuleMark(y: .value("", 0)).foregroundStyle(Ink.line2).lineStyle(StrokeStyle(lineWidth: 1))
            }
            ForEach(series) { line in
                ForEach(solid, id: \.month) { month in
                    LineMark(
                        x: .value("", Double(month.month.month)),
                        y: .value("", ChartScale.major(line.value(month))),
                        series: .value("", line.id)
                    )
                    .foregroundStyle(line.tint)
                    .lineStyle(StrokeStyle(lineWidth: 2, lineJoin: .round))
                }
                ForEach(dashed, id: \.month) { month in
                    LineMark(
                        x: .value("", Double(month.month.month)),
                        y: .value("", ChartScale.major(line.value(month))),
                        series: .value("", "\(line.id)-dash")
                    )
                    .foregroundStyle(line.tint)
                    .lineStyle(StrokeStyle(lineWidth: 2, lineJoin: .round, dash: [4, 4]))
                }
                if months.count == 1, let only = months.first {
                    PointMark(
                        x: .value("", Double(only.month.month)),
                        y: .value("", ChartScale.major(line.value(only)))
                    )
                    .symbolSize(24)
                    .foregroundStyle(line.tint)
                }
            }
            if let hovered {
                RuleMark(x: .value("", Double(hovered.month.month)))
                    .foregroundStyle(Ink.line2)
                    .lineStyle(StrokeStyle(lineWidth: 1))
                    .annotation(
                        position: .top, spacing: 4,
                        overflowResolution: .init(x: .fit(to: .chart), y: .fit(to: .chart))
                    ) {
                        tooltip(hovered)
                    }
                ForEach(series) { line in
                    PointMark(
                        x: .value("", Double(hovered.month.month)),
                        y: .value("", ChartScale.major(line.value(hovered)))
                    )
                    .symbolSize(40)
                    .foregroundStyle(line.tint)
                }
            }
        }
        .chartXSelection(value: $selected)
        .chartXScale(domain: 1...Double(max(months.last?.month.month ?? 2, 2)), range: .plotDimension(padding: 8))
        .chartYScale(domain: scale.lower...scale.upper)
        .chartXAxis { ChartAxes.months(upTo: months.last?.month.month ?? 1) }
        .chartYAxis { ChartAxes.money(scale) }
        .chartLegend(.hidden)
        .chartPlotStyle { $0.frame(height: 180) }
    }

    /// The hovered month's three figures, on the palette's raised card.
    private func tooltip(_ month: YearMonth) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(SummaryText.monthTitle(month.month.month, year: month.month.year))
                .font(Face.ui(11, .semibold))
                .foregroundStyle(Ink.text2)
            ForEach(series) { line in
                HStack(spacing: 14) {
                    HStack(spacing: 5) {
                        Capsule().fill(line.tint).frame(width: 8, height: 2)
                        Text(line.name).foregroundStyle(Ink.text2)
                    }
                    Spacer(minLength: 0)
                    let value = line.value(month)
                    Text(LedgerMoney.bare(value))
                        .foregroundStyle(line.id == "savings" && value < 0 ? Ink.negative : Ink.text)
                }
            }
        }
        .font(Face.ui(11.5))
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .frame(minWidth: 150)
        .background(Color(hex: 0x1E1E23), in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Ink.line2, lineWidth: 1))
    }

    /// "jan Income €9,200.00, Expenses €3,100.00, Savings €6,100.00; feb …".
    private var spoken: String {
        months.map { month in
            AccessibilityText.monthRow(
                month: LedgerDate.shortMonth(month.month.month),
                figures: series.map { ($0.name, LedgerMoney.amount($0.value(month))) }
            )
        }.joined(separator: "; ")
    }
}

// MARK: - Cash fund

/// The wallets' balance at the end of each month, from the opening fund. A
/// month where it went down is marked, so a bad month is visible without
/// reading the table.
private struct CashFundCard: View {
    let year: YearSummary

    private struct Point: Identifiable {
        /// 0 is the start of the year, 1...12 the months.
        let id: Int
        let total: Int64
    }

    private var points: [Point] {
        [Point(id: 0, total: year.initial)] + year.elapsed.map { Point(id: $0.month.month, total: $0.total) }
    }

    /// The months where TOTALE went down on the one before.
    private var drops: [Point] {
        zip(points, points.dropFirst()).filter { $0.1.total < $0.0.total }.map(\.1)
    }

    private var scale: (lower: Double, upper: Double, step: Double) {
        ChartScale.nice(points.map { ChartScale.major($0.total) }, includeZero: false)
    }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .firstTextBaseline) {
                    Text(String(localized: "Cash fund \u{00B7} total at month end"))
                        .font(Face.label)
                        .foregroundStyle(Ink.text2)
                    Spacer(minLength: 8)
                    Text(String(localized: "\(growth) since the start of the year"))
                        .font(Face.label)
                        .foregroundStyle(Ink.text3)
                }
                chart
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(String(localized: "Cash fund"))
        .accessibilityValue(spoken)
    }

    private var growth: String {
        SummaryText.signed(LedgerMoney.bare(year.growth), positive: year.growth >= 0)
    }

    private var chart: some View {
        let scale = scale
        let last = points.last
        return Chart {
            ForEach(points) { point in
                AreaMark(
                    x: .value("", Double(point.id)),
                    yStart: .value("", scale.lower),
                    yEnd: .value("", ChartScale.major(point.total))
                )
                .foregroundStyle(Ink.chartIncome.opacity(0.12))
                LineMark(x: .value("", Double(point.id)), y: .value("", ChartScale.major(point.total)))
                    .foregroundStyle(Ink.chartIncome)
                    .lineStyle(StrokeStyle(lineWidth: 2, lineJoin: .round))
            }
            ForEach(drops) { point in
                PointMark(x: .value("", Double(point.id)), y: .value("", ChartScale.major(point.total)))
                    .symbolSize(44)
                    .foregroundStyle(Ink.negative)
            }
            if let last, last.id > 0 {
                PointMark(x: .value("", Double(last.id)), y: .value("", ChartScale.major(last.total)))
                    .symbolSize(44)
                    .foregroundStyle(Ink.chartIncome)
                    .annotation(position: .top, alignment: .trailing, spacing: 4) {
                        Text(LedgerMoney.bare(last.total))
                            .font(Face.ui(11.5, .semibold))
                            .foregroundStyle(Ink.text)
                            .fixedSize()
                    }
            }
        }
        .chartXScale(domain: 0...Double(max(points.last?.id ?? 1, 1)), range: .plotDimension(padding: 8))
        .chartYScale(domain: scale.lower...scale.upper)
        .chartXAxis { ChartAxes.months(upTo: points.last?.id ?? 1, includeStart: true) }
        .chartYAxis { ChartAxes.money(scale) }
        .chartLegend(.hidden)
        .chartPlotStyle { $0.frame(height: 180) }
    }

    /// "start 750.00, jan 1,085.00, …": the TOTALE line, one point per
    /// spoken word instead of a shape with no numbers of its own.
    private var spoken: String {
        let opening = "\(String(localized: "start")) \(LedgerMoney.amount(year.initial))"
        let months = year.elapsed.map { "\(LedgerDate.shortMonth($0.month.month)) \(LedgerMoney.amount($0.total))" }
        return ([opening] + months).joined(separator: ", ")
    }
}

// MARK: - Shared drawing

/// The axes both charts share, in the palette: Swift Charts' defaults are the
/// system's, which are far too bright on this ground.
private enum ChartAxes {
    static func months(upTo last: Int, includeStart: Bool = false) -> some AxisContent {
        AxisMarks(values: Array((includeStart ? 0 : 1)...max(last, 1)).map(Double.init)) { value in
            AxisValueLabel {
                if let month = value.as(Double.self) {
                    Text(month == 0 ? String(localized: "start") : LedgerDate.shortMonth(Int(month)))
                        .font(Face.small)
                        .foregroundStyle(Ink.text3)
                }
            }
        }
    }

    static func money(_ scale: (lower: Double, upper: Double, step: Double)) -> some AxisContent {
        AxisMarks(values: Array(stride(from: scale.lower, through: scale.upper, by: scale.step))) { value in
            AxisGridLine(stroke: StrokeStyle(lineWidth: 1)).foregroundStyle(Ink.line)
            AxisValueLabel {
                if let amount = value.as(Double.self) {
                    Text(ChartScale.compact(Int(amount)))
                        .font(Face.small)
                        .foregroundStyle(Ink.text3)
                }
            }
        }
    }
}

/// Swift Charts' own legend cannot be styled; this is the palette's version
/// of it, with a short line for a swatch.
private struct ChartLegend: View {
    let entries: [(String, Color)]

    var body: some View {
        HStack(spacing: 12) {
            ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                HStack(spacing: 5) {
                    Capsule().fill(entry.1).frame(width: 12, height: 2)
                    Text(entry.0)
                        .font(Face.ui(11.5, .medium))
                        .foregroundStyle(Ink.text2)
                }
            }
        }
    }
}
