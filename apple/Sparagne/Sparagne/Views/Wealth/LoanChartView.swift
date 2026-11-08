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
        .chartXScale(domain: date(first)...date(last))
        .chartXAxis {
            AxisMarks(values: .stride(by: .year, count: yearStep)) { value in
                AxisValueLabel(collisionResolution: .greedy) {
                    if let day = value.as(Date.self) {
                        Text(day, format: .dateTime.year())
                            .font(Face.small)
                            .foregroundStyle(Ink.text3)
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

    /// One label every so many years: about five across.
    private var yearStep: Int {
        let years = (Int(last.prefix(4)) ?? 0) - (Int(first.prefix(4)) ?? 0)
        return max(1, Int((Double(years) / 5).rounded(.up)))
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
