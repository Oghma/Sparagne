import Charts
import SwiftUI

/// The two charts under the table, side by side at half
/// the width each.
struct SummaryCharts: View {
    let year: YearSummary

    var body: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8, alignment: .top), count: 2), spacing: 8) {
            CashFlowCard(year: year)
            CashFundCard(year: year)
        }
    }
}

// MARK: - Cash flow

/// ENTRATE, USCITE and RISPARMIO month by month. The line into today's
/// month is dashed: that month is not over, so the segment is a promise and
/// not a fact. A month gone by is drawn solid, even as the last one.
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

    /// Whether the last month drawn is still going: the one on screen, and
    /// the one today falls in. August looked at in October is over.
    private var endsUnfinished: Bool { year.isLive && months.last?.month == year.upTo }

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
        .chartXScale(domain: 1...Double(max(months.last?.month.month ?? 2, 2)), range: ChartAxes.monthRange)
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
///
/// With two or more people and no PERSONA filter, a thin line per person
/// runs under the household's: each one's share of it. In today's month,
/// a dashed tail carries the pace of the finished months on to December
/// (`YearSummary.projection`).
private struct CashFundCard: View {
    let year: YearSummary

    private struct Point: Identifiable {
        /// 0 is the start of the year, 1...12 the months.
        let id: Int
        let total: Int64
    }

    /// One person's share of TOTALE, from the start of the year.
    private struct PersonLine: Identifiable {
        /// The person's place in `YearSummary.people`.
        let id: Int
        let name: String
        let points: [Point]
    }

    private var points: [Point] {
        [Point(id: 0, total: year.initial)] + year.elapsed.map { Point(id: $0.month.month, total: $0.total) }
    }

    /// The months where TOTALE went down on the one before.
    private var drops: [Point] {
        zip(points, points.dropFirst()).filter { $0.1.total < $0.0.total }.map(\.1)
    }

    /// None under the PERSONA filter, where the one line would be the
    /// household's own, nor with one person.
    private var personLines: [PersonLine] {
        guard year.person == nil, year.people.count >= 2 else { return [] }
        return year.people.enumerated().compactMap { index, name in
            guard year.initialByPerson.indices.contains(index) else { return nil }
            let months = year.elapsed.compactMap { month in
                month.totalByPerson.indices.contains(index)
                    ? Point(id: month.month.month, total: month.totalByPerson[index]) : nil
            }
            return PersonLine(id: index, name: name, points: [Point(id: 0, total: year.initialByPerson[index])] + months)
        }
    }

    /// From the last finished month to December, where the year would end
    /// at their pace. Empty unless the month on screen is today's and one
    /// finished before it.
    private var tail: [Point] {
        guard let projection = year.projection, let last = year.finished.last else { return [] }
        return [Point(id: last.month.month, total: last.total), Point(id: 12, total: projection.yearEnd)]
    }

    /// The last month on the axis: December when the tail reaches it.
    private var lastMonth: Int { tail.isEmpty ? max(points.last?.id ?? 1, 1) : 12 }

    private var scale: (lower: Double, upper: Double, step: Double) {
        let values = (points + personLines.flatMap(\.points) + tail).map { ChartScale.major($0.total) }
        return ChartScale.nice(values, includeZero: false)
    }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                // The growth of every person makes the line long: under the
                // title when the two do not fit side by side.
                ViewThatFits(in: .horizontal) {
                    HStack(alignment: .firstTextBaseline) {
                        title
                        Spacer(minLength: 8)
                        growthLine
                    }
                    VStack(alignment: .leading, spacing: 2) {
                        title
                        growthLine
                    }
                }
                chart
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(String(localized: "Cash fund"))
        .accessibilityValue(spoken)
    }

    private var title: some View {
        Text(String(localized: "Cash fund \u{00B7} total at month end"))
            .font(Face.label)
            .foregroundStyle(Ink.text2)
            .lineLimit(1)
    }

    private var growthLine: some View {
        Text(growthText)
            .font(Face.label)
            .foregroundStyle(Ink.text3)
            .lineLimit(1)
    }

    /// `+29.966,23 since the start of the year · elisa +9.180,23 · matteo
    /// +20.786,00`: each person's growth after the household's, up to three
    /// people; past that the line would not fit, and the "By person" card
    /// has them all.
    private var growthText: String {
        let total = String(localized: "\(signed(year.growth)) since the start of the year")
        let people = year.personFigures
        guard year.person == nil, (2...3).contains(people.count) else { return total }
        return ([total] + people.map { "\($0.person) \(signed($0.growth))" }).joined(separator: " \u{00B7} ")
    }

    private func signed(_ amount: Int64) -> String {
        SummaryText.signed(LedgerMoney.bare(amount), positive: amount >= 0)
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
                LineMark(
                    x: .value("", Double(point.id)),
                    y: .value("", ChartScale.major(point.total)),
                    series: .value("", "total")
                )
                .foregroundStyle(Ink.chartIncome)
                .lineStyle(StrokeStyle(lineWidth: 2, lineJoin: .round))
            }
            ForEach(personLines) { line in
                ForEach(line.points) { point in
                    LineMark(
                        x: .value("", Double(point.id)),
                        y: .value("", ChartScale.major(point.total)),
                        series: .value("", "person-\(line.id)")
                    )
                    .foregroundStyle(Ink.text3)
                    .lineStyle(StrokeStyle(lineWidth: 1, lineJoin: .round))
                }
                // The name at the end of the line: the header says who grew
                // how much, this says which line is whose.
                if let end = line.points.last {
                    PointMark(x: .value("", Double(end.id)), y: .value("", ChartScale.major(end.total)))
                        .symbolSize(10)
                        .foregroundStyle(Ink.text3)
                        .annotation(position: .top, alignment: .trailing, spacing: 2) {
                            Text(line.name)
                                .font(Face.small)
                                .foregroundStyle(Ink.text3)
                                .fixedSize()
                        }
                }
            }
            ForEach(tail) { point in
                LineMark(
                    x: .value("", Double(point.id)),
                    y: .value("", ChartScale.major(point.total)),
                    series: .value("", "projection")
                )
                .foregroundStyle(Ink.chartIncome)
                .lineStyle(StrokeStyle(lineWidth: 1.5, lineJoin: .round, dash: [4, 4]))
            }
            if let end = tail.last {
                PointMark(x: .value("", Double(end.id)), y: .value("", ChartScale.major(end.total)))
                    .symbolSize(16)
                    .foregroundStyle(Ink.chartIncome)
                    .annotation(position: .top, alignment: .trailing, spacing: 4) {
                        Text(LedgerMoney.bare(end.total))
                            .font(Face.ui(11, .medium))
                            .foregroundStyle(Ink.text2)
                            .fixedSize()
                    }
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
        .chartXScale(domain: 0...Double(lastMonth), range: ChartAxes.monthRange)
        .chartYScale(domain: scale.lower...scale.upper)
        .chartXAxis { ChartAxes.months(upTo: lastMonth, includeStart: true) }
        .chartYAxis { ChartAxes.money(scale) }
        .chartLegend(.hidden)
        .chartPlotStyle { $0.frame(height: 180) }
    }

    /// "+29.966,23 since the start of the year · elisa +9.180,23; start
    /// 750.00, jan 1,085.00, …; projected year end €36,000.00; elisa: start
    /// …": the header, the TOTALE line, one point per spoken word instead
    /// of a shape with no numbers of its own, then where the year would end
    /// and each person's line.
    private var spoken: String {
        var parts = [growthText, Self.spoken(points)]
        if let projection = year.projection {
            parts.append(String(localized: "projected year end \(LedgerMoney.amount(projection.yearEnd))"))
        }
        for line in personLines {
            parts.append("\(line.name): \(Self.spoken(line.points))")
        }
        return parts.joined(separator: "; ")
    }

    private static func spoken(_ points: [Point]) -> String {
        points.map { point in
            let name = point.id == 0 ? String(localized: "start") : LedgerDate.shortMonth(point.id)
            return "\(name) \(LedgerMoney.amount(point.total))"
        }
        .joined(separator: ", ")
    }
}

// MARK: - Shared drawing

/// The axes both charts share, in the palette: Swift Charts' defaults are the
/// system's, which are far too bright on this ground.
private enum ChartAxes {
    /// The months' x range. The last month's label is centered on the
    /// plot's trailing edge, where the money axis sits: left to resolve the
    /// collision itself, Swift Charts dropped it, and the month on screen
    /// went missing under its own line. The padding at the end keeps the
    /// label clear of that axis.
    static let monthRange = PlotDimensionScaleRange.plotDimension(startPadding: 8, endPadding: 14)

    static func months(upTo last: Int, includeStart: Bool = false) -> some AxisContent {
        AxisMarks(values: Array((includeStart ? 0 : 1)...max(last, 1)).map(Double.init)) { value in
            // Every month gets its label: twelve short names fit half the
            // window, and the one dropped first was always the last.
            AxisValueLabel(collisionResolution: .disabled) {
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
