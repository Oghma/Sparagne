import Foundation
import SparagneCore

/// A capped envelope as the RIEPILOGO's fund cards show it: how full it is
/// against its cap.
struct FundGauge: Identifiable, Hashable, Sendable {
    let id: Uuid
    let name: String
    let cap: Int64
    /// The balance for a net cap, the cumulative income for an income cap.
    let filled: Int64
    /// What the cap is measured on, named in the card's tag.
    var kind = Kind.balance

    /// `FlowMode` without its amount, for the modes that have a cap.
    enum Kind: Hashable, Sendable {
        /// The cap is on the balance: spending frees room.
        case balance
        /// The cap is on the cumulative income: spending does not.
        case income

        /// `nil` for an envelope without a cap.
        init?(_ mode: FlowMode) {
            switch mode {
            case .unlimited: return nil
            case .netCapped: self = .balance
            case .incomeCapped: self = .income
            }
        }
    }

    /// 0...1.
    var fraction: Double {
        guard cap > 0 else { return 0 }
        return min(max(Double(filled) / Double(cap), 0), 1)
    }
}

extension FundGauge {
    /// An envelope's gauge, `nil` without a cap. Spending does not free room
    /// on an income cap, so what fills that bar is the cumulative income.
    init?(id: Uuid, name: String, mode: FlowMode, balance: Int64, incomeTotal: Int64?) {
        switch mode {
        case .unlimited: return nil
        case .netCapped(let cap): self.init(id: id, name: name, cap: cap, filled: balance, kind: .balance)
        case .incomeCapped(let cap): self.init(id: id, name: name, cap: cap, filled: incomeTotal ?? balance, kind: .income)
        }
    }
}

/// One person's share of one month, the four figures the table and the
/// per-person card read.
struct PersonMonth: Hashable, Sendable {
    /// Income other than opening balances.
    let income: Int64
    /// Net expense on the envelopes without a cap.
    let cashExpense: Int64
    /// Net expense on the envelopes with a cap.
    let fundExpense: Int64
    /// The person's share of TOTALE at the end of the month.
    let total: Int64

    var savings: Int64 { income - cashExpense }
}

/// One line of the RIEPILOGO's month table, everybody
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
    /// Each person's figures, in `YearSummary.people` order.
    let byPerson: [PersonMonth]

    /// RISPARMIO.
    var savings: Int64 { income - cashExpense }

    /// The same TOTALE per person, in `YearSummary.people` order.
    var totalByPerson: [Int64] { byPerson.map(\.total) }

    /// TASSO: savings over income, `nil` with no income. The rule of
    /// `MonthKPIs.rate`, so a card and its row print the same percent; a
    /// month that spent more than it earned has a negative one.
    var rate: Double? {
        income > 0 ? Double(savings) / Double(income) : nil
    }
}

/// Where the year would end at the pace of its finished months: only for
/// the month of today, since a month gone by is a fact, not a forecast.
struct Projection: Hashable, Sendable {
    /// The finished months the pace is read from.
    let finishedMonths: Int
    /// What the finished months earned less everything they spent, cash and
    /// funds, over `finishedMonths`: the move of TOTALE without the wallets
    /// opened meanwhile, which are money found, not money saved.
    let monthlyGrowth: Int64
    /// That TOTALE plus `monthlyGrowth` for every month still to come.
    let yearEnd: Int64
    /// Over the same finished months.
    let averageSavings: Int64
    let averageExpense: Int64
}

/// One person's year so far: the "Per persona" card.
struct PersonYear: Hashable, Sendable {
    let person: String
    /// Income without the opening balances.
    let income: Int64
    /// Net expense on the envelopes without a cap, like USCITE.
    let expenses: Int64
    /// `closing - initial`: how much the person's share of the fund moved.
    let growth: Int64

    var savings: Int64 { income - expenses }

    /// `nil` with no income.
    var rate: Double? { income > 0 ? Double(savings) / Double(income) : nil }
}

/// One wallet's column of the month table: its balance at the end of every
/// month. Drawn only when the vault has two or more wallets in the year.
struct WalletYearColumn: Identifiable, Hashable, Sendable {
    let id: Uuid
    let name: String
    /// The balance the year starts with.
    let initial: Int64
    /// The balance at the end of each month, January first, always twelve.
    /// A month after the one on screen carries the running sum, and the
    /// table draws it as a dash anyway.
    let balances: [Int64]
}

/// One line of the RIEPILOGO's table. A `nil` figure is an empty cell: the
/// opening row has only TOTALE, the sum has no FONDO CASSA, a future month
/// has nothing at all.
struct YearTableRow: Hashable, Sendable {
    enum Kind: Hashable, Sendable {
        /// "Inizio anno": the fund the year starts with.
        case opening
        case month(MonthKey)
        /// The year's sums.
        case sum
    }

    let kind: Kind
    /// After the month on screen: dashes.
    let isFuture: Bool
    /// The month on screen, in the year on screen.
    let isCurrent: Bool
    /// TOTALE did not go down on the line before (January is compared with
    /// the opening fund). Meaningless on a blank row.
    let trendUp: Bool
    let income: Int64?
    let cashExpense: Int64?
    let savings: Int64?
    /// TASSO: `nil` without income, and on the opening row.
    let rate: Double?
    let carried: Int64?
    let fundExpense: Int64?
    let total: Int64?
    /// In `YearSummary.walletColumns` order; empty on a blank row, and
    /// always with fewer than two wallets.
    let byWallet: [Int64]
    /// In `YearSummary.people` order; empty on a blank row.
    let totalByPerson: [Int64]
}

/// The four numbers of the month on screen, with the
/// month before and the year so far for their sub-lines. Plain figures, so
/// the deltas the cards print are checked without a window.
///
/// They come from the same `YearMonth`s as the table under them, so a card
/// and its row never disagree: expenses are USCITE (envelopes without a cap),
/// not the Mastro's net expense on every envelope.
struct MonthKPIs: Hashable, Sendable {
    let income: Int64
    let expenses: Int64
    /// The month before, `nil` for January: what precedes it is the opening
    /// balance (bucket 0), which is everything before the year, not a month
    /// that could be compared.
    let previous: Previous?
    /// Income and savings of the year up to and including this month.
    let yearIncome: Int64
    let yearSavings: Int64

    struct Previous: Hashable, Sendable {
        let income: Int64
        let expenses: Int64
        var savings: Int64 { income - expenses }
    }

    var savings: Int64 { income - expenses }

    /// Savings against the month before, in money rather than percent: the
    /// sign decides the arrow and its color. `nil` without a month before.
    var savingsDelta: Int64? { previous.map { savings - $0.savings } }

    /// This month's savings over its income, `nil` with no income.
    var rate: Double? {
        income > 0 ? Double(savings) / Double(income) : nil
    }

    /// The year's rate, `nil` with no income yet.
    var yearRate: Double? {
        yearIncome > 0 ? Double(yearSavings) / Double(yearIncome) : nil
    }
}

/// Everything the RIEPILOGO draws for one year, up to the month on screen.
struct YearSummary: Sendable {
    let year: Int
    /// The month on screen: decides the year and which months are future.
    let upTo: MonthKey
    /// The month today falls in, `nil` when nobody said: decides whether the
    /// month on screen is still going (`isLive`) and which months are over
    /// (`finished`).
    let today: MonthKey?
    /// The PERSONA filter the rows were read with; `nil` is everybody. With
    /// one, `people` is just that person.
    let person: String?
    /// One column per person the vault's rows are for (`transactions.person`):
    /// a row recorded for Elisa counts in her column, not in the recorder's.
    let people: [String]
    /// FONDO CASSA INIZIALE per person: everything before January.
    let initialByPerson: [Int64]
    /// January first, always twelve.
    let months: [YearMonth]
    /// The active capped envelopes, in the vault's order.
    let funds: [FundGauge]
    /// One column per wallet with a balance or a movement in the year, in
    /// the vault's order; empty with fewer than two, when the TOTALE column
    /// already says it all, and under the PERSONA filter: a wallet is the
    /// family's, and its balance would not add up to one person's TOTALE.
    let walletColumns: [WalletYearColumn]

    var initial: Int64 { initialByPerson.reduce(0, +) }

    /// The months up to the one on screen.
    var elapsed: [YearMonth] { months.filter { !$0.isFuture } }

    /// The month on screen is the one today falls in: still going, so its
    /// segment is dashed and the year can be projected from it.
    var isLive: Bool { today == upTo }

    /// The elapsed months that are over: the ones before the month of today
    /// when it is known, so neither the live month nor a month after it
    /// (December looked at in October, empty so far) weighs on an average;
    /// every elapsed month when nobody said what today is. What averages
    /// and projections are read from.
    var finished: [YearMonth] {
        guard let today else { return elapsed }
        return elapsed.filter { ($0.month.year, $0.month.month) < (today.year, today.month) }
    }

    /// Where the year would end at the pace of the finished months; `nil`
    /// unless the month on screen is live and a month has finished before it.
    ///
    /// The pace is what the months earned less what they spent, so a wallet
    /// opened in January is not read as a saving to repeat every month. It
    /// is whole minor units: the division rounds toward zero, so a
    /// projection never promises the cent a month the arithmetic cannot
    /// split. The months still to come include the live one, which has not
    /// finished either.
    var projection: Projection? {
        guard isLive else { return nil }
        let finished = self.finished
        guard let last = finished.last else { return nil }
        let count = Int64(finished.count)
        let monthlyGrowth = finished.reduce(0) { $0 + $1.income - $1.cashExpense - $1.fundExpense } / count
        return Projection(
            finishedMonths: finished.count,
            monthlyGrowth: monthlyGrowth,
            yearEnd: last.total + monthlyGrowth * (12 - count),
            averageSavings: finished.reduce(0) { $0 + $1.savings } / count,
            averageExpense: finished.reduce(0) { $0 + $1.cashExpense } / count
        )
    }

    /// Each person's year so far, in `people` order: the same months the
    /// table sums, so the card's expenses are the person's share of USCITE
    /// and its growth the move of their share of TOTALE.
    var personFigures: [PersonYear] {
        let elapsed = self.elapsed
        let closing = closingByPerson
        return people.enumerated().map { index, name in
            PersonYear(
                person: name,
                income: elapsed.reduce(0) { $0 + $1.byPerson[index].income },
                expenses: elapsed.reduce(0) { $0 + $1.byPerson[index].cashExpense },
                growth: closing[index] - initialByPerson[index]
            )
        }
    }

    /// The year's flows so far: income, expenses and savings add up over a
    /// year, unlike the running balances.
    var yearIncome: Int64 { elapsed.reduce(0) { $0 + $1.income } }
    var yearCashExpense: Int64 { elapsed.reduce(0) { $0 + $1.cashExpense } }
    var yearFundExpense: Int64 { elapsed.reduce(0) { $0 + $1.fundExpense } }
    var yearSavings: Int64 { yearIncome - yearCashExpense }

    /// The wallets' balance at the end of the last month drawn: the opening
    /// fund while no month has passed.
    var closingTotal: Int64 { elapsed.last?.total ?? initial }

    /// The same, per person.
    var closingByPerson: [Int64] { elapsed.last?.totalByPerson ?? initialByPerson }

    /// How much the cash fund grew since the start of the year.
    var growth: Int64 { closingTotal - initial }

    /// The savings rate of the year so far, `nil` with no income.
    var yearRate: Double? {
        guard yearIncome > 0 else { return nil }
        return Double(yearSavings) / Double(yearIncome)
    }

    /// The cards of the month on screen, read from the table's own months so
    /// both count USCITE the same way. The month is the last one drawn, which
    /// is `upTo` in its year and December in a past one; `nil` in a year
    /// still to come.
    var kpis: MonthKPIs? {
        let elapsed = self.elapsed
        guard let month = elapsed.last else { return nil }
        let before = elapsed.dropLast().last
        return MonthKPIs(
            income: month.income,
            expenses: month.cashExpense,
            previous: before.map { .init(income: $0.income, expenses: $0.cashExpense) },
            yearIncome: yearIncome,
            yearSavings: yearSavings
        )
    }

    /// The RIEPILOGO's month table, top to bottom: the opening cash
    /// fund, the twelve months, the year's sum. Every decision the table
    /// draws (which row is on screen, which is blank, which went down) is
    /// made here so it can be tested.
    var tableRows: [YearTableRow] {
        var rows = [
            YearTableRow(
                kind: .opening, isFuture: false, isCurrent: false, trendUp: true,
                income: nil, cashExpense: nil, savings: nil, rate: nil, carried: nil, fundExpense: nil,
                total: initial, byWallet: walletColumns.map(\.initial), totalByPerson: initialByPerson
            )
        ]
        var previous = initial
        for (index, month) in months.enumerated() {
            if month.isFuture {
                rows.append(
                    YearTableRow(
                        kind: .month(month.month), isFuture: true, isCurrent: false, trendUp: false,
                        income: nil, cashExpense: nil, savings: nil, rate: nil, carried: nil, fundExpense: nil,
                        total: nil, byWallet: [], totalByPerson: []
                    )
                )
                continue
            }
            rows.append(
                YearTableRow(
                    kind: .month(month.month), isFuture: false, isCurrent: month.month == upTo,
                    trendUp: month.total >= previous,
                    income: month.income, cashExpense: month.cashExpense, savings: month.savings,
                    rate: month.rate, carried: month.carried, fundExpense: month.fundExpense,
                    total: month.total, byWallet: walletColumns.map { $0.balances[index] },
                    totalByPerson: month.totalByPerson
                )
            )
            previous = month.total
        }
        // FONDO CASSA is a running balance, so the sum has none; TOTALE is
        // where the year ended up, and so is each wallet.
        let lastIndex = elapsed.count - 1
        rows.append(
            YearTableRow(
                kind: .sum, isFuture: false, isCurrent: false, trendUp: closingTotal >= initial,
                income: yearIncome, cashExpense: yearCashExpense, savings: yearSavings,
                rate: yearRate, carried: nil, fundExpense: yearFundExpense,
                total: closingTotal,
                byWallet: walletColumns.map { lastIndex >= 0 ? $0.balances[lastIndex] : $0.initial },
                totalByPerson: closingByPerson
            )
        )
        return rows
    }

    /// Pure arithmetic over the query's rows and the vault's envelopes, so it
    /// can be tested without a database. In `rows`, the core's
    /// `year_breakdown`, bucket 0 is everything before January and 1...12
    /// the months.
    ///
    /// `vaultPeople` are the persons of every live row of the vault
    /// (`AppStore.peopleInRows`): each gets a column in every year, so a year
    /// before someone's first row, or one viewed at a month before it, has
    /// the same columns as the rest, at zero.
    ///
    /// `person` is the PERSONA filter: the rows of anybody else are left out
    /// before anything is summed, and the one column is theirs. `today` is
    /// the month today falls in (`isLive`). `wallets` and `walletBalances`
    /// (the core's `balance_breakdown` on the wallets, over the same bounds
    /// as `rows`) give the wallet columns.
    static func build(
        year: Int,
        upTo: MonthKey,
        rows: [BucketPersonTotals],
        flows: [FlowView],
        vaultPeople: [String] = [],
        person: String? = nil,
        today: MonthKey? = nil,
        wallets: [WalletView] = [],
        walletBalances: [BucketTargetTotals] = []
    ) -> YearSummary {
        // Under PERSONA everybody else's rows go before anything is summed,
        // so TOTALE and FONDO CASSA are that person's share; the one column
        // is theirs even in a year with no row of theirs.
        let rows = person.map { name in rows.filter { $0.person == name } } ?? rows
        let people = person.map { [$0] } ?? Self.people(in: rows, vaultPeople: vaultPeople)
        var byBucket: [Int: [BucketPersonTotals]] = [:]
        for row in rows { byBucket[Int(row.bucket), default: []].append(row) }

        // Bucket 0 is everything before January: it opens the recurrence.
        let initialByPerson = people.map { name in
            Self.closing(byBucket[0, default: []].filter { $0.person == name })
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

            var byPerson: [PersonMonth] = []
            byPerson.reserveCapacity(people.count)
            for (index, name) in people.enumerated() {
                let personRows = monthRows.filter { $0.person == name }
                let personIncome = personRows.reduce(0) { $0 + $1.income }
                let personCash = personRows.reduce(0) { $0 + $1.cashExpense }
                let personFunds = personRows.reduce(0) { $0 + $1.fundExpense }
                let personCarried = runningByPerson[index] + personRows.reduce(0) { $0 + $1.opening }
                byPerson.append(
                    PersonMonth(
                        income: personIncome,
                        cashExpense: personCash,
                        fundExpense: personFunds,
                        total: (personIncome - personCash) + personCarried - personFunds
                    )
                )
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
                    byPerson: byPerson
                )
            )
            runningTotal = total
            runningByPerson = byPerson.map(\.total)
        }

        return YearSummary(
            year: year,
            upTo: upTo,
            today: today,
            person: person,
            people: people,
            initialByPerson: initialByPerson,
            months: months,
            funds: Self.funds(in: flows),
            walletColumns: person == nil
                ? Self.walletColumns(
                    wallets: wallets,
                    deltas: walletBalances,
                    through: months.filter { !$0.isFuture }.count
                )
                : []
        )
    }

    /// One column per wallet that moved or held money in the year, in the
    /// order of `wallets`; none with fewer than two, when TOTALE already is
    /// the one wallet's balance.
    ///
    /// The deltas are on the same bounds as the year's rows, so bucket 0 is
    /// the balance the year starts with and every month adds its own: the
    /// running sum is the balance at the end of each month, transfers
    /// between wallets included. A delta for a wallet the vault no longer
    /// lists, or in a bucket past December, is left out.
    ///
    /// Whether a wallet has a column is read up to `through`, the last month
    /// drawn: one opened in November is not there yet with March on screen,
    /// and would be a column of zeros.
    private static func walletColumns(
        wallets: [WalletView],
        deltas: [BucketTargetTotals],
        through: Int
    ) -> [WalletYearColumn] {
        var buckets: [Uuid: [Int64]] = [:]
        var inYear = Set<Uuid>()
        for delta in deltas where delta.bucket <= 12 {
            let bucket = Int(delta.bucket)
            buckets[delta.targetId, default: Array(repeating: 0, count: 13)][bucket] += delta.delta
            // A wallet closed before the year comes back with legs that net
            // to nothing: it has no column.
            if delta.delta != 0, bucket <= through { inYear.insert(delta.targetId) }
        }
        let shown = wallets.filter { inYear.contains($0.id) }
        guard shown.count >= 2 else { return [] }
        return shown.map { wallet in
            let slots = buckets[wallet.id] ?? Array(repeating: 0, count: 13)
            var running = slots[0]
            let balances = slots[1...].map { delta in
                running += delta
                return running
            }
            return WalletYearColumn(id: wallet.id, name: wallet.name, initial: slots[0], balances: balances)
        }
    }

    /// One column per person, de-duplicated in the core's order and then
    /// sorted case-insensitively, so two runs of the same vault always put
    /// the columns in the same place.
    private static func people(in rows: [BucketPersonTotals], vaultPeople: [String]) -> [String] {
        var seen = Set<String>()
        var people: [String] = []
        for person in vaultPeople + rows.map(\.person) where seen.insert(person).inserted {
            people.append(person)
        }
        return people.sorted { $0.lowercased() < $1.lowercased() }
    }

    /// What a bucket leaves behind: everything that came in, less everything
    /// that went out, on both kinds of envelope.
    private static func closing(_ rows: [BucketPersonTotals]) -> Int64 {
        rows.reduce(0) { $0 + $1.income + $1.opening - $1.cashExpense - $1.fundExpense }
    }

    /// Blank rows: the months after the one on screen, in the year on screen.
    /// A later year is all future, an earlier one all past.
    private static func isFuture(year: Int, month: Int, upTo: MonthKey) -> Bool {
        year > upTo.year || (year == upTo.year && month > upTo.month)
    }

    /// A "fondo" is an envelope with a cap, which makes the classification a
    /// fact of the vault rather than a name.
    private static func funds(in flows: [FlowView]) -> [FundGauge] {
        flows.compactMap { flow in
            flow.archived
                ? nil
                : FundGauge(id: flow.id, name: flow.name, mode: flow.mode, balance: flow.balance, incomeTotal: flow.incomeTotal)
        }
    }
}
