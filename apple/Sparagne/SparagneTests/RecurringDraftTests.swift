import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The inspector's fields (`RecurringDraft`): what a new template is
/// created with, and what an edit sends.
struct RecurringDraftTests {
    private static let wallet: Uuid = "wallet-conto"
    private static let otherWallet: Uuid = "wallet-contanti"
    private static let flow: Uuid = "flow-casa"
    private static let otherFlow: Uuid = "flow-cash"

    /// "mutuo": 780,00 a month on the 1st, from 1 Nov 2025, from Conto into
    /// Casa.
    private static let rent = RecurringFixture.template(
        "rent",
        amount: 78_000,
        walletId: wallet,
        flowId: flow,
        category: "Casa",
        note: "mutuo",
        schedule: RecurringFixture.schedule(.monthly(day: 1), from: "2025-11-01")
    )

    /// No wallet, no envelope.
    private static let netflix = RecurringFixture.template(
        "netflix",
        amount: 1_299,
        category: "Abbonamenti",
        note: "Netflix",
        schedule: RecurringFixture.schedule(.monthly(day: 12), from: "2026-01-12")
    )

    private func patch(_ edit: (inout RecurringDraft) -> Void, of template: RecurringView = rent) -> RecurringPatch {
        var draft = RecurringDraft(template: template)
        edit(&draft)
        return draft.patch(against: template, currency: .eur)
    }

    // MARK: - Unchanged

    @Test("An untouched draft sends nothing and has nothing to save")
    func unchanged() {
        let draft = RecurringDraft(template: Self.rent)
        #expect(draft.patch(against: Self.rent, currency: .eur).isEmpty)
        #expect(!draft.isDirty(against: Self.rent, currency: .eur))
        #expect(draft.isValid(currency: .eur))
        #expect(draft.schedule == Self.rent.schedule)
        #expect(draft.amountText == "780,00")
    }

    @Test("Every cadence survives the round trip unchanged")
    func unchangedEveryCadence() {
        let schedules = [
            RecurringFixture.schedule(.daily, every: 3),
            RecurringFixture.schedule(.weekly(weekday: 6), every: 2),
            RecurringFixture.schedule(.monthly(day: 31)),
            RecurringFixture.schedule(.yearly(month: 3, day: 15), until: "2030-12-31"),
        ]
        for schedule in schedules {
            let template = RecurringFixture.template(schedule: schedule)
            let draft = RecurringDraft(template: template)
            #expect(draft.schedule == schedule)
            #expect(!draft.isDirty(against: template, currency: .eur))
        }
    }

    @Test("A duplicate keeps the fields, runs, and starts today")
    func duplicate() {
        let draft = RecurringDraft(duplicating: Self.rent, today: "2026-10-08")
        #expect(draft.amountText == "780,00")
        #expect(draft.walletId == Self.wallet)
        #expect(draft.flowId == Self.flow)
        #expect(draft.category == "Casa")
        #expect(draft.note == "mutuo")
        #expect(draft.schedule == RecurringFixture.schedule(.monthly(day: 1), from: "2026-10-08"))
        #expect(draft.isValid(currency: .eur))
    }

    @Test("A duplicate of a paused template with an end date behind today has none")
    func duplicateDropsAPastEnd() {
        let old = RecurringFixture.template(
            schedule: RecurringFixture.schedule(.weekly(weekday: 3), from: "2025-01-01", until: "2025-06-01"),
            enabled: false
        )
        let draft = RecurringDraft(duplicating: old, today: "2026-10-08")
        #expect(draft.enabled)
        #expect(draft.schedule == RecurringFixture.schedule(.weekly(weekday: 3), from: "2026-10-08"))
        let ahead = RecurringFixture.template(
            schedule: RecurringFixture.schedule(.daily, from: "2025-01-01", until: "2027-01-01")
        )
        #expect(RecurringDraft(duplicating: ahead, today: "2026-10-08").schedule.endDate == "2027-01-01")
    }

    @Test("Spaces around the category and the note change nothing")
    func whitespaceIsNotAChange() {
        #expect(patch { $0.category = " Casa "; $0.note = "mutuo " }.isEmpty)
    }

    // MARK: - One field at a time

    @Test("The amount alone")
    func amount() {
        #expect(patch { $0.amountText = "800" } == RecurringPatch(amount: 80_000))
        #expect(patch { $0.amountText = "780.00" }.isEmpty)
    }

    @Test("The wallet alone")
    func wallet() {
        #expect(patch { $0.setWallet(Self.otherWallet, template: Self.rent) } == RecurringPatch(walletId: Self.otherWallet))
    }

    @Test("The envelope alone")
    func envelope() {
        #expect(patch { $0.setFlow(Self.otherFlow, template: Self.rent) } == RecurringPatch(flowId: Self.otherFlow))
    }

    @Test("The category alone, and cleared")
    func category() {
        #expect(patch { $0.category = "Mutuo" } == RecurringPatch(category: "Mutuo"))
        #expect(patch { $0.category = "" } == RecurringPatch(category: ""))
    }

    @Test("The note alone, and cleared")
    func note() {
        #expect(patch { $0.note = "mutuo casa" } == RecurringPatch(note: "mutuo casa"))
        #expect(patch { $0.note = "  " } == RecurringPatch(note: ""))
    }

    @Test("The schedule goes whole, whichever part of it changed")
    func schedule() {
        let day = patch { $0.monthDay = 5 }
        #expect(day == RecurringPatch(schedule: RecurringFixture.schedule(.monthly(day: 5), from: "2025-11-01")))

        let interval = patch { $0.interval = 2 }
        #expect(interval == RecurringPatch(schedule: RecurringFixture.schedule(.monthly(day: 1), every: 2, from: "2025-11-01")))

        let start = patch { $0.startDate = "2026-01-01" }
        #expect(start == RecurringPatch(schedule: RecurringFixture.schedule(.monthly(day: 1), from: "2026-01-01")))

        let end = patch {
            $0.hasEndDate = true
            $0.endDate = "2045-10-01"
        }
        #expect(end == RecurringPatch(
            schedule: RecurringFixture.schedule(.monthly(day: 1), from: "2025-11-01", until: "2045-10-01")
        ))

        let cadence = patch { $0.cadence = .weekly; $0.weekday = 3 }
        #expect(cadence == RecurringPatch(schedule: RecurringFixture.schedule(.weekly(weekday: 3), from: "2025-11-01")))
    }

    @Test("Enabled alone")
    func enabled() {
        #expect(patch { $0.enabled = false } == RecurringPatch(enabled: false))
    }

    @Test("Several fields together, each once")
    func several() {
        let several = patch {
            $0.amountText = "790"
            $0.note = "mutuo BPER"
            $0.monthDay = 2
        }
        #expect(several == RecurringPatch(
            amount: 79_000,
            note: "mutuo BPER",
            schedule: RecurringFixture.schedule(.monthly(day: 2), from: "2025-11-01")
        ))
    }

    // MARK: - What the patch cannot say

    @Test("The kind is never in the patch")
    func kindNeverSent() {
        var draft = RecurringDraft(template: Self.rent)
        draft.kind = .income
        #expect(draft.patch(against: Self.rent, currency: .eur).isEmpty)
        #expect(!draft.isDirty(against: Self.rent, currency: .eur))
    }

    @Test("A template with a wallet and an envelope cannot be set back to none")
    func noneIsRefused() {
        #expect(!RecurringDraft.mayClearWallet(Self.rent))
        #expect(!RecurringDraft.mayClearFlow(Self.rent))

        var draft = RecurringDraft(template: Self.rent)
        draft.setWallet(nil, template: Self.rent)
        draft.setFlow(nil, template: Self.rent)
        #expect(draft.walletId == Self.wallet)
        #expect(draft.flowId == Self.flow)

        // Even forced into the fields, none is not sent.
        draft.walletId = nil
        draft.flowId = nil
        #expect(draft.patch(against: Self.rent, currency: .eur).isEmpty)
    }

    @Test("A template without a wallet or an envelope may keep none, or get one")
    func noneIsKeptWhereItWas() {
        #expect(RecurringDraft.mayClearWallet(Self.netflix))
        #expect(RecurringDraft.mayClearFlow(Self.netflix))
        #expect(RecurringDraft.mayClearWallet(nil))

        var draft = RecurringDraft(template: Self.netflix)
        draft.setWallet(Self.wallet, template: Self.netflix)
        #expect(draft.patch(against: Self.netflix, currency: .eur) == RecurringPatch(walletId: Self.wallet))
        draft.setWallet(nil, template: Self.netflix)
        #expect(draft.patch(against: Self.netflix, currency: .eur).isEmpty)
    }

    // MARK: - Validity

    @Test("An amount of zero, below zero, or not a number is not valid")
    func invalidAmounts() {
        for text in ["0", "0,00", "-5", "", "abc", "1,234"] {
            var draft = RecurringDraft(template: Self.rent)
            draft.amountText = text
            #expect(!draft.isValid(currency: .eur), "\(text)")
            // Left out of the patch, but still something to save or undo.
            #expect(draft.patch(against: Self.rent, currency: .eur).amount == nil)
            #expect(draft.isDirty(against: Self.rent, currency: .eur), "\(text)")
        }
    }

    @Test("An interval of zero is not valid")
    func zeroInterval() {
        var draft = RecurringDraft(template: Self.rent)
        draft.interval = 0
        #expect(!draft.isValid(currency: .eur))
    }

    @Test("Days out of range, and an end before the start, are not valid")
    func invalidSchedules() {
        var draft = RecurringDraft(template: Self.rent)
        draft.monthDay = 32
        #expect(!draft.isValid(currency: .eur))

        draft = RecurringDraft(template: Self.rent)
        draft.cadence = .weekly
        draft.weekday = 8
        #expect(!draft.isValid(currency: .eur))

        draft = RecurringDraft(template: Self.rent)
        draft.cadence = .yearly
        draft.yearMonth = 13
        #expect(!draft.isValid(currency: .eur))

        draft = RecurringDraft(template: Self.rent)
        draft.hasEndDate = true
        draft.endDate = "2025-10-31"
        #expect(!draft.isValid(currency: .eur))
        // Turned back to Mai, the stale end date does not matter.
        draft.hasEndDate = false
        #expect(draft.isValid(currency: .eur))
    }

    @Test("Whatever isValid accepts, the core accepts too")
    func validMeansTheCoreAgrees() throws {
        var draft = RecurringDraft(today: "2026-10-07")
        draft.amountText = "12,99"
        for cadence in RecurringDraft.Cadence.allCases {
            draft.cadence = cadence
            #expect(draft.isValid(currency: .eur))
            #expect(try !RecurringFixture.core(draft.schedule, "2026-10-07", 1).isEmpty)
        }
    }

    // MARK: - The schedule of each cadence

    @Test("Each cadence builds its own frequency, with the day fields of that cadence")
    func scheduleOfEachCadence() {
        var draft = RecurringDraft(today: "2026-10-07")
        draft.interval = 2
        draft.weekday = 6
        draft.monthDay = 27
        draft.yearMonth = 3
        draft.yearDay = 15

        draft.cadence = .daily
        #expect(draft.schedule == RecurringFixture.schedule(.daily, every: 2, from: "2026-10-07"))
        draft.cadence = .weekly
        #expect(draft.schedule == RecurringFixture.schedule(.weekly(weekday: 6), every: 2, from: "2026-10-07"))
        draft.cadence = .monthly
        #expect(draft.schedule == RecurringFixture.schedule(.monthly(day: 27), every: 2, from: "2026-10-07"))
        draft.cadence = .yearly
        #expect(draft.schedule == RecurringFixture.schedule(.yearly(month: 3, day: 15), every: 2, from: "2026-10-07"))

        draft.hasEndDate = true
        draft.endDate = "2027-12-31"
        #expect(draft.schedule.endDate == "2027-12-31")
    }

    // MARK: - A new template

    @Test("A new template is a monthly expense from today, on today's day")
    func newTemplateDefaults() {
        let draft = RecurringDraft(today: "2026-10-07")
        #expect(draft.kind == .expense)
        #expect(draft.cadence == .monthly)
        #expect(draft.schedule == RecurringFixture.schedule(.monthly(day: 7), from: "2026-10-07"))
        #expect(draft.weekday == 3)
        #expect(draft.walletId == nil)
        #expect(draft.flowId == nil)
        #expect(draft.enabled)
        // No amount yet.
        #expect(!draft.isValid(currency: .eur))
        #expect(draft.creation(currency: .eur) == nil)
    }

    @Test("Creating sends what was typed, blank category and note as none")
    func creation() {
        var draft = RecurringDraft(today: "2026-10-07")
        draft.kind = .income
        draft.amountText = "2400,00"
        draft.walletId = Self.wallet
        draft.note = " Stipendio "
        draft.monthDay = 27

        #expect(draft.creation(currency: .eur) == RecurringDraft.Creation(
            kind: .income,
            amount: 240_000,
            walletId: Self.wallet,
            flowId: nil,
            category: nil,
            note: "Stipendio",
            schedule: RecurringFixture.schedule(.monthly(day: 27), from: "2026-10-07"),
            owner: nil
        ))
    }

    // MARK: - The owner

    @Test("A new template belongs to whoever the inspector starts it with, and says so when created")
    func ownerOnCreation() {
        var draft = RecurringDraft(today: "2026-10-07", owner: "matteo")
        draft.amountText = "780"
        #expect(draft.owner == "matteo")
        #expect(draft.creation(currency: .eur)?.owner == "matteo")

        draft.owner = "elisa"
        #expect(draft.creation(currency: .eur)?.owner == "elisa")

        // Blank is the core's own default, the author: nothing to send.
        draft.owner = " "
        #expect(draft.creation(currency: .eur)?.owner == nil)
    }

    @Test("An edit sends the owner only when it changed, and never a blank")
    func ownerInThePatch() {
        #expect(RecurringDraft(template: Self.rent).owner == "matteo")
        #expect(patch { $0.owner = "elisa" } == RecurringPatch(owner: "elisa"))
        #expect(patch { $0.owner = "matteo" }.isEmpty)
        #expect(patch { $0.owner = "" }.isEmpty)
        #expect(RecurringDraft(template: Self.rent).isDirty(against: Self.elisasRent, currency: .eur))
    }

    @Test("A duplicate keeps its template's owner, and creates the copy for them")
    func duplicateKeepsTheOwner() {
        let draft = RecurringDraft(duplicating: Self.elisasRent, today: "2026-10-08")
        #expect(draft.owner == "elisa")
        #expect(draft.creation(currency: .eur)?.owner == "elisa")
    }

    /// `rent`, owned by elisa.
    private static let elisasRent = RecurringFixture.template(
        "rent",
        amount: 78_000,
        walletId: wallet,
        flowId: flow,
        category: "Casa",
        note: "mutuo",
        schedule: RecurringFixture.schedule(.monthly(day: 1), from: "2025-11-01"),
        owner: "elisa"
    )
}
