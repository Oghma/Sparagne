import Foundation

/// The numbers behind a chart's axis, kept out of the view so they can be
/// checked: Swift Charts' own "nice" ticks do not know about the direct
/// label and the area's baseline, which both need the real bounds.
enum ChartScale {
    /// Minor units are the model's truth; a chart only has to be the right
    /// height, so this is the one place a `Double` is allowed.
    static func major(_ minorUnits: Int64) -> Double { Double(minorUnits) / 100 }

    /// Bounds and a tick step that cover `values` in about four steps, on a
    /// 1-2-5 grid. `includeZero` anchors a flow chart on its zero line.
    static func nice(_ values: [Double], includeZero: Bool) -> (lower: Double, upper: Double, step: Double) {
        var low = values.min() ?? 0
        var high = values.max() ?? 0
        if includeZero {
            low = min(low, 0)
            high = max(high, 0)
        }
        let span = max(high - low, 1)
        let raw = span / 4
        let magnitude = pow(10, floor(log10(raw)))
        let fraction = raw / magnitude
        let step = magnitude * (fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 5 ? 5 : 10)
        let lower = (low / step).rounded(.down) * step
        var upper = (high / step).rounded(.up) * step
        if upper == lower { upper = lower + step }
        return (lower, upper, step)
    }

    /// `12k`, `1,5k` (`1.5k` in English), `840`: built with integers, so no
    /// rounding surprises. Only the separator comes from the locale, like
    /// every other figure the app prints.
    static func compact(_ major: Int, locale: Locale = .autoupdatingCurrent) -> String {
        let separator = locale.decimalSeparator ?? "."
        let sign = major < 0 ? "-" : ""
        let value = abs(major)
        guard value >= 1_000 else { return "\(sign)\(value)" }
        let thousands = value / 1_000
        let tenth = (value % 1_000) / 100
        return tenth == 0 ? "\(sign)\(thousands)k" : "\(sign)\(thousands)\(separator)\(tenth)k"
    }
}
