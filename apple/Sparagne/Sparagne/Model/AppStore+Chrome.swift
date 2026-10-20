import Foundation
import SparagneCore

/// What the window's chrome reads off the store, and the reveals the RIEPILOGO asks for.
extension AppStore {
    /// Every period waiting for a decision, over all the templates: a
    /// template three months behind counts three times, since each period is
    /// confirmed or skipped on its own. None on a vault the account only
    /// reads, where nothing can be confirmed or skipped.
    var actionableDueCount: Int {
        isReadOnly ? 0 : pendingRecurringItems.reduce(0) { $0 + $1.due.count }
    }

    /// A row of the RIEPILOGO was clicked, one of its top expenses or the
    /// strip's first row without a category: the Mastro on the row's month,
    /// the row selected and scrolled into view once (`revealedRow`, which
    /// the grid consumes).
    ///
    /// The list has to be one the row is in: `direction`, USCITE for an
    /// expense or a refund and ENTRATE for an income, with nothing searched.
    /// PERSONA stays as it is, since the RIEPILOGO was read with it. The
    /// selection waits for the reload the month or the direction queue, and
    /// `reload()` makes it; on a vault only read there is none, and the grid
    /// only scrolls.
    ///
    /// The list is set before the tab and the id: the ledger must not come
    /// up on the old month's rows with the row already asked for.
    func reveal(transactionId: Uuid, occurredAt: OffsetDateTime, direction list: LedgerDirection = .expenses) {
        guard let date = CoreDate.date(occurredAt) else { return }
        let target = MonthKey(date)
        let queuesReload = month != target || direction != list
        let searched = !searchText.isEmpty
        month = target
        direction = list
        searchText = ""
        revealedRow = transactionId
        tab = .ledger
        guard !queuesReload else { return }
        // The month on screen already: a search narrowed it, and the list
        // has to come back whole; otherwise the row is there now, or not
        // in this list at all (voided, past the first page).
        if searched {
            scheduleReload()
        } else if rows.contains(where: { $0.id == transactionId }) {
            selectRevealedRow()
        } else {
            revealedRow = nil
        }
    }

    /// A chip of the RIEPILOGO's strip was clicked: the tab that deals with
    /// it, and for the rows without a category the first of them, since the
    /// Mastro on its own shows one month and one direction, rarely theirs.
    func open(_ item: SummaryTodoItem) async {
        switch item.kind {
        case .uncategorized: await revealFirstUncategorized()
        case .dueRecurring, .pendingAllocation: tab = item.tab
        }
    }

    /// The oldest row without a category among those the strip counted: the
    /// year from January to the end of the month on screen, incomes,
    /// expenses and refunds, not voided, under PERSONA (`SummaryTodo`).
    /// Revealed in its own list, so an income opens ENTRATE. Once it has a
    /// category the chip counts one less and its next click goes to the next
    /// one. None left, a sync having filed it since the count: the Mastro as
    /// it is.
    func revealFirstUncategorized() async {
        guard let vault = currentVault else { return }
        let filter = TransactionFilter(
            from: CoreDate.utcString(MonthKey(year: month.year, month: 1).start()),
            to: month.bounds().to,
            kinds: [.income, .expense, .refund],
            person: person,
            ascending: true
        )
        do {
            var cursor: String?
            repeat {
                let page = try await core.transactions(
                    vaultId: vault.id,
                    filter: filter,
                    limit: Self.pageSize,
                    cursor: cursor
                )
                if let row = page.items.first(where: { TransactionRow.isUncategorized($0.category, isSystem: $0.categoryIsSystem) }) {
                    reveal(
                        transactionId: row.id,
                        occurredAt: row.occurredAt,
                        direction: row.kind == .income ? .income : .expenses
                    )
                    return
                }
                cursor = page.nextCursor
            } while cursor != nil
        } catch {
            report(error)
            return
        }
        tab = .ledger
    }
}
