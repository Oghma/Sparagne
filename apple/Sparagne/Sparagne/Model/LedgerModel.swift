import Foundation
import SparagneCore

/// One calendar month, the unit the ledger reads and writes in
/// (`docs/v2/UI.md` §2.1).
///
/// All the range arithmetic lives here, in the system calendar, so the core
/// only ever receives two UTC instants and never has to know about months,
/// leap years or DST.
struct MonthKey: Hashable, Sendable, Identifiable {
    let year: Int
    /// 1...12.
    let month: Int

    var id: String { "\(year)-\(month)" }

    init(year: Int, month: Int) {
        self.year = year
        self.month = month
    }

    init(_ date: Date, calendar: Calendar = .current) {
        let parts = calendar.dateComponents([.year, .month], from: date)
        year = parts.year ?? 1970
        month = parts.month ?? 1
    }

    /// Local midnight on the first day of the month.
    func start(_ calendar: Calendar = .current) -> Date {
        calendar.date(from: DateComponents(year: year, month: month, day: 1)) ?? Date()
    }

    /// Local midnight on the first day of the next month: the open end of the
    /// half-open range the core queries take.
    func end(_ calendar: Calendar = .current) -> Date {
        calendar.date(byAdding: .month, value: 1, to: start(calendar)) ?? start(calendar)
    }

    /// `[from, to)` as the UTC strings the FFI expects.
    func bounds(_ calendar: Calendar = .current) -> (from: UtcDateTime, to: UtcDateTime) {
        (CoreDate.utcString(start(calendar)), CoreDate.utcString(end(calendar)))
    }

    func adding(months: Int, calendar: Calendar = .current) -> MonthKey {
        let moved = calendar.date(byAdding: .month, value: months, to: start(calendar)) ?? start(calendar)
        return MonthKey(moved, calendar: calendar)
    }

    /// `"AGOSTO 2026"`.
    func title(locale: Locale = .autoupdatingCurrent) -> String {
        "\(LedgerDate.fullMonth(month, locale: locale)) \(year)"
    }

    /// The twelve months of this month's year, January first.
    func yearMonths() -> [MonthKey] {
        (1...12).map { MonthKey(year: year, month: $0) }
    }

    /// The 13 boundaries `bucket_totals` needs for the twelve bars of a year.
    static func yearBounds(_ year: Int, calendar: Calendar = .current) -> [UtcDateTime] {
        let starts = (1...12).map { MonthKey(year: year, month: $0).start(calendar) }
        let end = MonthKey(year: year, month: 12).end(calendar)
        return (starts + [end]).map(CoreDate.utcString)
    }

    /// The twelve months ending with this one, oldest first, and their 13
    /// boundaries: the "last twelve months" strip of the summary panel.
    func trailingYear(calendar: Calendar = .current) -> (months: [MonthKey], bounds: [UtcDateTime]) {
        let months = (-11...0).map { adding(months: $0, calendar: calendar) }
        let bounds = months.map { $0.start(calendar) } + [end(calendar)]
        return (months, bounds.map(CoreDate.utcString))
    }
}

/// The `USCITE / ENTRATE` switch of the ledger header.
///
/// Transfers are in neither: they move money without earning or spending it.
/// The View menu can add them to the list (`showTransfers`) for the rare check.
enum LedgerDirection: String, CaseIterable, Identifiable, Sendable {
    case expenses
    case income

    var id: String { rawValue }

    var label: String {
        switch self {
        case .expenses: String(localized: "Expenses")
        case .income: String(localized: "Income")
        }
    }

    /// Refunds belong with expenses: they are a negative expense, and
    /// `net_expense` nets them off (`DISTILLATO_V1.md` §3.5).
    var kinds: [TransactionKind] {
        switch self {
        case .expenses: [.expense, .refund]
        case .income: [.income]
        }
    }

    /// The kind a new row typed in the grid gets.
    var newRowKind: TransactionKind {
        switch self {
        case .expenses: .expense
        case .income: .income
        }
    }
}

/// The two views behind the title-bar switcher (`docs/v2/UI.md` §2), in
/// the switcher's order; the first is the one the window opens on.
enum LedgerTab: String, CaseIterable, Identifiable, Sendable {
    case summary
    case ledger

    var id: String { rawValue }

    var label: String {
        switch self {
        case .summary: String(localized: "Summary")
        case .ledger: String(localized: "Ledger")
        }
    }
}

/// Everything the ledger's summary panel draws, loaded together so the
/// numbers on screen always describe one month. The RIEPILOGO view has its
/// own model (`YearSummary`).
struct LedgerSummary: Sendable {
    /// The month these figures cover.
    let month: MonthKey
    /// Envelope x person, straight from `flow_person_totals`.
    let flowPerson: [FlowPersonTotals]
    /// Heaviest net expense first.
    let categories: [CategoryTotals]
    /// The month's totals, for the cards.
    let totals: PeriodTotals
    /// The month before, for the "+4,2% vs luglio" line.
    let previous: PeriodTotals
    /// Twelve buckets ending with `month`, oldest first, and their labels.
    let trailing: [PeriodTotals]
    let trailingMonths: [MonthKey]

    /// `income - net_expense`, the ledger's definition of savings.
    static func savings(_ totals: PeriodTotals) -> Int64 {
        totals.income - totals.netExpense
    }

    var savings: Int64 { Self.savings(totals) }
    var previousSavings: Int64 { Self.savings(previous) }

    /// The people who moved money this month, ordered as the core returned
    /// them (case-insensitive by name).
    var people: [String] {
        var seen: [String] = []
        for row in flowPerson where !seen.contains(row.person) {
            seen.append(row.person)
        }
        return seen
    }

    /// Income, net expense and savings for one person.
    func totals(for person: String) -> (income: Int64, netExpense: Int64, savings: Int64) {
        let rows = flowPerson.filter { $0.person == person }
        let income = rows.reduce(0) { $0 + $1.income }
        // Refunds net off per envelope, so the floor at zero is applied there
        // and summed, not re-applied to the total.
        let net = rows.reduce(0) { $0 + $1.netExpense }
        return (income, net, income - net)
    }

    /// Net expense on one envelope for one person; the cells of the
    /// `Uscite cash / varie / …` table.
    func netExpense(flow: Uuid, person: String) -> Int64 {
        flowPerson
            .filter { $0.flowId == flow && $0.person == person }
            .reduce(0) { $0 + $1.netExpense }
    }

    /// Net expense on one envelope, all people.
    func netExpense(flow: Uuid) -> Int64 {
        flowPerson.filter { $0.flowId == flow }.reduce(0) { $0 + $1.netExpense }
    }

    /// Net expense by person, largest first: "chi ha speso cosa".
    var spendByPerson: [(person: String, amount: Int64)] {
        people
            .map { (person: $0, amount: totals(for: $0).netExpense) }
            .filter { $0.amount > 0 }
            .sorted { $0.amount > $1.amount }
    }
}
