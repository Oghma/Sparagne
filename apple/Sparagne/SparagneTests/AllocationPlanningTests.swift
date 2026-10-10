import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// What the Riparto tab works out around the core's answers: the order of
/// the lines, the moves Distribuisci sends, the plan's figures and warnings,
/// and the words of a line's status and of a decided period.
struct AllocationPlanningTests {
    private static let rent: Uuid = "flow-affitto"
    private static let savings: Uuid = "flow-risparmi"
    private static let food: Uuid = "flow-spesa"
    private static let holiday: Uuid = "flow-vacanze"

    /// Affitto 800,00, Risparmi 10%, Spesa up to the cap, Vacanze 5%.
    private static let lines = [
        AllocationLine(flowId: rent, rule: .fixed(amount: 80_000)),
        AllocationLine(flowId: savings, rule: .percent(basisPoints: 1_000)),
        AllocationLine(flowId: food, rule: .fillToCap),
        AllocationLine(flowId: holiday, rule: .percent(basisPoints: 500)),
    ]

    private static func flow(
        _ id: Uuid,
        mode: FlowMode = .unlimited,
        archived: Bool = false
    ) -> FlowView {
        FlowView(
            id: id,
            name: id,
            balance: 0,
            mode: mode,
            incomeTotal: nil,
            allowNegative: false,
            archived: archived,
            isUnallocated: false
        )
    }

    private static func previewLine(
        _ flowId: Uuid,
        rule: AllocationRule = .fixed(amount: 10_000),
        wanted: Int64 = 10_000,
        room: Int64? = nil,
        amount: Int64,
        status: LineStatus = .full
    ) -> PreviewLine {
        PreviewLine(
            flowId: flowId,
            rule: rule,
            wanted: wanted,
            room: room,
            amount: amount,
            balanceAfter: amount,
            status: status
        )
    }

    private static func preview(_ lines: [PreviewLine], unallocatedAfter: Int64 = 0) -> AllocationPreview {
        let distributed = lines.reduce(0) { $0 + $1.amount }
        return AllocationPreview(
            total: distributed,
            lines: lines,
            distributed: distributed,
            remainder: 0,
            unallocatedAfter: unallocatedAfter,
            percentTotalBp: 0
        )
    }

    // MARK: - The order

    @Test("A line moves up or down one place, and never off the list")
    func moving() {
        let ids = Self.lines.map(\.flowId)
        #expect(AllocationLines.moving(ids, at: 1, by: -1) == [Self.savings, Self.rent, Self.food, Self.holiday])
        #expect(AllocationLines.moving(ids, at: 1, by: 1) == [Self.rent, Self.food, Self.savings, Self.holiday])
        #expect(AllocationLines.moving(ids, at: 3, by: -3) == [Self.holiday, Self.rent, Self.savings, Self.food])
        #expect(AllocationLines.moving(ids, at: 0, by: -1) == nil)
        #expect(AllocationLines.moving(ids, at: 3, by: 1) == nil)
        #expect(AllocationLines.moving(ids, at: 2, by: 0) == nil)
        #expect(AllocationLines.moving(ids, at: 7, by: -1) == nil)
        #expect(AllocationLines.moving([Self.rent], at: 0, by: 1) == nil)
    }

    @Test("A line leaves the plan, unless it is the last one")
    func removing() {
        let remaining = AllocationLines.removing(Self.savings, from: Self.lines)
        #expect(remaining?.map(\.flowId) == [Self.rent, Self.food, Self.holiday])
        #expect(AllocationLines.removing("flow-unknown", from: Self.lines) == nil)
        #expect(AllocationLines.removing(Self.rent, from: [Self.lines[0]]) == nil)
    }

    // MARK: - Distribuisci

    @Test("Distribuisci sends one move per line that gets something, in the plan's order")
    func moves() {
        let preview = Self.preview([
            Self.previewLine(Self.rent, amount: 80_000),
            Self.previewLine(Self.savings, amount: 0, status: .short),
            Self.previewLine(Self.food, amount: 12_345, status: .capLimited),
            Self.previewLine(Self.holiday, amount: 0, status: .archived),
        ])
        #expect(AllocationDecision(preview: preview) == .execute([
            AllocationMove(flowId: Self.rent, amount: 80_000),
            AllocationMove(flowId: Self.food, amount: 12_345),
        ]))
    }

    @Test("A period where no line gets anything is skipped")
    func allZeroSkips() {
        let nothing = Self.preview([
            Self.previewLine(Self.rent, amount: 0, status: .short),
            Self.previewLine(Self.food, amount: 0, status: .noCap),
        ])
        #expect(AllocationDecision(preview: nothing) == .skip)
        #expect(AllocationDecision(preview: Self.preview([])) == .skip)
    }

    // MARK: - Figures and warnings

    @Test("The figures add the fixed amounts and the percentages up, and count the lines to the cap")
    func figures() {
        let figures = AllocationFigures(lines: Self.lines)
        #expect(figures.lines == 4)
        #expect(figures.fixed == 80_000)
        #expect(figures.percent == 1_500)
        #expect(figures.fillToCap == 1)
        #expect(AllocationFigures(lines: []).lines == 0)
    }

    @Test("An archived or unknown envelope, Al tetto without a cap and percentages above 100% are warnings")
    func planWarnings() {
        let lines = Self.lines + [
            AllocationLine(flowId: "flow-gone", rule: .fixed(amount: 100)),
            AllocationLine(flowId: "flow-big", rule: .percent(basisPoints: 9_000)),
        ]
        let flows = [
            Self.flow(Self.rent),
            Self.flow(Self.savings, mode: .netCapped(cap: 500_000)),
            Self.flow(Self.food),
            Self.flow(Self.holiday, archived: true),
            Self.flow("flow-big"),
        ]
        let warnings = AllocationWarning.of(lines: lines, flows: flows, preview: nil, due: true)
        #expect(warnings == [
            .noCap(flowId: Self.food),
            .archived(flowId: Self.holiday),
            .archived(flowId: "flow-gone"),
            .percentOver(basisPoints: 10_500),
        ])

        // Up to the cap on a capped envelope, 100% exactly: nothing to say.
        let capped = [Self.flow(Self.food, mode: .incomeCapped(cap: 100_000)), Self.flow(Self.savings)]
        let fine = [
            AllocationLine(flowId: Self.food, rule: .fillToCap),
            AllocationLine(flowId: Self.savings, rule: .percent(basisPoints: 10_000)),
        ]
        #expect(AllocationWarning.of(lines: fine, flows: capped, preview: nil, due: true).isEmpty)
    }

    @Test("A shortfall and an overdrawn Unallocated warn only for the period due")
    func previewWarnings() {
        let flows = Self.lines.map { Self.flow($0.flowId) }
        let preview = Self.preview(
            [
                Self.previewLine(Self.rent, amount: 80_000),
                Self.previewLine(Self.holiday, wanted: 176_000, amount: 143_250, status: .short),
            ],
            unallocatedAfter: -500
        )
        let lines = [Self.lines[0], Self.lines[3]]
        #expect(AllocationWarning.of(lines: lines, flows: flows, preview: preview, due: true) == [
            .short(flowId: Self.holiday, amount: 143_250, wanted: 176_000),
            .overdrawn(unallocatedAfter: -500),
        ])
        #expect(AllocationWarning.of(lines: lines, flows: flows, preview: preview, due: false).isEmpty)
    }

    @Test("A preview line belongs to a row only when it was worked out on that very line")
    func previewMatch() {
        let preview = Self.preview([
            Self.previewLine(Self.rent, rule: .fixed(amount: 80_000), amount: 80_000),
            Self.previewLine(Self.savings, rule: .percent(basisPoints: 2_000), amount: 1),
        ])
        let matched = AllocationPreviewMatch.lines(Self.lines, in: preview)
        #expect(matched.count == 4)
        #expect(matched[0]?.amount == 80_000)
        // Risparmi was edited to 10% since: the 20% answer is not its.
        #expect(matched[1] == nil)
        #expect(matched[2] == nil)
        #expect(AllocationPreviewMatch.lines(Self.lines, in: nil).allSatisfy { $0 == nil })
    }

    // MARK: - Words

    @Test("Each status has its tag, and only what a line misses warns")
    func statusTags() {
        let full = AllocationText.tag(Self.previewLine(Self.rent, amount: 10_000))
        #expect(full == AllocationText.Tag(text: String(localized: "complete"), warns: false))

        let alreadyFull = AllocationText.tag(
            Self.previewLine(Self.food, rule: .fillToCap, wanted: 0, room: 0, amount: 0)
        )
        #expect(alreadyFull.text == String(localized: "already full"))
        // A percent of a zero total also asks for nothing, and is complete.
        let zeroPercent = AllocationText.tag(
            Self.previewLine(Self.savings, rule: .percent(basisPoints: 1_000), wanted: 0, amount: 0)
        )
        #expect(zeroPercent.text == String(localized: "complete"))

        let capped = AllocationText.tag(Self.previewLine(Self.food, room: 5_000, amount: 5_000, status: .capLimited))
        #expect(capped == AllocationText.Tag(text: String(localized: "at the cap"), warns: false))

        let short = Self.previewLine(Self.holiday, wanted: 176_000, amount: 143_250, status: .short)
        let missing = LedgerMoney.bare(32_750)
        #expect(AllocationText.tag(short) == AllocationText.Tag(text: String(localized: "\(missing) short"), warns: true))
        #expect(!AllocationText.tag(short, due: false).warns)

        let noCap = AllocationText.tag(Self.previewLine(Self.food, amount: 0, status: .noCap))
        #expect(noCap == AllocationText.Tag(text: String(localized: "no cap"), warns: true))
        let archived = AllocationText.tag(Self.previewLine(Self.holiday, amount: 0, status: .archived))
        #expect(archived == AllocationText.Tag(text: String(localized: "archived"), warns: true))
    }

    @Test("A rule reads as the table writes it")
    func ruleText() {
        #expect(AllocationText.rule(.fixed(amount: 140_000), cap: nil) == String(localized: "Fixed \(LedgerMoney.bare(140_000))"))
        #expect(AllocationText.rule(.percent(basisPoints: 1_250), cap: nil) == String(localized: "\("12,5%") of the total"))
        #expect(AllocationText.rule(.fillToCap, cap: 300_000) == String(localized: "Up to the cap \(LedgerMoney.bare(300_000))"))
        #expect(AllocationText.rule(.fillToCap, cap: nil) == String(localized: "Up to the cap"))
    }

    @Test("A decided period counts what went out, transfers deleted since left out")
    func history() {
        let run = AllocationRunView(
            periodDate: "2026-08-27",
            outcome: .executed,
            total: 425_000,
            moves: [
                RunMove(flowId: Self.rent, amount: 80_000, transactionId: "t1", voided: false),
                RunMove(flowId: Self.food, amount: 20_000, transactionId: "t2", voided: true),
            ],
            createdBy: "matteo"
        )
        #expect(AllocationText.distributed(run) == 80_000)
        #expect(AllocationText.detail(run).hasSuffix("\u{00B7} matteo"))
        let skipped = AllocationRunView(periodDate: "2026-07-27", outcome: .skipped, total: 0, moves: [], createdBy: "elisa")
        #expect(AllocationText.distributed(skipped) == 0)
        #expect(AllocationText.detail(skipped) == "\(String(localized: "incomes handled by hand")) \u{00B7} elisa")
        #expect(AllocationText.outcome(.executed) != AllocationText.outcome(.skipped))
    }

    @Test("The cadence hint names the day, and says nothing for an interval above one")
    func cadenceHint() {
        let english = Locale(identifier: "en_US")
        #expect(AllocationText.cadenceHint(RecurringFixture.schedule(.monthly(day: 27)), locale: english)
            == String(localized: "due on day \(27)"))
        #expect(AllocationText.cadenceHint(RecurringFixture.schedule(.weekly(weekday: 1)), locale: english)
            == String(localized: "due every \("Monday")"))
        #expect(AllocationText.cadenceHint(RecurringFixture.schedule(.monthly(day: 27), every: 2)) == nil)
        #expect(AllocationText.cadenceHint(RecurringFixture.schedule(.daily)) == nil)
    }

    @Test("Counts read as one or many, and the incomes say since when, in English and in Italian")
    func counts() throws {
        let english = Locale(identifier: "en_US")
        let italian = Locale(identifier: "it_IT")
        let en = try AllocationCatalog.english()
        let it = try AllocationCatalog.italian()

        #expect(AllocationText.envelopes(1, bundle: en, locale: english) == "1 envelope")
        #expect(AllocationText.envelopes(5, bundle: en, locale: english) == "5 envelopes")
        #expect(AllocationText.envelopes(1, bundle: it, locale: italian) == "1 busta")
        #expect(AllocationText.envelopes(5, bundle: it, locale: italian) == "5 buste")
        #expect(AllocationText.missed(1, bundle: it, locale: italian) == "comprende 1 periodo precedente")
        #expect(AllocationText.missed(2, bundle: it, locale: italian) == "comprende 2 periodi precedenti")
        #expect(AllocationText.toShareOut(1, bundle: it, locale: italian) == "1 da distribuire")

        #expect(AllocationText.incomes(2, after: "2026-08-27", since: "2026-06-27", bundle: it, locale: italian)
            == "2 entrate arrivate in Non allocato dopo l'ultimo riparto (gio 27 ago)")
        #expect(AllocationText.incomes(1, after: nil, since: "2026-06-27", bundle: it, locale: italian)
            == "1 entrata arrivata in Non allocato dal sab 27 giu")
        #expect(AllocationText.incomes(1, after: nil, since: "2026-06-27", bundle: en, locale: english)
            == "1 income reached Unallocated since Sat, Jun 27")
    }

    // MARK: - The toast

    @Test("The toast's window runs from Distribuisci and ends after its duration")
    func undoWindow() {
        let start = Date(timeIntervalSince1970: 1_000)
        let undo = AllocationUndo(
            vaultId: "vault",
            planId: "plan",
            periodDate: "2026-09-27",
            distributed: 425_000,
            startedAt: start,
            duration: .seconds(8)
        )
        #expect(undo.progress(at: start) == 0)
        #expect(undo.progress(at: start.addingTimeInterval(4)) == 0.5)
        #expect(undo.progress(at: start.addingTimeInterval(20)) == 1)
        #expect(undo.deadline == start.addingTimeInterval(8))
    }
}

/// A string catalog built on disk for one language, with the plural keys of
/// the Riparto tab and the sentences they go into: the helpers read it
/// through their `bundle` parameter, whatever the app's own catalog holds.
private enum AllocationCatalog {
    static func bundle(
        language: String,
        plurals: [String: (one: String, other: String)],
        strings: [String: String]
    ) throws -> Bundle {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "sparagne-allocation-\(UUID().uuidString).bundle", directoryHint: .isDirectory)
        let contents = root.appending(path: "Contents", directoryHint: .isDirectory)
        let lproj = contents.appending(path: "Resources/\(language).lproj", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: lproj, withIntermediateDirectories: true)
        let info: [String: Any] = [
            "CFBundleIdentifier": "it.oghma.sparagne.tests.allocation.\(language)",
            "CFBundleDevelopmentRegion": language,
            "CFBundlePackageType": "BNDL",
        ]
        try plist(info).write(to: contents.appending(path: "Info.plist"))
        var rules: [String: Any] = [:]
        for (key, forms) in plurals {
            rules[key] = [
                "NSStringLocalizedFormatKey": "%#@count@",
                "count": [
                    "NSStringFormatSpecTypeKey": "NSStringPluralRuleType",
                    "NSStringFormatValueTypeKey": "lld",
                    "one": forms.one,
                    "other": forms.other,
                ],
            ]
        }
        try plist(rules).write(to: lproj.appending(path: "Localizable.stringsdict"))
        try plist(strings).write(to: lproj.appending(path: "Localizable.strings"))
        return try #require(Bundle(url: root))
    }

    private static func plist(_ value: Any) throws -> Data {
        try PropertyListSerialization.data(fromPropertyList: value, format: .xml, options: 0)
    }

    static func english() throws -> Bundle {
        try bundle(
            language: "en",
            plurals: [
                "%lld envelopes": ("%lld envelope", "%lld envelopes"),
                "%lld incomes reached Unallocated": ("%lld income reached Unallocated", "%lld incomes reached Unallocated"),
            ],
            strings: [
                "%@ since %@": "%1$@ since %2$@",
            ]
        )
    }

    static func italian() throws -> Bundle {
        try bundle(
            language: "it",
            plurals: [
                "%lld envelopes": ("%lld busta", "%lld buste"),
                "%lld earlier periods included": ("comprende %lld periodo precedente", "comprende %lld periodi precedenti"),
                "%lld to share out": ("%lld da distribuire", "%lld da distribuire"),
                "%lld incomes reached Unallocated": ("%lld entrata arrivata in Non allocato", "%lld entrate arrivate in Non allocato"),
            ],
            strings: [
                "%@ after the last allocation (%@)": "%1$@ dopo l'ultimo riparto (%2$@)",
                "%@ since %@": "%1$@ dal %2$@",
            ]
        )
    }
}
