import Foundation
import SparagneCore

/// The words of the Riparto tab: a line's rule and its status, a decided
/// period, the warnings. Amounts are the window's fixed `4.250,00`
/// (`LedgerMoney`), percentages `12,5%` (`AllocationPercent`).
///
/// `bundle` is the catalog to read the plural keys from, as `CountText`
/// takes it.
enum AllocationText {
    // MARK: - Rules

    /// The segmented Fisso / % / Al tetto.
    static func kind(_ kind: AllocationRuleKind) -> String {
        switch kind {
        case .fixed: String(localized: "Fixed")
        case .percent: "%"
        case .fillToCap: String(localized: "To cap")
        }
    }

    /// The Regola column of a closed row: `"Fisso 1.400,00"`, `"10% del
    /// totale"`, `"Fino al tetto 3.000,00"`. `cap` is the envelope's, for
    /// Al tetto; without one the rule says only what it would do.
    static func rule(_ rule: AllocationRule, cap: Int64?) -> String {
        switch rule {
        case .fixed(let amount):
            return String(localized: "Fixed \(LedgerMoney.bare(amount))")
        case .percent(let basisPoints):
            return String(localized: "\(AllocationPercent.label(basisPoints)) of the total")
        case .fillToCap:
            guard let cap else { return String(localized: "Up to the cap") }
            return String(localized: "Up to the cap \(LedgerMoney.bare(cap))")
        }
    }

    // MARK: - Status

    /// A line's Stato tag, and whether it is drawn as a warning.
    struct Tag: Equatable {
        let text: String
        let warns: Bool
    }

    /// `"completo"`, `"al tetto"`, `"mancano 327,50"`, `"senza tetto"`,
    /// `"archiviata"`. An Al tetto line on an envelope already at its cap is
    /// complete with nothing to get: `"già piena"`. A shortfall warns only
    /// when `due`: worked out on what has come in so far, it is no news.
    static func tag(_ line: PreviewLine, due: Bool = true) -> Tag {
        switch line.status {
        case .full:
            if line.rule == .fillToCap, line.wanted == 0 {
                return Tag(text: String(localized: "already full"), warns: false)
            }
            return Tag(text: String(localized: "complete"), warns: false)
        case .capLimited:
            return Tag(text: String(localized: "at the cap"), warns: false)
        case .short:
            let missing = LedgerMoney.bare(max(line.wanted - line.amount, 0))
            return Tag(text: String(localized: "\(missing) short"), warns: due)
        case .noCap:
            return Tag(text: String(localized: "no cap"), warns: true)
        case .archived:
            return Tag(text: String(localized: "archived"), warns: true)
        }
    }

    /// The due card's chip for a line that gets less than it asked for:
    /// `"Vacanze riceve 327,50 in meno"`.
    static func less(_ line: PreviewLine, name: String) -> String {
        String(localized: "\(name) gets \(LedgerMoney.bare(line.wanted - line.amount)) less")
    }

    // MARK: - The period to share out

    /// `"2 entrate arrivate in Non allocato dopo gio 27 ago"`: after the
    /// latest decided period, or since the plan's start before the first.
    /// A weekday leads the day, so the Italian needs no article.
    static func incomes(
        _ count: Int,
        after decided: NaiveDate?,
        since start: NaiveDate,
        bundle: Bundle = .main,
        locale: Locale = .autoupdatingCurrent
    ) -> String {
        guard count > 0 else {
            return String(localized: "No income reached Unallocated", bundle: bundle, locale: locale)
        }
        let arrived = String(localized: "\(count) incomes reached Unallocated", bundle: bundle, locale: locale)
        if let decided {
            let day = RecurringDayText.weekday(decided, locale: locale)
            return String(localized: "\(arrived) after \(day)", bundle: bundle, locale: locale)
        }
        let day = RecurringDayText.weekday(start, locale: locale)
        return String(localized: "\(arrived) since \(day)", bundle: bundle, locale: locale)
    }

    /// `"comprende 2 periodi precedenti"`: older periods nobody decided,
    /// which this decision closes too.
    static func missed(_ count: Int, bundle: Bundle = .main, locale: Locale = .autoupdatingCurrent) -> String {
        String(localized: "\(count) earlier periods included", bundle: bundle, locale: locale)
    }

    /// `"5 buste"`.
    static func envelopes(_ count: Int, bundle: Bundle = .main, locale: Locale = .autoupdatingCurrent) -> String {
        String(localized: "\(count) envelopes", bundle: bundle, locale: locale)
    }

    /// The tab's count, as VoiceOver reads it: `"1 da distribuire"`.
    static func toShareOut(_ count: Int, bundle: Bundle = .main, locale: Locale = .autoupdatingCurrent) -> String {
        String(localized: "\(count) to share out", bundle: bundle, locale: locale)
    }

    // MARK: - History

    static func outcome(_ outcome: RunOutcome) -> String {
        switch outcome {
        case .executed: String(localized: "allocation.outcome.executed", defaultValue: "Shared out")
        case .skipped: String(localized: "allocation.outcome.skipped", defaultValue: "Skipped")
        }
    }

    /// What went into the envelopes, transfers deleted since left out.
    static func distributed(_ run: AllocationRunView) -> Int64 {
        run.moves.filter { !$0.voided }.reduce(0) { $0 + $1.amount }
    }

    /// `"5 buste su 4.250,00 · Matteo"`, `"entrate gestite a mano · Elisa"`.
    static func detail(_ run: AllocationRunView) -> String {
        let what = switch run.outcome {
        case .executed:
            String(localized: "\(envelopes(run.moves.count)) out of \(LedgerMoney.bare(run.total))")
        case .skipped:
            String(localized: "incomes handled by hand")
        }
        return run.createdBy.isEmpty ? what : "\(what) \u{00B7} \(run.createdBy)"
    }

    // MARK: - The plan

    /// The inspector's footer: `"da distribuire ogni 27"`. Nothing for an
    /// interval above one, which the heading's cadence says whole.
    static func cadenceHint(_ schedule: Schedule, locale: Locale = .autoupdatingCurrent) -> String? {
        guard schedule.interval == 1 else { return nil }
        switch schedule.frequency {
        case .daily:
            return nil
        case .weekly(let weekday):
            let name = ScheduleFormatting.weekdayName(weekday, locale: locale)
            return String(localized: "due every \(name)")
        case .monthly(let day):
            return String(localized: "due on day \(Int(day))")
        case .yearly(let month, let day):
            let name = ScheduleFormatting.monthName(month, locale: locale)
            return String(localized: "due every \(name) \(Int(day))")
        }
    }

    /// One warning as the inspector's Avvisi write it: the envelope's name as
    /// a tag, when it is about one, and the sentence after it.
    static func warning(_ warning: AllocationWarning, names: NameBook) -> (tag: String?, text: String) {
        let name: (Uuid) -> String = { names.flow($0) ?? TransactionRow.placeholder }
        switch warning {
        case .archived(let flowId):
            return (name(flowId), String(localized: "is archived, so its line gets nothing."))
        case .noCap(let flowId):
            return (name(flowId), String(localized: "has no cap, so its line gets nothing."))
        case .percentOver(let basisPoints):
            let total = AllocationPercent.label(basisPoints)
            return (nil, String(localized: "Percentages add up to \(total): the total does not cover them all."))
        case .short(let flowId, let amount, let wanted):
            let gets = LedgerMoney.bare(amount)
            let asked = LedgerMoney.bare(wanted)
            return (name(flowId), String(localized: "gets \(gets) instead of \(asked): the total does not cover every line."))
        case .overdrawn(let after):
            let balance = LedgerMoney.bare(after)
            return (nil, String(localized: "Unallocated would go to \(balance): the total is more than it holds."))
        }
    }

    /// What Avvisi says when there is nothing to warn about.
    static func allClear(percent: UInt32) -> String {
        percent > 0
            ? String(localized: "Percentages add up to \(AllocationPercent.label(percent)): nothing to flag.")
            : String(localized: "Nothing to flag.")
    }
}
