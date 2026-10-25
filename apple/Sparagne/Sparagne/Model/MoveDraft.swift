import Foundation
import SparagneCore

/// The "Move between flows" draft, or between wallets: the source, the lines and their amounts, a day and a note.
struct MoveDraft: Equatable {
    // Each line is an ordinary transfer between flows or between wallets, the lot one batch;
    // no allocation period is decided.

    /// One flow or wallet the money goes to, with its amount as typed.
    struct Line: Identifiable, Equatable {
        let targetId: Uuid
        var amount = ""

        var id: Uuid { targetId }
    }

    /// Flows or wallets: the source and every line are of this side.
    let side: TargetKind
    /// Where the money leaves from: Unallocated when the sheet opens on the
    /// flows.
    private(set) var sourceId: Uuid
    /// In the order they were added. Never the source, never one target
    /// twice.
    private(set) var lines: [Line] = []
    var day: NaiveDate
    var note = ""

    init(side: TargetKind = .flow, sourceId: Uuid, day: NaiveDate) {
        self.side = side
        self.sourceId = sourceId
        self.day = day
    }

    /// Another source, and its line dropped: a flow or a wallet does not
    /// move money to itself.
    mutating func setSource(_ targetId: Uuid) {
        sourceId = targetId
        lines.removeAll { $0.targetId == targetId }
    }

    /// A line for `targetId`, unless it is the source or has one already.
    mutating func add(_ targetId: Uuid) {
        guard targetId != sourceId, !lines.contains(where: { $0.targetId == targetId }) else { return }
        lines.append(Line(targetId: targetId))
    }

    mutating func remove(_ targetId: Uuid) {
        lines.removeAll { $0.targetId == targetId }
    }

    mutating func setAmount(_ text: String, for targetId: Uuid) {
        guard let index = lines.firstIndex(where: { $0.targetId == targetId }) else { return }
        lines[index].amount = text
    }

    /// "To cap": as much as the flow can still take.
    mutating func fill(_ targetId: Uuid, room: Int64) {
        guard room > 0 else { return }
        setAmount(LedgerMoney.bare(room), for: targetId)
    }

    /// The draft against `targets`, the vault's flows or wallets as they are
    /// now, and `balances`, what each held on the draft's day and afterwards
    /// (`AppStore.moveBalances(_:on:)`). A target missing from `balances` is
    /// taken as it is now, which is right for a move dated today with
    /// nothing dated after it.
    ///
    /// The core adds and subtracts balances without overflow checks, so a
    /// line that would take the total, or the source once the total has left
    /// it, past what an `Int64` holds is too large, and the total leaves it
    /// out.
    func check(
        targets: [MoveTarget],
        currency: Currency,
        balances: [Uuid: MoveBalance] = [:]
    ) -> MoveCheck {
        let byId = Dictionary(targets.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let balance = { (target: MoveTarget) in balances[target.id] ?? MoveBalance(now: target) }
        let source = byId[sourceId]
        let sourceBalance = source.map(balance)
        var rows = lines.map { line in
            let target = byId[line.targetId]
            return MoveCheck.row(line, target: target, balance: target.map(balance), currency: currency)
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
        return MoveCheck(rows: rows, source: source, sourceBalance: sourceBalance, total: total)
    }

    /// One transfer per line, out of the source, on `occurredAt`, carrying
    /// the note when there is one: between flows or between wallets, by
    /// `side`. None unless the check against `targets` and `balances` lets
    /// the draft move.
    func commands(
        targets: [MoveTarget],
        currency: Currency,
        balances: [Uuid: MoveBalance] = [:],
        occurredAt: OffsetDateTime
    ) -> [Command] {
        let check = check(targets: targets, currency: currency, balances: balances)
        guard check.canMove else { return [] }
        let trimmed = note.trimmingCharacters(in: .whitespacesAndNewlines)
        let note = trimmed.isEmpty ? nil : trimmed
        return check.rows.compactMap { row in
            row.amount.map { amount in
                switch side {
                case .flow:
                    .transferFlow(amount: amount, fromFlowId: sourceId, toFlowId: row.targetId, note: note, occurredAt: occurredAt)
                case .wallet:
                    .transferWallet(amount: amount, fromWalletId: sourceId, toWalletId: row.targetId, note: note, occurredAt: occurredAt)
                }
            }
        }
    }
}

/// A flow or a wallet as a move sees it: its balance, a flow's cap, and whether it may go below zero.
nonisolated struct MoveTarget: Equatable, Sendable {
    let id: Uuid
    /// As the window shows it, Unallocated localized.
    let name: String
    let balance: Int64
    /// `.unlimited` for a wallet: the core puts no cap on one.
    let mode: FlowMode
    let incomeTotal: Int64?
    /// Below zero is then only pointed out: Unallocated, a flow set to
    /// allow it, and every wallet, which the core lets go below zero.
    let mayGoNegative: Bool
}

nonisolated extension MoveTarget {
    init(_ flow: FlowView) {
        self.init(
            id: flow.id,
            name: flow.isUnallocated ? NameBook.unallocatedLabel : flow.name,
            balance: flow.balance,
            mode: flow.mode,
            incomeTotal: flow.incomeTotal,
            mayGoNegative: flow.isUnallocated || flow.allowNegative
        )
    }

    init(_ wallet: WalletView) {
        self.init(id: wallet.id, name: wallet.name, balance: wallet.balance, mode: .unlimited, incomeTotal: nil, mayGoNegative: true)
    }
}

/// A flow's or a wallet's balance at the end of the move's day, and its lowest and highest from then on.
struct MoveBalance: Equatable {
    // A past transfer moves every later balance: the source gives only what it holds at its
    // lowest, and a balance cap takes only what fits at its highest.

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

extension MoveBalance {
    /// The target as it is now, all three figures its balance: what a move
    /// dated today sees when nothing is dated after it, and what the sheet
    /// shows until the core has said otherwise.
    init(now target: MoveTarget) {
        self.init(balance: target.balance, low: target.balance, lowOn: nil, high: target.balance)
    }

    /// The core's figures, the instant of the lowest point read as a day in
    /// `timeZone`.
    init(_ range: BalanceRange, timeZone: TimeZone = .current) {
        self.init(
            balance: range.balance,
            low: range.low,
            lowOn: range.lowAt.flatMap(CoreDate.date).map { CoreDate.day($0, timeZone: timeZone) },
            high: range.high
        )
    }
}

/// What a draft would do to the flows or wallets on its day and after, and what stops the move.
struct MoveCheck: Equatable {
    // The core's rules (`Flow::apply_leg_change`), checked here so the sheet names the wrong
    // line, on every day from the move's: a past move cannot spend money that came later.
    // Unallocated and a wallet below zero are only pointed out.

    struct Row: Identifiable, Equatable {
        let targetId: Uuid
        /// `nil` once the flow or wallet is gone: archived or deleted
        /// elsewhere while the sheet was open.
        let target: MoveTarget?
        /// On the move's day and after; `nil` once the target is gone.
        let balance: MoveBalance?
        /// What it can still take under its cap, on every day from the
        /// move's on; `nil` without a cap, which a wallet never has.
        let room: Int64?
        /// `nil` while nothing usable is typed.
        let amount: Int64?
        var issue: Issue?

        var id: Uuid { targetId }

        /// Its balance on the move's day once the move is done.
        var after: Int64? {
            balance.map { MoveCheck.adding($0.balance, amount ?? 0) }
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
        /// The flow or wallet is no longer there.
        case gone
    }

    /// `on` is the day the source is furthest below zero, `nil` when that is the move's own day.
    enum SourceState: Equatable {
        case fine
        /// Below zero by `shortfall`, which the source allows: Unallocated
        /// and a wallet always do, another flow when it is set to.
        case belowZero(shortfall: Int64, on: NaiveDate? = nil)
        /// Below zero by `shortfall`, which the source does not allow.
        case short(shortfall: Int64, on: NaiveDate? = nil)
        /// The source is no longer there.
        case gone
    }

    /// What stops the move, the first of it: what the footer says.
    enum Blocker: Equatable {
        case noLines
        case line(targetId: Uuid, issue: Issue)
        case sourceShort(shortfall: Int64)
        case sourceGone
    }

    let rows: [Row]
    let source: MoveTarget?
    /// The source on the move's day and after; `nil` once it is gone.
    let sourceBalance: MoveBalance?
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
        return source.mayGoNegative
            ? .belowZero(shortfall: shortfall, on: sourceBalance.lowOn)
            : .short(shortfall: shortfall, on: sourceBalance.lowOn)
    }

    var blocker: Blocker? {
        if case .gone = sourceState { return .sourceGone }
        guard !rows.isEmpty else { return .noLines }
        if let row = rows.first(where: { $0.issue != nil }), let issue = row.issue {
            return .line(targetId: row.targetId, issue: issue)
        }
        if case .short(let shortfall, _) = sourceState { return .sourceShort(shortfall: shortfall) }
        return nil
    }

    var canMove: Bool { blocker == nil }

    /// One line against its flow or wallet, `balance` the target's figures
    /// on the move's day and after (as it is now when `nil`).
    static func row(
        _ line: MoveDraft.Line,
        target: MoveTarget?,
        balance: MoveBalance? = nil,
        currency: Currency
    ) -> Row {
        let typed = line.amount.trimmingCharacters(in: .whitespacesAndNewlines)
        let parsed = typed.isEmpty ? nil : AllocationMoney.amount(typed, currency: currency)
        let amount = parsed.flatMap { $0 > 0 ? $0 : nil }
        guard let target else {
            return Row(targetId: line.targetId, target: nil, balance: nil, room: nil, amount: amount, issue: .gone)
        }
        let balance = balance ?? MoveBalance(now: target)
        let room = headroom(target, high: balance.high)
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
        return Row(targetId: line.targetId, target: target, balance: balance, room: room, amount: amount, issue: issue)
    }

    /// The room under a flow's cap, as the core counts it
    /// (`flow::headroom`): the cap less the balance for a cap on the balance,
    /// less what has come in for a cap on the income, since spending frees
    /// no room there. `nil` without a cap.
    ///
    /// `high` is the highest balance from the move's day on, the one a cap
    /// on the balance has to hold at (today's balance when `nil`). What has
    /// come in only grows, so for a cap on the income today's total is
    /// already the highest.
    static func headroom(_ target: MoveTarget, high: Int64? = nil) -> Int64? {
        switch target.mode {
        case .unlimited: nil
        case .netCapped(let cap): subtracting(cap, high ?? target.balance)
        case .incomeCapped(let cap): subtracting(cap, target.incomeTotal ?? 0)
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
