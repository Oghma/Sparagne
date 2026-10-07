import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Where the due recurring periods sit among the grid's rows
/// (`LedgerLines.interleave`).
struct LedgerLinesTests {
    private static let zone = TimeZone(identifier: "Europe/Rome") ?? .gmt
    private static let october = MonthKey(year: 2026, month: 10)

    private static var calendar: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = zone
        return calendar
    }

    /// An expense on `day` of October 2026 at `hour`, Rome time.
    private static func row(day: Int, hour: Int = 12, kind: TransactionKind = .expense, note: String) -> TransactionRow {
        let date = calendar.date(from: DateComponents(year: 2026, month: 10, day: day, hour: hour)) ?? Date()
        let view = TransactionView(
            id: "row-\(note)",
            kind: kind,
            occurredAt: CoreDate.offset(date),
            amount: 1_000,
            categoryId: "cat",
            category: "Spesa",
            categoryIsSystem: false,
            note: note,
            createdBy: "matteo",
            voided: false,
            walletId: "wallet",
            flowId: "flow",
            fromId: nil,
            toId: nil,
            legs: []
        )
        return TransactionRow(view: view, names: NameBook(snapshot: nil))
    }

    private static func due(
        _ date: NaiveDate,
        kind: TransactionKind = .expense,
        note: String,
        category: String? = "Casa"
    ) -> DuePeriod {
        let template = RecurringView(
            id: "tpl-\(note)",
            kind: kind,
            amount: 78_000,
            walletId: nil,
            flowId: nil,
            category: category,
            note: note,
            schedule: Schedule(frequency: .monthly(day: 1), interval: 1, startDate: "2026-01-01", endDate: nil),
            enabled: true,
            archived: false
        )
        return DuePeriod(template: template, date: date)
    }

    /// `"r:coop"` for a row, `"p:mutuo"` for a pending period: the order in
    /// one line of text.
    private static func names(_ lines: [LedgerLine]) -> [String] {
        lines.map { line in
            switch line {
            case .row(let row): "r:\(row.note)"
            case .pending(let period): "p:\(period.template.note ?? "")"
            }
        }
    }

    private static func interleave(
        _ rows: [TransactionRow],
        _ due: [DuePeriod],
        direction: LedgerDirection = .expenses,
        search: String = "",
        hasMoreRows: Bool = false
    ) -> [String] {
        names(
            LedgerLines.interleave(
                rows: rows,
                due: due,
                month: october,
                direction: direction,
                search: search,
                hasMoreRows: hasMoreRows,
                timeZone: zone
            )
        )
    }

    @Test("A due period sits at its date among the rows, in ascending order")
    func order() {
        let rows = [Self.row(day: 2, note: "coop"), Self.row(day: 5, note: "bar"), Self.row(day: 20, note: "cinema")]
        let due = [Self.due("2026-10-15", note: "luce"), Self.due("2026-10-03", note: "mutuo")]
        #expect(Self.interleave(rows, due) == ["r:coop", "p:mutuo", "r:bar", "p:luce", "r:cinema"])
    }

    @Test("Periods before the first row lead, periods after the last row trail, and none needs rows to show")
    func edges() {
        let rows = [Self.row(day: 10, note: "coop")]
        let due = [Self.due("2026-10-31", note: "affitto"), Self.due("2026-10-01", note: "mutuo")]
        #expect(Self.interleave(rows, due) == ["p:mutuo", "r:coop", "p:affitto"])
        #expect(Self.interleave([], due) == ["p:mutuo", "p:affitto"])
    }

    @Test("With another page to come, the periods past the last row loaded wait for it")
    func morePages() {
        let firstPage = [Self.row(day: 2, note: "coop"), Self.row(day: 10, note: "bar")]
        let due = [
            Self.due("2026-10-05", note: "mutuo"),
            Self.due("2026-10-10", note: "luce"),
            Self.due("2026-10-25", note: "affitto"),
        ]
        // The next page may still hold rows of the 10th, and the 25th's
        // period would sit above every one of them.
        #expect(Self.interleave(firstPage, due, hasMoreRows: true) == ["r:coop", "p:mutuo", "r:bar"])
        let lastPage = firstPage + [Self.row(day: 10, hour: 20, note: "cena"), Self.row(day: 28, note: "cinema")]
        #expect(
            Self.interleave(lastPage, due)
                == ["r:coop", "p:mutuo", "r:bar", "r:cena", "p:luce", "p:affitto", "r:cinema"]
        )
    }

    @Test("Only the month on screen: a period of September or November stays off October's sheet")
    func monthBounds() {
        let due = [
            Self.due("2026-09-30", note: "settembre"),
            Self.due("2026-10-01", note: "primo"),
            Self.due("2026-10-31", note: "ultimo"),
            Self.due("2026-11-01", note: "novembre"),
        ]
        #expect(Self.interleave([], due) == ["p:primo", "p:ultimo"])
    }

    @Test("An income template shows under ENTRATE only, an expense template under USCITE only")
    func direction() {
        let due = [Self.due("2026-10-01", kind: .income, note: "stipendio"), Self.due("2026-10-01", note: "mutuo")]
        #expect(Self.interleave([], due, direction: .income) == ["p:stipendio"])
        #expect(Self.interleave([], due, direction: .expenses) == ["p:mutuo"])
    }

    @Test("On a day that already has rows, the period comes after them, before the next day's")
    func sameDay() {
        let rows = [
            Self.row(day: 1, hour: 8, note: "colazione"),
            Self.row(day: 1, hour: 23, note: "cena"),
            Self.row(day: 2, hour: 0, note: "benzina"),
        ]
        let due = [Self.due("2026-10-01", note: "mutuo")]
        #expect(Self.interleave(rows, due) == ["r:colazione", "r:cena", "p:mutuo", "r:benzina"])
    }

    @Test("A search narrows the periods as it does the rows, by note or category, ignoring case and accents")
    func search() {
        let due = [
            Self.due("2026-10-01", note: "mutuo", category: "Casa"),
            Self.due("2026-10-02", note: "palestra", category: "Salute"),
        ]
        #expect(Self.interleave([], due, search: "MUT") == ["p:mutuo"])
        #expect(Self.interleave([], due, search: "salute") == ["p:palestra"])
        #expect(Self.interleave([], due, search: "   ") == ["p:mutuo", "p:palestra"])
    }

    @Test("The DATA column writes the day in two digits and the weekday in the locale's short form")
    func dayAndWeekday() throws {
        let thursday = try #require(Self.calendar.date(from: DateComponents(year: 2026, month: 10, day: 1, hour: 12)))
        #expect(LedgerDate.dayNumber(thursday, calendar: Self.calendar) == "01")
        #expect(LedgerDate.weekday(thursday, calendar: Self.calendar, locale: Locale(identifier: "it_IT")) == "gio")
        #expect(LedgerDate.weekday(thursday, calendar: Self.calendar, locale: Locale(identifier: "en_US")) == "Thu")
    }
}
