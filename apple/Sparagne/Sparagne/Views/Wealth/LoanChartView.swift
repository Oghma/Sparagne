import Charts
import SwiftUI
import SparagneCore

/// A loan's residual debt from the month before its first installment to
/// its last: the part behind today filled stronger than the part ahead, a
/// dashed «oggi» rule between them, and a dot on each event.
struct LoanChartView: View {
    let chart: LoanChart
    /// For VoiceOver's sentence.
    let name: String

    var body: some View {
        let scale = ChartScale.nice([ChartScale.major(chart.peak)], includeZero: true)
        Chart {
            ForEach(Array(chart.past.enumerated()), id: \.offset) { _, point in
                AreaMark(
                    x: .value("", date(point.day)),
                    yStart: .value("", 0),
                    yEnd: .value("", ChartScale.major(point.residual)),
                    series: .value("", "past")
                )
                .foregroundStyle(Ink.chartExpense.opacity(0.26))
            }
            ForEach(Array(chart.future.enumerated()), id: \.offset) { _, point in
                AreaMark(
                    x: .value("", date(point.day)),
                    yStart: .value("", 0),
                    yEnd: .value("", ChartScale.major(point.residual)),
                    series: .value("", "future")
                )
                .foregroundStyle(Ink.chartExpense.opacity(0.08))
            }
            ForEach(Array(line.enumerated()), id: \.offset) { _, point in
                LineMark(x: .value("", date(point.day)), y: .value("", ChartScale.major(point.residual)))
                    .foregroundStyle(Ink.chartExpense)
                    .lineStyle(StrokeStyle(lineWidth: 1.5, lineJoin: .round))
            }
            if let today = chart.today {
                RuleMark(x: .value("", date(today.day)))
                    .foregroundStyle(Ink.text2)
                    .lineStyle(StrokeStyle(lineWidth: 1, dash: [3, 3]))
                    .annotation(position: .top, alignment: .leading, spacing: 2, overflowResolution: .init(x: .fit(to: .chart))) {
                        Text(String(localized: "today"))
                            .font(Face.small)
                            .foregroundStyle(Ink.text2)
                    }
            }
            ForEach(Array(chart.events.enumerated()), id: \.offset) { _, point in
                PointMark(x: .value("", date(point.day)), y: .value("", ChartScale.major(point.residual)))
                    .symbolSize(36)
                    .foregroundStyle(Ink.chartExpense)
            }
        }
        .chartYScale(domain: scale.lower...scale.upper)
        // Room at both ends for the first and the last year, centred on
        // their ticks.
        .chartXScale(domain: date(first)...date(last), range: .plotDimension(startPadding: 14, endPadding: 14))
        // The years are drawn by hand, centred on their dates: the axis
        // starts a date's label at its tick whatever the anchor, and drops
        // the last one, the loan's end, off the plot's edge.
        .chartXAxis(.hidden)
        .chartOverlay { proxy in
            GeometryReader { geometry in
                if let plot = proxy.plotFrame {
                    let frame = geometry[plot]
                    ForEach(axisDates, id: \.self) { day in
                        if let x = proxy.position(forX: day) {
                            Text(day, format: .dateTime.year())
                                .font(Face.small)
                                .foregroundStyle(Ink.text3)
                                .fixedSize()
                                .position(x: frame.minX + x, y: frame.maxY + 10)
                        }
                    }
                }
            }
        }
        .chartYAxis {
            AxisMarks(position: .leading, values: Array(stride(from: scale.lower, through: scale.upper, by: scale.step))) { value in
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
        .chartLegend(.hidden)
        .chartPlotStyle { $0.frame(height: 110) }
        // Room for the years under the plot.
        .padding(.bottom, 16)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(String(localized: "Remaining debt over time"))
        .accessibilityValue(spoken)
    }

    /// The whole line, past and future, each point once.
    private var line: [LoanChart.Point] {
        chart.past + chart.future.dropFirst(chart.past.isEmpty ? 0 : 1)
    }

    private var first: NaiveDate { line.first?.day ?? "" }
    private var last: NaiveDate { line.last?.day ?? "" }

    /// The years under the chart: the start's, one every few years after
    /// it, about five across, and the end's; the one before the end goes
    /// when it would crowd it, and a year is never written twice.
    private var axisDates: [Date] {
        guard line.count > 1 else { return [] }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = CoreDate.utc
        let start = date(first)
        let end = date(last)
        let years = calendar.dateComponents([.year], from: start, to: end).year ?? 0
        let step = max(1, Int((Double(years) / 5).rounded(.up)))
        var dates: [Date] = []
        var index = 0
        while let next = calendar.date(byAdding: .year, value: index * step, to: start), next < end {
            dates.append(next)
            index += 1
        }
        if dates.count > 1, let previous = dates.last,
           (calendar.dateComponents([.month], from: previous, to: end).month ?? 0) < step * 6 {
            dates.removeLast()
        }
        if let previous = dates.last, calendar.component(.year, from: previous) == calendar.component(.year, from: end) {
            return dates
        }
        return dates + [end]
    }

    private func date(_ day: NaiveDate) -> Date {
        CoreDate.localDay(day, timeZone: CoreDate.utc) ?? Date(timeIntervalSince1970: 0)
    }

    /// "165.000,00 € on 1 Feb 2024, 152.490,96 € today, 0,00 € on 1 Feb
    /// 2049; 1 events".
    private var spoken: String {
        var parts: [String] = []
        if let start = line.first {
            parts.append(String(localized: "\(LedgerMoney.amount(start.residual)) on \(RecurringDayText.full(start.day))"))
        }
        if let today = chart.today {
            parts.append(String(localized: "\(LedgerMoney.amount(today.residual)) today"))
        }
        if let end = line.last, line.count > 1 {
            parts.append(String(localized: "\(LedgerMoney.amount(end.residual)) on \(RecurringDayText.full(end.day))"))
        }
        if !chart.events.isEmpty {
            parts.append(String(localized: "\(chart.events.count) events"))
        }
        return [name, parts.joined(separator: ", ")].joined(separator: ": ")
    }
}
