import Foundation
import SparagneCore

/// One calendar month, the unit the ledger reads and writes in.
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

    /// `"Agosto 2026"`: the top bar's month, in sentence case like every
    /// other label of the window, whatever case the
    /// locale gives its month names.
    func title(locale: Locale = .autoupdatingCurrent) -> String {
        let name = LedgerDate.fullMonth(month, locale: locale).lowercased(with: locale)
        return "\(name.prefix(1).uppercased(with: locale))\(name.dropFirst()) \(year)"
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
    /// `net_expense` nets them off.
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

/// The rows picked with ⌘-click, ⇧-click and ⌘A, for the bulk actions of the
/// selection bar. Apart from editing: a plain click still opens one row, and
/// clears this.
///
/// A value with no SwiftUI in it, so the gestures can be tested without a
/// window (`SparagneTests/SelectionTests.swift`).
struct RowSelection: Equatable, Sendable {
    private(set) var ids: Set<Uuid> = []
    /// Where a ⇧-click extends from: the last row ⌘-clicked.
    private(set) var anchor: Uuid?
    /// What was selected when the anchor was set. A second ⇧-click replaces
    /// the range of the first instead of adding to it, so a range can shrink
    /// as well as grow, the way a Finder list behaves.
    private var base: Set<Uuid> = []

    var isEmpty: Bool { ids.isEmpty }
    var count: Int { ids.count }

    func contains(_ id: Uuid) -> Bool { ids.contains(id) }

    /// ⌘-click: the row goes in or out, and becomes the anchor either way.
    mutating func toggle(_ id: Uuid) {
        if ids.remove(id) == nil { ids.insert(id) }
        anchor = id
        base = ids
    }

    /// ⇧-click: every row from the anchor to `id`, in the order on screen, on
    /// top of what was selected before the anchor was set. With no anchor on
    /// screen the row alone is selected and becomes one.
    mutating func extend(to id: Uuid, in order: [Uuid]) {
        guard let anchor, let from = order.firstIndex(of: anchor), let to = order.firstIndex(of: id) else {
            ids.insert(id)
            anchor = id
            base = ids
            return
        }
        ids = base.union(order[min(from, to)...max(from, to)])
    }

    /// ⌘A: every row on screen.
    mutating func selectAll(_ order: [Uuid]) {
        ids = Set(order)
        anchor = order.first
        base = []
    }

    mutating func clear() {
        ids = []
        anchor = nil
        base = []
    }

    /// Drops the rows no longer on screen: a sync or a void took them away,
    /// and a row nobody can see must not be voided with the rest.
    mutating func retain(_ visible: Set<Uuid>) {
        ids.formIntersection(visible)
        base.formIntersection(visible)
        if let anchor, !visible.contains(anchor) { self.anchor = nil }
    }
}

/// What ⌫ and ⌦ delete while no cell has the caret (`LedgerGrid.selectionKeys`).
/// "Delete" is the window's word: the core voids the row (`VoidTransaction`),
/// since its log is append-only and synced, and the row stays in the history.
///
/// The rows picked come first, as they always have; with none picked, the row
/// under the pointer, the one whose trash icon is showing. A value with no
/// SwiftUI in it, so the choice is tested without a window
/// (`SparagneTests/DeleteTargetTests.swift`).
enum DeleteTarget: Equatable, Sendable {
    /// The selection, in the order on screen. The store passes over what a
    /// bulk action skips, deleted rows and transfers (`AppStore.bulkTargets`).
    case selection([Uuid])
    /// The row under the pointer, alone, as its context menu deletes it.
    case row(Uuid)
    /// Nothing: the keys go on to the system, which beeps.
    case none

    /// - Parameters:
    ///   - selection: the picked rows, in the order on screen.
    ///   - hovered: the row under the pointer, if it is still on screen.
    ///   - editing: a row is open or a cell has the caret, so the keys belong
    ///     to the text being typed.
    ///   - canWrite: the vault on screen is not one this account only reads.
    static func resolve(
        selection: [Uuid],
        hovered: TransactionRow?,
        editing: Bool,
        canWrite: Bool
    ) -> DeleteTarget {
        guard canWrite, !editing else { return .none }
        if !selection.isEmpty { return .selection(selection) }
        guard let hovered, isDeletable(hovered) else { return .none }
        return .row(hovered.id)
    }

    /// Whether `row` can be deleted on its own: what the context menu's
    /// Delete, the trash icon and ⌫ over the row all ask. A row deleted
    /// already cannot be deleted again. A transfer can, as its context menu
    /// has always allowed: only the bulk actions pass over transfers, which
    /// share their targets with Set Category (`AppStore.bulkTargets`).
    static func isDeletable(_ row: TransactionRow) -> Bool {
        !row.voided
    }
}

/// The sheets of the tab bar at the bottom of the window, in the bar's
/// order, which is also ⌘1 to ⌘5; the window opens on the first.
enum LedgerTab: String, CaseIterable, Identifiable, Sendable {
    case summary
    case ledger
    /// The recurring templates and the periods waiting for a decision.
    case recurring
    /// The allocation plan: what each envelope gets out of Unallocated, and
    /// the period waiting to be shared out.
    case allocation
    /// Envelopes and categories, as two editable tables.
    case setup

    var id: String { rawValue }

    var label: String {
        switch self {
        case .summary: String(localized: "Summary")
        case .ledger: String(localized: "Ledger")
        case .recurring: String(localized: "Recurring")
        case .allocation: String(localized: "Allocation")
        case .setup: String(localized: "Setup")
        }
    }

    /// The digit of its ⌘-shortcut in the View menu: its place in the bar,
    /// from 1.
    var shortcut: Int {
        (Self.allCases.firstIndex(of: self) ?? 0) + 1
    }

    /// The tab ⌘`shortcut` selects; `nil` past the last one.
    init?(shortcut: Int) {
        let tabs = Self.allCases
        guard tabs.indices.contains(shortcut - 1) else { return nil }
        self = tabs[shortcut - 1]
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
