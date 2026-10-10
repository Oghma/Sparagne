import Foundation
import SparagneCore

/// The list operations of the plan's table: the order of the lines is their
/// priority, and every edit sends the whole list (`UpdateAllocationPlan`
/// replaces it), so they are worked out here, apart from the view.
enum AllocationLines {
    /// `lines` with the one at `index` moved `offset` places (−1 up, +1
    /// down), or `nil` when it would leave the list.
    static func moving<Line>(_ lines: [Line], at index: Int, by offset: Int) -> [Line]? {
        let target = index + offset
        guard offset != 0, lines.indices.contains(index), lines.indices.contains(target) else { return nil }
        var moved = lines
        let line = moved.remove(at: index)
        moved.insert(line, at: target)
        return moved
    }

    /// `lines` without the line of `flowId`, or `nil` when that would leave
    /// none: a plan has at least one line, and the core refuses an empty
    /// list.
    static func removing(_ flowId: Uuid, from lines: [AllocationLine]) -> [AllocationLine]? {
        let remaining = lines.filter { $0.flowId != flowId }
        return remaining.isEmpty || remaining.count == lines.count ? nil : remaining
    }
}

/// What Distribuisci sends for a preview: one move per line that gets
/// something, in the plan's order. A period where no line gets anything is
/// skipped instead, since the core takes no execution without a move.
enum AllocationDecision: Equatable {
    case execute([AllocationMove])
    case skip

    init(preview: AllocationPreview) {
        let moves = preview.lines
            .filter { $0.amount > 0 }
            .map { AllocationMove(flowId: $0.flowId, amount: $0.amount) }
        self = moves.isEmpty ? .skip : .execute(moves)
    }
}

/// "Il piano in cifre": what the lines ask for, without a total.
struct AllocationFigures: Equatable {
    var lines = 0
    /// The fixed lines added up, in minor units.
    var fixed: Int64 = 0
    /// The percent lines added up, in basis points.
    var percent: UInt32 = 0
    /// How many lines fill their envelope up to the cap.
    var fillToCap = 0

    init(lines: [AllocationLine]) {
        self.lines = lines.count
        for line in lines {
            switch line.rule {
            case .fixed(let amount): fixed += amount
            case .percent(let basisPoints): percent += basisPoints
            case .fillToCap: fillToCap += 1
            }
        }
    }
}

/// What the plan says that deserves a second look. Never a refusal: the core
/// takes every one of these, and the preview already gives such a line
/// nothing, or less.
enum AllocationWarning: Hashable {
    /// The line's envelope is archived, or gone: it gets nothing.
    case archived(flowId: Uuid)
    /// Al tetto on an envelope without a cap: it gets nothing.
    case noCap(flowId: Uuid)
    /// The percent lines ask for more than the whole total.
    case percentOver(basisPoints: UInt32)
    /// The total ran out before this line had what it asked for.
    case short(flowId: Uuid, amount: Int64, wanted: Int64)
    /// The total is more than Unallocated holds: it would go below zero.
    case overdrawn(unallocatedAfter: Int64)

    /// The warnings of `lines` against the vault's envelopes (archived ones
    /// included, `VaultSnapshot.flows`), in the plan's order, then those of
    /// `preview` when it is the one a decision would send (`due`): a
    /// shortfall worked out on what has come in so far is no news.
    static func of(
        lines: [AllocationLine],
        flows: [FlowView],
        preview: AllocationPreview?,
        due: Bool
    ) -> [AllocationWarning] {
        var warnings: [AllocationWarning] = []
        for line in lines {
            guard let flow = flows.first(where: { $0.id == line.flowId }), !flow.archived else {
                warnings.append(.archived(flowId: line.flowId))
                continue
            }
            if line.rule == .fillToCap, EnvelopeCapKind.cap(of: flow.mode) == nil {
                warnings.append(.noCap(flowId: line.flowId))
            }
        }
        let percent = AllocationFigures(lines: lines).percent
        if percent > AllocationPercent.full {
            warnings.append(.percentOver(basisPoints: percent))
        }
        if due, let preview {
            for line in preview.lines where line.status == .short {
                warnings.append(.short(flowId: line.flowId, amount: line.amount, wanted: line.wanted))
            }
            if preview.unallocatedAfter < 0 {
                warnings.append(.overdrawn(unallocatedAfter: preview.unallocatedAfter))
            }
        }
        return warnings
    }
}

/// The preview line of each of `lines`, by position, when `preview` was
/// worked out on these very lines; `nil` for a line it does not cover (a
/// preview still on its way after an edit).
enum AllocationPreviewMatch {
    static func lines(_ lines: [AllocationLine], in preview: AllocationPreview?) -> [PreviewLine?] {
        lines.enumerated().map { index, line in
            guard let preview, preview.lines.indices.contains(index) else { return nil }
            let candidate = preview.lines[index]
            return candidate.flowId == line.flowId && candidate.rule == line.rule ? candidate : nil
        }
    }
}

/// Distribuisci just went through: what its toast says and what Annulla
/// reopens. The command is applied already, so undoing it is a command of its
/// own (`ReopenAllocation`), unlike the void's toast, which holds the write
/// back.
struct AllocationUndo: Identifiable, Equatable, Sendable {
    let id = UUID()
    let vaultId: Uuid
    let planId: Uuid
    let periodDate: NaiveDate
    /// What went into the envelopes, in minor units.
    let distributed: Int64
    let startedAt: Date
    let duration: Duration

    var seconds: Double {
        let components = duration.components
        return Double(components.seconds) + Double(components.attoseconds) * 1e-18
    }

    var deadline: Date { startedAt.addingTimeInterval(seconds) }

    /// `0...1`, how much of the window has elapsed.
    func progress(at now: Date) -> Double {
        guard seconds > 0 else { return 1 }
        return min(max(now.timeIntervalSince(startedAt) / seconds, 0), 1)
    }
}
