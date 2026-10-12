import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The "Move between envelopes" draft and its check: what a line reads as,
/// which line stops the move and why, what the source is left with, and the
/// transfers it sends. Plain structs against made-up envelopes, so none of
/// this needs a vault.
struct EnvelopeMoveTests {
    private static let unallocatedId: Uuid = "flow-unallocated"
    private static let rentId: Uuid = "flow-affitto"
    private static let holidayId: Uuid = "flow-vacanze"
    private static let carId: Uuid = "flow-auto"
    private static let giftsId: Uuid = "flow-regali"

    private static func envelope(
        _ id: Uuid,
        _ name: String,
        balance: Int64,
        mode: FlowMode = .unlimited,
        incomeTotal: Int64? = nil,
        allowNegative: Bool = false,
        unallocated: Bool = false
    ) -> FlowView {
        FlowView(
            id: id,
            name: name,
            balance: balance,
            mode: mode,
            incomeTotal: incomeTotal,
            allowNegative: allowNegative,
            archived: false,
            isUnallocated: unallocated
        )
    }

    /// Unallocated 5.000,00; Affitto 200,00 with no cap; Vacanze 2.672,50
    /// under a cap of 3.000,00 on its balance; Auto 100,00 under a cap of
    /// 1.000,00 on its income, 900,00 of it already in; Regali 50,00, which
    /// may go below zero.
    private static let flows = [
        envelope(unallocatedId, "unallocated", balance: 500_000, unallocated: true),
        envelope(rentId, "Affitto", balance: 20_000),
        envelope(holidayId, "Vacanze", balance: 267_250, mode: .netCapped(cap: 300_000)),
        envelope(carId, "Auto", balance: 10_000, mode: .incomeCapped(cap: 100_000), incomeTotal: 90_000),
        envelope(giftsId, "Regali", balance: 5_000, allowNegative: true),
    ]

    private static func draft(from source: Uuid = unallocatedId, _ lines: [(Uuid, String)]) -> EnvelopeMoveDraft {
        var draft = EnvelopeMoveDraft(sourceId: source, day: "2026-10-11")
        for (flowId, amount) in lines {
            draft.add(flowId)
            draft.setAmount(amount, for: flowId)
        }
        return draft
    }

    private static func check(_ draft: EnvelopeMoveDraft) -> EnvelopeMoveCheck {
        draft.check(flows: flows, currency: .eur)
    }

    // MARK: - The lines

    @Test("An envelope gets one line, never the source's; a new source drops its own line")
    func lines() {
        var draft = EnvelopeMoveDraft(sourceId: Self.unallocatedId, day: "2026-10-11")
        draft.add(Self.unallocatedId)
        draft.add(Self.rentId)
        draft.add(Self.rentId)
        draft.add(Self.holidayId)
        #expect(draft.lines.map(\.flowId) == [Self.rentId, Self.holidayId])

        draft.setSource(Self.rentId)
        #expect(draft.sourceId == Self.rentId)
        #expect(draft.lines.map(\.flowId) == [Self.holidayId])

        draft.remove(Self.holidayId)
        #expect(draft.lines.isEmpty)
    }

    @Test("The window's amounts read as the plan's cells read them, thousands dots included")
    func amounts() {
        let check = Self.check(Self.draft([(Self.rentId, "1.000"), (Self.holidayId, "250,5")]))
        #expect(check.rows.map(\.amount) == [100_000, 25_050])
        #expect(check.rows.allSatisfy { $0.issue == nil })
        #expect(check.total == 125_050)
        #expect(check.rows.map(\.after) == [120_000, 292_300])
        #expect(check.canMove)
    }

    @Test("Nothing to move, an empty amount and something that is no amount above zero each stop the move")
    func incomplete() {
        #expect(Self.check(Self.draft([])).blocker == .noLines)

        let empty = Self.check(Self.draft([(Self.rentId, "100"), (Self.holidayId, "  ")]))
        #expect(empty.rows[1].issue == .empty)
        #expect(empty.blocker == .line(flowId: Self.holidayId, issue: .empty))
        #expect(empty.total == 10_000)

        for garbage in ["abc", "0", "-5", "1,2,3"] {
            let check = Self.check(Self.draft([(Self.rentId, garbage)]))
            #expect(check.rows[0].issue == .notAnAmount, "\(garbage)")
            #expect(check.rows[0].amount == nil, "\(garbage)")
            #expect(!check.canMove, "\(garbage)")
        }
    }

    // MARK: - Caps

    @Test("A cap on the balance leaves the cap less the balance; more than that is over by the difference")
    func balanceCap() {
        let fits = Self.check(Self.draft([(Self.holidayId, "327,50")]))
        #expect(fits.rows[0].room == 32_750)
        #expect(fits.rows[0].issue == nil)
        #expect(fits.rows[0].after == 300_000)

        let over = Self.check(Self.draft([(Self.holidayId, "400")]))
        #expect(over.rows[0].issue == .overCap(excess: 7_250))
        #expect(over.blocker == .line(flowId: Self.holidayId, issue: .overCap(excess: 7_250)))
        #expect(!over.canMove)
    }

    @Test("A cap on the income leaves the cap less what came in, whatever the balance")
    func incomeCap() {
        let check = Self.check(Self.draft([(Self.carId, "150")]))
        #expect(check.rows[0].room == 10_000)
        #expect(check.rows[0].issue == .overCap(excess: 5_000))
    }

    @Test("To cap types the room left, which then fits exactly")
    func fill() {
        var draft = Self.draft([(Self.holidayId, "400")])
        draft.fill(Self.holidayId, room: 32_750)
        #expect(draft.lines[0].amount == LedgerMoney.bare(32_750))
        let check = Self.check(draft)
        #expect(check.rows[0].amount == 32_750)
        #expect(check.canMove)

        // No room, nothing typed over what is there.
        draft.fill(Self.holidayId, room: 0)
        #expect(draft.lines[0].amount == LedgerMoney.bare(32_750))
    }

    @Test("No cap, no room: any amount fits")
    func noCap() {
        let check = Self.check(Self.draft([(Self.rentId, "1.000.000")]))
        #expect(check.rows[0].room == nil)
        #expect(check.rows[0].issue == nil)
    }

    // MARK: - The source

    @Test("Unallocated below zero is only pointed out, and the move goes")
    func unallocatedBelowZero() {
        let check = Self.check(Self.draft([(Self.rentId, "4.000"), (Self.giftsId, "1.400")]))
        #expect(check.total == 540_000)
        #expect(check.sourceAfter == -40_000)
        #expect(check.sourceState == .belowZero(shortfall: 40_000))
        #expect(check.canMove)
    }

    @Test("An envelope that may not go below zero stops the move; one that may is only pointed out")
    func envelopeBelowZero() {
        let short = Self.check(Self.draft(from: Self.rentId, [(Self.holidayId, "100"), (Self.carId, "150,00")]))
        // Auto's line is over its cap too, and a line comes first.
        #expect(short.blocker == .line(flowId: Self.carId, issue: .overCap(excess: 5_000)))

        let onlyShort = Self.check(Self.draft(from: Self.rentId, [(Self.holidayId, "250")]))
        #expect(onlyShort.sourceAfter == -5_000)
        #expect(onlyShort.sourceState == .short(shortfall: 5_000))
        #expect(onlyShort.blocker == .sourceShort(shortfall: 5_000))

        let allowed = Self.check(Self.draft(from: Self.giftsId, [(Self.rentId, "80")]))
        #expect(allowed.sourceState == .belowZero(shortfall: 3_000))
        #expect(allowed.canMove)
    }

    @Test("An envelope gone since the sheet opened stops the move, as a line or as the source")
    func gone() {
        let others = Self.flows.filter { $0.id != Self.holidayId }
        let line = Self.draft([(Self.holidayId, "100")]).check(flows: others, currency: .eur)
        #expect(line.rows[0].issue == .gone)
        #expect(line.blocker == .line(flowId: Self.holidayId, issue: .gone))
        #expect(line.rows[0].after == nil)
        #expect(!line.canMove)

        let withoutSource = Self.flows.filter { $0.id != Self.unallocatedId }
        let source = Self.draft([(Self.rentId, "100")]).check(flows: withoutSource, currency: .eur)
        #expect(source.sourceState == .gone)
        #expect(source.blocker == .sourceGone)
    }

    @Test("A line that would take the total past an Int64 is too large, and stops the move")
    func tooLargeTotal() {
        // Each fits in an Int64 of cents; the two together do not, and the
        // core would wrap the source's balance rather than refuse.
        let huge = "50.000.000.000.000.000"
        let check = Self.check(Self.draft([(Self.rentId, huge), (Self.giftsId, huge)]))
        #expect(check.rows.map(\.amount) == [5_000_000_000_000_000_000, 5_000_000_000_000_000_000])
        #expect(check.rows.map(\.issue) == [nil, .tooLarge])
        #expect(check.total == 5_000_000_000_000_000_000)
        #expect(check.blocker == .line(flowId: Self.giftsId, issue: .tooLarge))
        let sent = Self.draft([(Self.rentId, huge), (Self.giftsId, huge)])
            .commands(flows: Self.flows, currency: .eur, occurredAt: "2026-10-11T09:30:00+02:00")
        #expect(sent.isEmpty)
    }

    @Test("A line that would take its envelope's balance past an Int64 is too large")
    func tooLargeBalance() {
        let full = Self.envelope(Self.rentId, "Affitto", balance: .max - 10)
        let flows = Self.flows.map { $0.id == Self.rentId ? full : $0 }
        let check = Self.draft([(Self.rentId, "1")]).check(flows: flows, currency: .eur)
        #expect(check.rows[0].issue == .tooLarge)
        #expect(!check.canMove)
    }

    @Test("A total the source cannot give without passing the bottom of an Int64 is too large")
    func tooLargeForTheSource() {
        let deep = Self.envelope(Self.unallocatedId, "unallocated", balance: .min + 10, unallocated: true)
        let flows = Self.flows.map { $0.id == Self.unallocatedId ? deep : $0 }
        let check = Self.draft([(Self.rentId, "1")]).check(flows: flows, currency: .eur)
        #expect(check.rows[0].issue == .tooLarge)
        #expect(check.total == 0)
        #expect(!check.canMove)
    }

    // MARK: - The transfers

    @Test("One transfer per line, out of the source, on the time given, with the note trimmed or none")
    func commands() {
        var draft = Self.draft([(Self.rentId, "1.000"), (Self.holidayId, "300")])
        draft.note = "  riparto straordinario "
        let at: OffsetDateTime = "2026-10-11T09:30:00+02:00"
        let commands = draft.commands(flows: Self.flows, currency: .eur, occurredAt: at)
        #expect(commands == [
            .transferFlow(amount: 100_000, fromFlowId: Self.unallocatedId, toFlowId: Self.rentId, note: "riparto straordinario", occurredAt: at),
            .transferFlow(amount: 30_000, fromFlowId: Self.unallocatedId, toFlowId: Self.holidayId, note: "riparto straordinario", occurredAt: at),
        ])

        draft.note = "   "
        let unnoted = draft.commands(flows: Self.flows, currency: .eur, occurredAt: at)
        #expect(unnoted.allSatisfy {
            if case .transferFlow(_, _, _, let note, _) = $0 { note == nil } else { false }
        })
    }

    @Test("A draft that cannot move sends nothing")
    func nothingSent() {
        let over = Self.draft([(Self.rentId, "100"), (Self.holidayId, "400")])
        #expect(over.commands(flows: Self.flows, currency: .eur, occurredAt: "2026-10-11T09:30:00+02:00").isEmpty)
        #expect(Self.draft([]).commands(flows: Self.flows, currency: .eur, occurredAt: "2026-10-11T09:30:00+02:00").isEmpty)
    }
}
