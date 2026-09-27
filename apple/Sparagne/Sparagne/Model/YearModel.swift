import Foundation
import SparagneCore

/// One person's movement in one bucket of the core's `year_breakdown`
/// (`docs/v2/UI.md` §4). Bucket 0 is everything before January; 1...12 are
/// the months of the year.
struct YearRow: Hashable, Sendable {
    let bucket: Int
    let person: String
    /// Income other than opening balances.
    let income: Int64
    /// Opening balances of wallets (the system category `Opening`), signed.
    let opening: Int64
    /// `net_expense` on envelopes without a cap.
    let cashExpense: Int64
    /// `net_expense` on envelopes with a cap: the "fondi".
    let fundExpense: Int64
}

/// A capped envelope as the RIEPILOGO's gauges show it (`docs/v2/UI.md`
/// §2.2): how full it is against its cap.
struct FundGauge: Identifiable, Hashable, Sendable {
    let id: Uuid
    let name: String
    let cap: Int64
    /// The balance for a net cap, the cumulative income for an income cap
    /// (`DISTILLATO_V1.md` §2.2).
    let filled: Int64

    /// 0...1.
    var fraction: Double {
        guard cap > 0 else { return 0 }
        return min(max(Double(filled) / Double(cap), 0), 1)
    }
}

/// One line of the RIEPILOGO's month table (`docs/v2/UI.md` §2.2), everybody
/// together, plus the running total of each person.
struct YearMonth: Hashable, Sendable {
    let month: MonthKey
    /// After the month on screen, in the year on screen: drawn blank.
    let isFuture: Bool
    /// ENTRATE: income without the opening balances.
    let income: Int64
    /// USCITE: net expense on the envelopes without a cap.
    let cashExpense: Int64
    /// USCITE FONDI: net expense on the envelopes with a cap.
    let fundExpense: Int64
    /// FONDO CASSA: the TOTALE of the month before, plus this month's
    /// opening balances.
    let carried: Int64
    /// TOTALE: `savings + carried - fundExpense`, the wallets' total balance
    /// at the end of the month.
    let total: Int64
    /// The same TOTALE per person, in `YearSummary.people` order.
    let totalByPerson: [Int64]

    /// RISPARMIO.
    var savings: Int64 { income - cashExpense }
}

/// Everything the RIEPILOGO draws for one year, up to the month on screen.
struct YearSummary: Sendable {
    let year: Int
    /// The month on screen: decides the year and which months are future.
    let upTo: MonthKey
    /// One column per author of the vault, in the core's order.
    let people: [String]
    /// FONDO CASSA INIZIALE per person: everything before January.
    let initialByPerson: [Int64]
    /// January first, always twelve.
    let months: [YearMonth]
    /// The active capped envelopes, in the vault's order.
    let funds: [FundGauge]

    var initial: Int64 { initialByPerson.reduce(0, +) }

    /// Pure arithmetic over the query's rows and the vault's envelopes, so it
    /// can be tested without a database (`docs/v2/UI.md` §2.2 for the
    /// definitions).
    static func build(year: Int, upTo: MonthKey, rows: [YearRow], flows: [FlowView]) -> YearSummary {
        let people = Self.people(in: rows)
        var byBucket: [Int: [YearRow]] = [:]
        for row in rows { byBucket[row.bucket, default: []].append(row) }

        // Bucket 0 is everything before January: it opens the recurrence.
        let initialByPerson = people.map { person in
            Self.closing(byBucket[0, default: []].filter { $0.person == person })
        }

        var months: [YearMonth] = []
        var runningTotal = initialByPerson.reduce(0, +)
        var runningByPerson = initialByPerson
        for month in 1...12 {
            let monthRows = byBucket[month, default: []]
            let income = monthRows.reduce(0) { $0 + $1.income }
            let cashExpense = monthRows.reduce(0) { $0 + $1.cashExpense }
            let fundExpense = monthRows.reduce(0) { $0 + $1.fundExpense }
            // FONDO CASSA: last month's TOTALE plus the wallets opened now.
            let carried = runningTotal + monthRows.reduce(0) { $0 + $1.opening }
            let total = (income - cashExpense) + carried - fundExpense

            var totalByPerson: [Int64] = []
            totalByPerson.reserveCapacity(people.count)
            for (index, person) in people.enumerated() {
                let personRows = monthRows.filter { $0.person == person }
                let personCarried = runningByPerson[index] + personRows.reduce(0) { $0 + $1.opening }
                let personSavings = personRows.reduce(0) { $0 + $1.income - $1.cashExpense }
                let personFunds = personRows.reduce(0) { $0 + $1.fundExpense }
                totalByPerson.append(personSavings + personCarried - personFunds)
            }

            months.append(
                YearMonth(
                    month: MonthKey(year: year, month: month),
                    isFuture: Self.isFuture(year: year, month: month, upTo: upTo),
                    income: income,
                    cashExpense: cashExpense,
                    fundExpense: fundExpense,
                    carried: carried,
                    total: total,
                    totalByPerson: totalByPerson
                )
            )
            runningTotal = total
            runningByPerson = totalByPerson
        }

        return YearSummary(
            year: year,
            upTo: upTo,
            people: people,
            initialByPerson: initialByPerson,
            months: months,
            funds: Self.funds(in: flows)
        )
    }

    /// One column per author, de-duplicated in the core's order and then
    /// sorted case-insensitively, so two runs of the same vault always put
    /// the columns in the same place.
    private static func people(in rows: [YearRow]) -> [String] {
        var seen = Set<String>()
        var people: [String] = []
        for row in rows where seen.insert(row.person).inserted {
            people.append(row.person)
        }
        return people.sorted { $0.lowercased() < $1.lowercased() }
    }

    /// What a bucket leaves behind: everything that came in, less everything
    /// that went out, on both kinds of envelope.
    private static func closing(_ rows: [YearRow]) -> Int64 {
        rows.reduce(0) { $0 + $1.income + $1.opening - $1.cashExpense - $1.fundExpense }
    }

    /// Blank rows: the months after the one on screen, in the year on screen.
    /// A later year is all future, an earlier one all past.
    private static func isFuture(year: Int, month: Int, upTo: MonthKey) -> Bool {
        year > upTo.year || (year == upTo.year && month > upTo.month)
    }

    /// A "fondo" is an envelope with a cap, which makes the classification a
    /// fact of the vault rather than a name (`docs/v2/UI.md` §2.2).
    private static func funds(in flows: [FlowView]) -> [FundGauge] {
        flows.compactMap { flow in
            guard !flow.archived else { return nil }
            switch flow.mode {
            case .unlimited:
                return nil
            case .netCapped(let cap):
                return FundGauge(id: flow.id, name: flow.name, cap: cap, filled: flow.balance)
            case .incomeCapped(let cap):
                // Spending does not free room on an income cap, so what fills
                // the ring is the cumulative income.
                return FundGauge(id: flow.id, name: flow.name, cap: cap, filled: flow.incomeTotal ?? flow.balance)
            }
        }
    }
}
