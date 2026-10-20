import Foundation
import SparagneCore

/// The "Move between envelopes" sheet's draft: the envelope the money
/// leaves, the envelopes it goes to with the amount typed for each, one day
/// and one note. Plain values, so the checks and the commands are tested
/// without a window (`EnvelopeMoveTests`).
///
/// Each line becomes an ordinary transfer between envelopes, the one `tf>`
/// writes, and the store sends them as one batch
/// (`AppStore.moveBetweenEnvelopes`): all of them or none. It decides no
/// allocation period: the plan and its periods stay as they are.
struct EnvelopeMoveDraft: Equatable {
    /// One envelope the money goes to, with its amount as typed.
    struct Line: Identifiable, Equatable {
        let flowId: Uuid
        var amount = ""

        var id: Uuid { flowId }
    }

    /// Where the money leaves from: Unallocated when the sheet opens.
    private(set) var sourceId: Uuid
    /// In the order they were added. Never the source, never one envelope
    /// twice.
    private(set) var lines: [Line] = []
    var day: NaiveDate
    var note = ""

    init(sourceId: Uuid, day: NaiveDate) {
        self.sourceId = sourceId
        self.day = day
    }

    /// Another source, and its line dropped: an envelope does not move
    /// money to itself.
    mutating func setSource(_ flowId: Uuid) {
        sourceId = flowId
        lines.removeAll { $0.flowId == flowId }
    }

    /// A line for `flowId`, unless it is the source or has one already.
    mutating func add(_ flowId: Uuid) {
        guard flowId != sourceId, !lines.contains(where: { $0.flowId == flowId }) else { return }
        lines.append(Line(flowId: flowId))
    }

    mutating func remove(_ flowId: Uuid) {
        lines.removeAll { $0.flowId == flowId }
    }

    mutating func setAmount(_ text: String, for flowId: Uuid) {
        guard let index = lines.firstIndex(where: { $0.flowId == flowId }) else { return }
        lines[index].amount = text
    }

    /// "To cap": as much as the envelope can still take.
    mutating func fill(_ flowId: Uuid, room: Int64) {
        guard room > 0 else { return }
        setAmount(LedgerMoney.bare(room), for: flowId)
    }

    /// The draft against `flows`, the vault's envelopes as they are now, and
    /// `balances`, what each held on the draft's day and afterwards
    /// (`AppStore.envelopeBalances(on:)`). An envelope missing from
    /// `balances` is taken as it is now, which is right for a move dated
    /// today with nothing dated after it.
    ///
    /// The core adds and subtracts balances without overflow checks, so a
    /// line that would take the total, or the source once the total has left
    /// it, past what an `Int64` holds is too large, and the total leaves it
    /// out.
    func check(
        flows: [FlowView],
        currency: Currency,
        balances: [Uuid: EnvelopeMoveBalance] = [:]
    ) -> EnvelopeMoveCheck {
        let byId = Dictionary(flows.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let balance = { (flow: FlowView) in balances[flow.id] ?? EnvelopeMoveBalance(now: flow) }
        let source = byId[sourceId]
        let sourceBalance = source.map(balance)
        var rows = lines.map { line in
            let envelope = byId[line.flowId]
            return EnvelopeMoveCheck.row(line, envelope: envelope, balance: envelope.map(balance), currency: currency)
        }
        var total: Int64 = 0
        for index in rows.indices {
            guard let amount = rows[index].amount else { continue }
            let (sum, overflow) = total.addingReportingOverflow(amount)
            // The lowest point is the furthest from zero the total can take
            // the source: if it holds there, it holds everywhere.
            let sourceOverflow = sourceBalance.map { $0.low.subtractingReportingOverflow(sum).overflow } ?? false
            if overflow || sourceOverflow {
                rows[index].issue = .tooLarge
            } else {
                total = sum
            }
        }
        return EnvelopeMoveCheck(rows: rows, source: source, sourceBalance: sourceBalance, total: total)
    }

    /// One transfer per line, out of the source, on `occurredAt`, carrying
    /// the note when there is one. None unless the check against `flows` and
    /// `balances` lets the draft move.
    func commands(
        flows: [FlowView],
        currency: Currency,
        balances: [Uuid: EnvelopeMoveBalance] = [:],
        occurredAt: OffsetDateTime
    ) -> [Command] {
        let check = check(flows: flows, currency: currency, balances: balances)
        guard check.canMove else { return [] }
        let trimmed = note.trimmingCharacters(in: .whitespacesAndNewlines)
        return check.rows.compactMap { row in
            row.amount.map {
                .transferFlow(
                    amount: $0,
                    fromFlowId: sourceId,
                    toFlowId: row.flowId,
                    note: trimmed.isEmpty ? nil : trimmed,
                    occurredAt: occurredAt
                )
            }
        }
    }
}

/// What an envelope holds at the end of the move's day, and how low and how
/// high it goes from then on (`FlowBalanceRange`).
///
/// A transfer dated in the past changes every balance after it, not only
/// today's: the source can give only what it still holds at its lowest
/// point from that day on, and an envelope with a cap on its balance can
/// take only what fits under the cap at its highest.
struct EnvelopeMoveBalance: Equatable {
    /// At the end of the move's day.
    let balance: Int64
    /// The lowest from that day on, `balance` included.
    let low: Int64
    /// The day it first gets there; `nil` when `balance` is already the
    /// lowest.
    let lowOn: NaiveDate?
    /// The highest from that day on, `balance` included.
    let high: Int64
}

extension EnvelopeMoveBalance {
    /// The envelope as it is now, all three figures its balance: what a
    /// move dated today sees when nothing is dated after it, and what the
    /// sheet shows until the core has said otherwise.
    init(now envelope: FlowView) {
        self.init(balance: envelope.balance, low: envelope.balance, lowOn: nil, high: envelope.balance)
    }

    /// The core's figures, the instant of the lowest point read as a day in
    /// `timeZone`.
    init(_ range: FlowBalanceRange, timeZone: TimeZone = .current) {
        self.init(
            balance: range.balance,
            low: range.low,
            lowOn: range.lowAt.flatMap(CoreDate.date).map { CoreDate.day($0, timeZone: timeZone) },
            high: range.high
        )
    }
}

/// What a draft would do to the envelopes, on its day and from then on: each
/// line's amount, room and balance after, the total, what the source is left
/// with, and what, if anything, stops the move.
///
/// The rules are the core's (`Flow::apply_leg_change`), checked here so the
/// sheet can say which line is wrong before anything is sent: a line may not
/// take its envelope past its cap, and the source may not go below zero
/// unless it is Unallocated or allows it. Unallocated below zero is only
/// pointed out, as the Riparto's preview does. The core checks them on the
/// balances of today; here they hold on every day from the move's on, so a
/// move dated in the past cannot spend money that came in only later.
struct EnvelopeMoveCheck: Equatable {
    struct Row: Identifiable, Equatable {
        let flowId: Uuid
        /// `nil` once the envelope is gone: archived or deleted elsewhere
        /// while the sheet was open.
        let envelope: FlowView?
        /// On the move's day and after; `nil` once the envelope is gone.
        let balance: EnvelopeMoveBalance?
        /// What it can still take under its cap, on every day from the
        /// move's on; `nil` without a cap.
        let room: Int64?
        /// `nil` while nothing usable is typed.
        let amount: Int64?
        var issue: Issue?

        var id: Uuid { flowId }

        /// Its balance on the move's day once the move is done.
        var after: Int64? {
            balance.map { EnvelopeMoveCheck.adding($0.balance, amount ?? 0) }
        }
    }

    enum Issue: Equatable {
        /// Nothing typed yet.
        case empty
        /// Typed, but not an amount above zero.
        case notAnAmount
        /// More than the room under the cap, by `excess`.
        case overCap(excess: Int64)
        /// More than a balance or the total can hold.
        case tooLarge
        /// The envelope is no longer there.
        case gone
    }

    /// `on` is the day the source is furthest below zero, when that is
    /// after the move's own day: `nil` when it is the move's day.
    enum SourceState: Equatable {
        case fine
        /// Below zero by `shortfall`, which the source allows: Unallocated
        /// always does, another envelope when it is set to.
        case belowZero(shortfall: Int64, on: NaiveDate? = nil)
        /// Below zero by `shortfall`, which the source does not allow.
        case short(shortfall: Int64, on: NaiveDate? = nil)
        /// The source is no longer there.
        case gone
    }

    /// What stops the move, the first of it: what the footer says.
    enum Blocker: Equatable {
        case noLines
        case line(flowId: Uuid, issue: Issue)
        case sourceShort(shortfall: Int64)
        case sourceGone
    }

    let rows: [Row]
    let source: FlowView?
    /// The source on the move's day and after; `nil` once it is gone.
    let sourceBalance: EnvelopeMoveBalance?
    let total: Int64

    /// The source's balance on the move's day once every line has left it.
    var sourceAfter: Int64? {
        sourceBalance.map { Self.subtracting($0.balance, total) }
    }

    /// The lowest the source gets afterwards, once every line has left it:
    /// `nil` when that is the move's own day, which `sourceAfter` already
    /// says.
    var sourceLowAfter: (amount: Int64, on: NaiveDate)? {
        guard let sourceBalance, let day = sourceBalance.lowOn else { return nil }
        return (Self.subtracting(sourceBalance.low, total), day)
    }

    var sourceState: SourceState {
        guard let source, let sourceBalance else { return .gone }
        let lowest = Self.subtracting(sourceBalance.low, total)
        guard lowest < 0 else { return .fine }
        let shortfall = lowest == .min ? .max : -lowest
        return source.isUnallocated || source.allowNegative
            ? .belowZero(shortfall: shortfall, on: sourceBalance.lowOn)
            : .short(shortfall: shortfall, on: sourceBalance.lowOn)
    }

    var blocker: Blocker? {
        if case .gone = sourceState { return .sourceGone }
        guard !rows.isEmpty else { return .noLines }
        if let row = rows.first(where: { $0.issue != nil }), let issue = row.issue {
            return .line(flowId: row.flowId, issue: issue)
        }
        if case .short(let shortfall, _) = sourceState { return .sourceShort(shortfall: shortfall) }
        return nil
    }

    var canMove: Bool { blocker == nil }

    /// One line against its envelope, `balance` the envelope's figures on
    /// the move's day and after (as it is now when `nil`).
    static func row(
        _ line: EnvelopeMoveDraft.Line,
        envelope: FlowView?,
        balance: EnvelopeMoveBalance? = nil,
        currency: Currency
    ) -> Row {
        let typed = line.amount.trimmingCharacters(in: .whitespacesAndNewlines)
        let parsed = typed.isEmpty ? nil : AllocationMoney.amount(typed, currency: currency)
        let amount = parsed.flatMap { $0 > 0 ? $0 : nil }
        guard let envelope else {
            return Row(flowId: line.flowId, envelope: nil, balance: nil, room: nil, amount: amount, issue: .gone)
        }
        let balance = balance ?? EnvelopeMoveBalance(now: envelope)
        let room = headroom(envelope, high: balance.high)
        let issue: Issue?
        if typed.isEmpty {
            issue = .empty
        } else if let amount {
            if let room, amount > room {
                issue = .overCap(excess: subtracting(amount, room))
            } else if balance.high.addingReportingOverflow(amount).overflow {
                // The highest balance is the furthest the line takes it.
                issue = .tooLarge
            } else {
                issue = nil
            }
        } else {
            issue = .notAnAmount
        }
        return Row(flowId: line.flowId, envelope: envelope, balance: balance, room: room, amount: amount, issue: issue)
    }

    /// The room under an envelope's cap, as the core counts it
    /// (`flow::headroom`): the cap less the balance for a cap on the balance,
    /// less what has come in for a cap on the income, since spending frees
    /// no room there. `nil` without a cap.
    ///
    /// `high` is the highest balance from the move's day on, the one a cap
    /// on the balance has to hold at (today's balance when `nil`). What has
    /// come in only grows, so for a cap on the income today's total is
    /// already the highest.
    static func headroom(_ envelope: FlowView, high: Int64? = nil) -> Int64? {
        switch envelope.mode {
        case .unlimited: nil
        case .netCapped(let cap): subtracting(cap, high ?? envelope.balance)
        case .incomeCapped(let cap): subtracting(cap, envelope.incomeTotal ?? 0)
        }
    }

    /// `a + b`, held at the ends of `Int64` instead of trapping on an absurd
    /// amount typed.
    static func adding(_ a: Int64, _ b: Int64) -> Int64 {
        let (sum, overflow) = a.addingReportingOverflow(b)
        guard overflow else { return sum }
        return b > 0 ? .max : .min
    }

    /// `a - b`, held the same way.
    static func subtracting(_ a: Int64, _ b: Int64) -> Int64 {
        let (difference, overflow) = a.subtractingReportingOverflow(b)
        guard overflow else { return difference }
        return b < 0 ? .max : .min
    }
}
