import Foundation
import SparagneCore

/// `AllocationRule` as a flat choice, the segmented Fisso / % / Al tetto.
enum AllocationRuleKind: String, CaseIterable, Identifiable, Hashable {
    case fixed
    case percent
    case fillToCap

    var id: String { rawValue }

    init(_ rule: AllocationRule) {
        switch rule {
        case .fixed: self = .fixed
        case .percent: self = .percent
        case .fillToCap: self = .fillToCap
        }
    }

    /// Whether the kind needs a value typed beside it.
    var takesValue: Bool { self != .fillToCap }
}

/// The cell at fault in a refused line. The table reports `underlying` and
/// puts the focus back on the line's value, whichever cell it was.
enum AllocationField: Hashable {
    case envelope
    case value
}

struct AllocationDraftError: Error {
    let field: AllocationField
    let underlying: Error
}

/// The cells of the plan's line being edited, or of the empty line.
struct AllocationLineDraft: Equatable {
    /// `nil` on the empty line until an envelope is picked.
    var flowId: Uuid?
    var kind: AllocationRuleKind = .fixed
    /// A fixed line's amount, as typed: a half-typed amount never has to be
    /// a number. Kept apart from `percent` while the user tries the kinds,
    /// so switching back loses nothing.
    var amount = ""
    /// A percent line's share, as typed: `"12,5"`.
    var percent = ""

    init() {}

    init(line: AllocationLine) {
        flowId = line.flowId
        kind = AllocationRuleKind(line.rule)
        switch line.rule {
        case .fixed(let value): amount = LedgerMoney.bare(value)
        case .percent(let basisPoints): percent = AllocationPercent.text(basisPoints)
        case .fillToCap: break
        }
    }

    /// The text of the value cell, whichever kind is chosen; nothing for Al
    /// tetto, which asks for no value.
    var value: String {
        get {
            switch kind {
            case .fixed: amount
            case .percent: percent
            case .fillToCap: ""
            }
        }
        set {
            switch kind {
            case .fixed: amount = newValue
            case .percent: percent = newValue
            case .fillToCap: break
            }
        }
    }

    /// Whether anything was picked or typed: ↩ on an untouched empty line
    /// is not an error, it is nothing.
    var isBlank: Bool {
        flowId == nil && amount.trimmingCharacters(in: .whitespaces).isEmpty
            && percent.trimmingCharacters(in: .whitespaces).isEmpty
    }

    /// The rule the cells say, or the cell that does not say one.
    func rule(currency: Currency) throws -> AllocationRule {
        switch kind {
        case .fixed:
            let money: Int64
            do {
                money = try AllocationMoney.parse(amount, currency: currency)
            } catch {
                throw AllocationDraftError(field: .value, underlying: error)
            }
            guard money > 0 else {
                throw AllocationDraftError(
                    field: .value,
                    underlying: DomainError.InvalidCommand(
                        message: String(localized: "A fixed line needs an amount above zero")
                    )
                )
            }
            return .fixed(amount: money)
        case .percent:
            guard let basisPoints = AllocationPercent.parse(percent) else {
                throw AllocationDraftError(
                    field: .value,
                    underlying: DomainError.InvalidCommand(
                        message: String(localized: "A percentage goes from 0,01 to 100")
                    )
                )
            }
            return .percent(basisPoints: basisPoints)
        case .fillToCap:
            return .fillToCap
        }
    }

    /// The line the cells say. Throws for a line with no envelope or with a
    /// value the rule cannot take.
    func line(currency: Currency) throws -> AllocationLine {
        guard let flowId else {
            throw AllocationDraftError(
                field: .envelope,
                underlying: DomainError.InvalidCommand(message: String(localized: "Pick an envelope for the line"))
            )
        }
        return AllocationLine(flowId: flowId, rule: try rule(currency: currency))
    }

    func isValid(currency: Currency) -> Bool {
        (try? line(currency: currency)) != nil
    }

    /// What the Busta menu of the line that had envelope `key` offers, `nil`
    /// for the empty line: the active envelopes but Unallocated, less those
    /// another line already has.
    static func offered(_ flows: [FlowView], lines: [AllocationLine], keeping key: Uuid?) -> [FlowView] {
        let taken = Set(lines.map(\.flowId)).subtracting([key].compactMap { $0 })
        return flows.filter { !$0.isUnallocated && !taken.contains($0.id) }
    }

    /// The cap of the envelope picked, which an Al tetto line fills up to;
    /// `nil` with no envelope, or one without a cap.
    func cap(in flows: [FlowView]) -> Int64? {
        flowId.flatMap { id in flows.first { $0.id == id } }.flatMap { EnvelopeCapKind.cap(of: $0.mode) }
    }

    /// `lines` with this draft written in: in place of the line of envelope
    /// `replacing`, or at the end for the empty line (`nil`). An envelope is
    /// in a plan once, as the core requires, so one that another line already
    /// has is refused on the envelope cell.
    func committing(into lines: [AllocationLine], replacing: Uuid?, currency: Currency) throws -> [AllocationLine] {
        let line = try line(currency: currency)
        if lines.contains(where: { $0.flowId == line.flowId && $0.flowId != replacing }) {
            throw AllocationDraftError(
                field: .envelope,
                underlying: DomainError.InvalidCommand(
                    message: String(localized: "That envelope already has a line in the plan")
                )
            )
        }
        guard let replacing, let index = lines.firstIndex(where: { $0.flowId == replacing }) else {
            return lines + [line]
        }
        var updated = lines
        updated[index] = line
        return updated
    }
}

/// Money as the plan's cells take it.
enum AllocationMoney {
    /// The window writes `4.250,00`, with a dot between thousands that the
    /// core's `parseMoney` does not read, so the dots before a decimal comma
    /// are dropped first. Without a comma, dots that each start a group of
    /// exactly three digits (`1.400`, `12.500.000`) are thousands too; any
    /// other dot is the decimal point (`4250.5`).
    static func parse(_ text: String, currency: Currency) throws -> Int64 {
        var trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if let comma = trimmed.lastIndex(of: ","), trimmed[..<comma].contains(".") {
            trimmed = trimmed[..<comma].replacingOccurrences(of: ".", with: "") + trimmed[comma...]
        } else if !trimmed.contains(","), groupedInThousands(trimmed) {
            trimmed = trimmed.replacingOccurrences(of: ".", with: "")
        }
        return try parseMoney(text: trimmed, currency: currency)
    }

    /// `1.400`, `12.500.000`, with a sign or not: one to three digits, then
    /// one or more dots each followed by exactly three.
    private static func groupedInThousands(_ text: String) -> Bool {
        let unsigned = text.first == "-" || text.first == "+" ? text.dropFirst() : Substring(text)
        let groups = unsigned.split(separator: ".", omittingEmptySubsequences: false)
        guard groups.count >= 2, let first = groups.first, (1...3).contains(first.count) else { return false }
        let digits: (Substring) -> Bool = { $0.allSatisfy { ("0"..."9").contains($0) } }
        return digits(first) && groups.dropFirst().allSatisfy { $0.count == 3 && digits($0) }
    }

    /// The amount, or `nil` while the text is not one.
    static func amount(_ text: String, currency: Currency) -> Int64? {
        try? parse(text, currency: currency)
    }
}

/// Percentages as basis points, the unit of `AllocationRule.percent`:
/// 12,5% is 1250.
enum AllocationPercent {
    /// The whole: 100% in basis points.
    static let full: UInt32 = 10_000

    /// `"12,5"` or `"12.5%"` as basis points, whatever its size; `nil` for
    /// anything that is not a number with at most two decimals. Either
    /// separator reads as the decimal one: a percentage has no thousands.
    static func basisPoints(_ text: String) -> UInt32? {
        var trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasSuffix("%") {
            trimmed.removeLast()
            trimmed = trimmed.trimmingCharacters(in: .whitespaces)
        }
        let parts = trimmed.replacingOccurrences(of: ",", with: ".").split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count <= 2 else { return nil }
        let whole = parts[0]
        let fraction = parts.count == 2 ? parts[1] : ""
        let digits: (Substring) -> Bool = { $0.allSatisfy { ("0"..."9").contains($0) } }
        guard !whole.isEmpty || !fraction.isEmpty, digits(whole), digits(fraction),
              whole.count <= 6, fraction.count <= 2,
              let units = UInt32(whole.isEmpty ? "0" : whole),
              let hundredths = UInt32((fraction + "00").prefix(2))
        else { return nil }
        return units * 100 + hundredths
    }

    /// The basis points a percent line may ask for, 1 to 10000, or `nil`.
    static func parse(_ text: String) -> UInt32? {
        basisPoints(text).flatMap { (1...full).contains($0) ? $0 : nil }
    }

    /// `1250` → `"12,5"`, `1000` → `"10"`: with the window's decimal comma
    /// and no trailing zeros.
    static func text(_ basisPoints: UInt32) -> String {
        let units = basisPoints / 100
        let hundredths = basisPoints % 100
        if hundredths == 0 { return "\(units)" }
        if hundredths % 10 == 0 { return "\(units),\(hundredths / 10)" }
        return "\(units)," + (hundredths < 10 ? "0" : "") + "\(hundredths)"
    }

    /// `"12,5%"`.
    static func label(_ basisPoints: UInt32) -> String {
        text(basisPoints) + "%"
    }
}

/// The plan's Quando in the inspector, the same draft as a template's.
typealias AllocationScheduleDraft = ScheduleDraft

extension ScheduleDraft {
    /// The schedule, when it is not `plan`'s; nothing otherwise.
    func patch(against plan: AllocationPlanView) -> AllocationPlanPatch {
        schedule == plan.schedule ? AllocationPlanPatch() : AllocationPlanPatch(schedule: schedule)
    }

    func isDirty(against plan: AllocationPlanView) -> Bool {
        !patch(against: plan).isEmpty
    }
}

extension AllocationPlanPatch {
    /// `UpdateAllocationPlan` refuses a patch that changes nothing.
    var isEmpty: Bool { self == AllocationPlanPatch() }
}
