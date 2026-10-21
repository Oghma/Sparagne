import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// `AppStore` driven end to end over an in-memory core: every assertion is
/// about what the projection says after a real command was applied.
struct AppStoreTests {
    /// A store that has been through onboarding: vault `Main`, wallet `Cash`
    /// holding 100.00.
    private static func onboarded() async throws -> AppStore {
        try await TestStore.make(vault: "Main", wallet: "Cash", opening: 10_000, author: "tester", undoWindow: .seconds(60))
    }

    private static func pizzaRow(_ store: AppStore) throws -> TransactionRow {
        try #require(store.rows.first { $0.note == "pizza" })
    }

    /// A rent of 500,00 on the 1st of every month, started a month ago, so a period is due.
    private static func rentSinceAMonthAgo(_ store: AppStore) async throws {
        let monthAgo = try #require(Calendar.current.date(byAdding: .month, value: -1, to: Date()))
        await store.createRecurring(
            kind: .expense,
            amount: 50_000,
            walletId: nil,
            flowId: nil,
            category: "Casa",
            note: "affitto",
            schedule: RecurringFixture.schedule(.monthly(day: 1), from: CoreDate.day(monthAgo))
        )
    }

    @Test("An empty database asks for onboarding, which creates the vault and its wallet")
    func onboarding() async throws {
        let store = try TestStore.fresh(author: "tester", undoWindow: .seconds(60))

        await store.bootstrap()
        #expect(store.needsOnboarding)
        #expect(store.currentVault == nil)

        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 10000)

        #expect(store.presentedError == nil)
        #expect(!store.needsOnboarding)
        #expect(store.currentVault?.name == "Main")
        #expect(store.vaults.count == 1)

        let wallet = try #require(store.wallets.first)
        #expect(wallet.name == "Cash")
        #expect(wallet.balance == 10000)

        // The opening balance is a real transaction, not a special case. The
        // ledger opens on ENTRATE or USCITE, never both, so ask for income.
        store.direction = .income
        await store.settle()
        #expect(store.rows.count == 1)
        #expect(store.rows.first?.kind == .income)
    }

    @Test("A quick-add line adds a row and moves the wallet balance")
    func quickAddLowersTheWalletBalance() async throws {
        let store = try await Self.onboarded()

        await store.submit(quickAdd: "-12.50 pizza #food")

        #expect(store.presentedError == nil)
        #expect(store.quickAddText.isEmpty)

        let row = try Self.pizzaRow(store)
        #expect(row.kind == .expense)
        #expect(row.absoluteAmount == 1250)
        #expect(row.category == "food")
        #expect(row.walletDisplay == "Cash")

        #expect(store.wallets.first?.balance == 8750)
    }

    @Test("Void hides the row, and undo puts it back without executing anything")
    func undoCancelsThePendingVoid() async throws {
        let store = try await Self.onboarded()
        await store.submit(quickAdd: "-12.50 pizza #food")
        let row = try Self.pizzaRow(store)

        await store.void(transactionId: row.id)
        #expect(store.pendingUndo?.ids == [row.id])
        #expect(store.pendingUndo?.vaultId == store.currentVault?.id)
        #expect(!store.rows.contains { $0.id == row.id })
        // Nothing was sent yet: the balance has not moved back.
        #expect(store.wallets.first?.balance == 8750)

        store.undo()
        #expect(store.pendingUndo == nil)
        #expect(store.rows.contains { $0.id == row.id })

        // And the core still has it as a live transaction.
        await store.reload()
        let reloaded = try Self.pizzaRow(store)
        #expect(!reloaded.voided)
        #expect(store.wallets.first?.balance == 8750)
    }

    @Test("Letting the undo window elapse voids the transaction for real")
    func elapsedUndoWindowVoids() async throws {
        // The window is driven by the injected sleeper, so the test never waits.
        let store = try await TestStore.make(
            vault: "Main", wallet: "Cash", opening: 10_000, author: "tester", undoWindow: .zero, sleeper: { _ in }
        )
        await store.submit(quickAdd: "-12.50 pizza #food")
        let row = try Self.pizzaRow(store)

        await store.void(transactionId: row.id)
        let task = store.undoTask
        await task?.value

        #expect(store.presentedError == nil)
        #expect(store.pendingUndo == nil)
        // The void gave the money back to the wallet.
        #expect(store.wallets.first?.balance == 10000)
        #expect(!store.rows.contains { $0.id == row.id })

        store.showVoided = true
        await store.settle()
        let voided = try #require(store.allRows.first { $0.id == row.id })
        #expect(voided.voided)
    }

    // MARK: - The void waiting on the toast, when the window moves under it

    /// `Main` with a pizza waiting on the toast, and `Casa` beside it.
    private static func voidInMainBesideCasa() async throws -> (store: AppStore, main: VaultView, casa: VaultView) {
        let store = try await TestStore.make(opening: 5_000, author: "tester", undoWindow: .seconds(60))
        let casa = try #require(store.currentVault)
        await store.createVault(name: "Main", walletName: "Cash", openingBalance: 10_000)
        let main = try #require(store.currentVault)
        await store.submit(quickAdd: "-12.50 pizza #food")
        await store.void(transactionId: try pizzaRow(store).id)
        #expect(store.pendingUndo?.vaultId == main.id)
        return (store, main, casa)
    }

    @Test("A pull that deletes the vault a void waits in drops the void, with no alert and no toast left")
    func pulledDeletionDropsThePendingVoid() async throws {
        let (store, main, casa) = try await Self.voidInMainBesideCasa()

        // What a pull does: the vault goes from the core, then the window
        // re-reads the vault list.
        try await store.core.execute(vaultId: main.id, .deleteVault)
        await store.refreshAfterSync()

        #expect(store.presentedError == nil)
        #expect(store.pendingUndo == nil)
        #expect(store.currentVault?.id == casa.id)
        // Casa was not touched by a void meant for Main.
        #expect(store.wallets.first?.balance == 5_000)
    }

    @Test("A pull that deletes the last vault does not leave the toast stuck")
    func pulledDeletionOfTheLastVaultClearsTheToast() async throws {
        let store = try await Self.onboarded()
        await store.submit(quickAdd: "-12.50 pizza #food")
        await store.void(transactionId: try Self.pizzaRow(store).id)
        let vault = try #require(store.currentVault)

        try await store.core.execute(vaultId: vault.id, .deleteVault)
        await store.refreshAfterSync()

        #expect(store.presentedError == nil)
        #expect(store.pendingUndo == nil)
        #expect(store.currentVault == nil)
        #expect(store.needsOnboarding)
    }

    @Test("A void whose vault is gone is dropped by the flush itself, silently")
    func flushingIntoAGoneVaultIsSilent() async throws {
        let (store, main, _) = try await Self.voidInMainBesideCasa()

        // The undo window elapses before the store has heard of the deletion.
        try await store.core.execute(vaultId: main.id, .deleteVault)
        await store.flushPendingUndo()

        #expect(store.presentedError == nil)
        #expect(store.pendingUndo == nil)
    }

    @Test("Switching vault applies the void in the vault it was made in")
    func switchingVaultVoidsInTheRightVault() async throws {
        let (store, main, casa) = try await Self.voidInMainBesideCasa()

        await store.select(casa)

        #expect(store.presentedError == nil)
        #expect(store.pendingUndo == nil)
        #expect(store.currentVault?.id == casa.id)
        #expect(store.wallets.first?.balance == 5_000)
        // The pizza is voided in Main, whose wallet has its 12.50 back.
        let snapshot = try await store.core.snapshot(vaultId: main.id)
        #expect(snapshot.wallets.first?.balance == 10_000)
    }

    @Test("Quitting with a void on the toast writes it before the app goes")
    func quittingFlushesThePendingVoid() async throws {
        let store = try await Self.onboarded()
        await store.submit(quickAdd: "-12.50 pizza #food")
        await store.void(transactionId: try Self.pizzaRow(store).id)
        let quit = QuitHarness(store: store)

        #expect(await quit.deferredReply())
        #expect(store.presentedError == nil)
        #expect(store.pendingUndo == nil)
        #expect(store.wallets.first?.balance == 10_000)

        // Nothing waiting: the app quits straight away.
        #expect(quit.answer() == .terminateNow)
    }

    @Test("An empty patch is not sent")
    func emptyPatchDoesNothing() async throws {
        let store = try await Self.onboarded()
        await store.submit(quickAdd: "-12.50 pizza #food")
        let row = try Self.pizzaRow(store)

        await store.update(transactionId: row.id, patch: TransactionPatch())

        #expect(store.presentedError == nil)
        #expect(store.wallets.first?.balance == 8750)
    }

    @Test("An unknown envelope name is a quick-add error, not a domain error")
    func unknownNameIsReported() async throws {
        let store = try await Self.onboarded()

        await store.submit(quickAdd: "-5.00 hotel >nowhere")

        let error = try #require(store.presentedError)
        #expect(error.code == "unknown_name")
    }

    @Test("The preview parses without touching the database")
    func previewParses() async throws {
        let store = try await Self.onboarded()

        guard case .success(let parsed) = store.preview(quickAdd: "-12.50 pizza #food @cash") else {
            Issue.record("expected the line to parse")
            return
        }
        guard case .entry(let kind, let amount, let note, _, _, _, _, _) = parsed else {
            Issue.record("expected an entry")
            return
        }
        #expect(kind == .expense)
        #expect(amount == 1250)
        #expect(note == "pizza")

        let tokens = QuickAddTokens.make(parsed, currency: .eur)
        #expect(tokens.contains { $0.role == .category && $0.value == "food" })
        #expect(tokens.contains { $0.role == .wallet && $0.value == "cash" })

        guard case .failure(let error) = store.preview(quickAdd: "pizza") else {
            Issue.record("expected a parse failure")
            return
        }
        #expect(error.code == "invalid_amount")
        #expect(store.presentedError == nil)
    }

    @Test("An ambiguous wallet name surfaces the candidates to choose from")
    func ambiguousNameOffersTheCandidates() async throws {
        let store = try await Self.onboarded()
        await store.createWallet(name: "Bank", openingBalance: 0)
        await store.createWallet(name: "Bancoposta", openingBalance: 0)
        #expect(store.presentedError == nil)

        await store.submit(quickAdd: "-5.00 hotel @ban")

        let error = try #require(store.presentedError)
        #expect(error.code == "ambiguous_name")
        #expect(error.candidates.sorted() == ["Bancoposta", "Bank"])
        #expect(error.message.contains("Bancoposta"))
        // Nothing was written.
        #expect(!store.rows.contains { $0.note == "hotel" })
    }

    @Test("A row exposes the ids of its wallet and envelope")
    func rowExposesItsIds() async throws {
        let store = try await Self.onboarded()
        await store.createEnvelope(name: "Spesa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let envelope = try #require(store.flows.first { $0.name == "Spesa" })

        await store.submit(quickAdd: "-12.50 pizza #food >Spesa")

        #expect(store.presentedError == nil)
        let row = try Self.pizzaRow(store)
        #expect(row.walletId == store.wallets.first?.id)
        #expect(row.flowId == envelope.id)
        #expect(row.envelopeDisplay == "Spesa")

        // The opening balance carries the system category, localized here.
        store.direction = .income
        await store.settle()
        let opening = try #require(store.rows.first { $0.kind == .income })
        #expect(opening.category == String(localized: "Opening"))
    }

    @Test("A wallet transfer shows both ends in the wallet column")
    func transferRowShowsBothEnds() async throws {
        let store = try await Self.onboarded()
        await store.createWallet(name: "Bank", openingBalance: 0)

        await store.submit(quickAdd: "tw> 20.00 @cash @bank")

        #expect(store.presentedError == nil)
        // Transfers are in neither direction; the View menu opts into them.
        store.showTransfers = true
        await store.settle()
        let row = try #require(store.rows.first { $0.kind == .transferWallet })
        #expect(row.walletDisplay == "Cash → Bank")
        #expect(row.envelopeDisplay == TransactionRow.placeholder)
        #expect(row.isTransfer)
        #expect(row.walletId == store.wallets.first { $0.name == "Cash" }?.id)
        #expect(row.flowId == nil)
        #expect(store.wallets.first { $0.name == "Bank" }?.balance == 2000)
    }

    // MARK: - Wallet and envelope management

    @Test("A setup command that goes through moves the status line's saved-at; one refused, a wallet with a balance archived, does not")
    func setupCommandMovesSavedAt() async throws {
        let store = try await Self.onboarded()
        let cash = try #require(store.wallets.first { $0.name == "Cash" })
        #expect(store.savedAt == nil)

        await store.archiveWallet(cash.id)
        #expect(store.presentedError?.code == "invalid_command")
        #expect(store.wallets.contains { $0.id == cash.id })
        #expect(store.savedAt == nil)

        store.presentedError = nil
        await store.createWallet(name: "Card", openingBalance: 0)
        #expect(store.presentedError == nil)
        #expect(store.savedAt != nil)
    }

    @Test("Updating an envelope's cap shows up in the snapshot")
    func updatingAnEnvelopeChangesTheCapInTheSnapshot() async throws {
        let store = try await Self.onboarded()
        await store.createEnvelope(name: "Vacanze", mode: .unlimited, allowNegative: false, openingAllocation: 0)
        let envelope = try #require(store.flows.first { $0.name == "Vacanze" })

        await store.updateEnvelope(envelope.id, mode: .netCapped(cap: 5000))

        #expect(store.presentedError == nil)
        let updated = try #require(store.flows.first { $0.id == envelope.id })
        #expect(updated.mode == .netCapped(cap: 5000))
    }

    // MARK: - Category management

    @Test("Renaming a category shows up in the categories list")
    func renamingACategoryShowsInCategories() async throws {
        let store = try await Self.onboarded()
        await store.submit(quickAdd: "-5.00 pizza #Food")
        #expect(store.presentedError == nil)
        await store.loadCategoryManagement()
        let food = try #require(store.windowCategories.first { $0.name == "Food" })

        await store.renameCategory(food.id, name: "Groceries")

        #expect(store.presentedError == nil)
        #expect(store.categories.contains { $0.name == "Groceries" })
        #expect(store.windowCategories.contains { $0.id == food.id && $0.name == "Groceries" })
    }

    @Test("A merge with preview conflicts is refused; a clean merge repoints transactions")
    func categoryMergePreviewAndCleanMerge() async throws {
        let store = try await Self.onboarded()
        await store.submit(quickAdd: "-5.00 pizza #Food")
        #expect(store.presentedError == nil)
        await store.loadCategoryManagement()
        let food = try #require(store.windowCategories.first { $0.name == "Food" })

        // Merging a category into itself always conflicts.
        let selfPreview = await store.previewCategoryMerge(sourceId: food.id, targetId: food.id)
        #expect(selfPreview?.ok == false)
        #expect(selfPreview?.conflicts.contains { $0.kind == .sameCategory } == true)

        await store.mergeCategory(sourceId: food.id, targetId: food.id)
        #expect(store.presentedError != nil)
        store.presentedError = nil

        // A fresh, unrelated target has no conflicts and the merge repoints
        // the existing transaction.
        await store.createCategory(name: "Groceries")
        await store.loadCategoryManagement()
        let groceries = try #require(store.windowCategories.first { $0.name == "Groceries" })

        let cleanPreview = await store.previewCategoryMerge(sourceId: food.id, targetId: groceries.id)
        #expect(cleanPreview?.ok == true)
        #expect(cleanPreview?.conflicts.isEmpty == true)

        await store.mergeCategory(sourceId: food.id, targetId: groceries.id)

        #expect(store.presentedError == nil)
        let pizza = try #require(store.transactions.first { $0.note == "pizza" })
        #expect(pizza.categoryId == groceries.id)
        #expect(!store.windowCategories.contains { $0.id == food.id })
    }

    // MARK: - Recurring

    @Test("A template due a month ago is pending; executing adds a transaction and clears it, skipping leaves no transaction")
    func recurringExecuteAndSkip() async throws {
        let store = try await Self.onboarded()
        let now = Date()
        let calendar = Calendar(identifier: .gregorian)
        let dayOfMonth = calendar.component(.day, from: now)
        let monthAgo = try #require(calendar.date(byAdding: .month, value: -1, to: now))
        // The executed transaction lands last month, and the ledger reads one
        // month at a time, so move the window there or `rows` will be empty.
        store.month = MonthKey(monthAgo)
        await store.settle()

        await store.createRecurring(
            kind: .expense,
            amount: 999,
            walletId: nil,
            flowId: nil,
            category: "Bills",
            note: "Rent",
            schedule: Schedule(
                frequency: .monthly(day: UInt8(dayOfMonth)),
                interval: 1,
                startDate: CoreDate.day(monthAgo),
                endDate: nil
            )
        )
        #expect(store.presentedError == nil)
        let template = try #require(store.recurringTemplates.first { $0.note == "Rent" })

        let dueDate = try #require(
            store.pendingRecurringItems.first { $0.template.id == template.id }?.due.first
        )

        await store.executeRecurring(template.id, periodDate: dueDate)

        #expect(store.presentedError == nil)
        #expect(!(store.pendingRecurringItems.first { $0.template.id == template.id }?.due.contains(dueDate) ?? false))
        let executed = try #require(store.rows.first { $0.note == "Rent" })
        #expect(executed.kind == .expense)
        #expect(executed.absoluteAmount == 999)

        // Skipping the following period (if any is already due) leaves no
        // additional transaction.
        if let nextDue = store.pendingRecurringItems.first(where: { $0.template.id == template.id })?.due.first {
            let countBefore = store.rows.filter { $0.note == "Rent" }.count
            await store.skipRecurring(template.id, periodDate: nextDue)
            #expect(store.presentedError == nil)
            #expect(store.rows.filter { $0.note == "Rent" }.count == countBefore)
        }
    }

    // MARK: - The RIEPILOGO

    @Test("The summary's categories, envelopes, top expenses and strip come from the same load as its table")
    func summaryExtras() async throws {
        let store = try await Self.onboarded()
        await store.submit(quickAdd: "-12.50 pizza #food")
        #expect(store.presentedError == nil)

        let year = try #require(store.year)
        // The month on screen is the month of today.
        #expect(year.today == store.month)
        #expect(year.isLive)
        #expect(store.yearCategories.map(\.name) == ["food"])
        #expect(store.yearCategories.first?.amount == 1250)
        #expect(store.yearCategories.first?.share == 1)
        #expect(store.categoryHeat?.rows.map(\.name) == ["food"])
        #expect(store.topExpenses.map(\.note) == ["pizza"])
        let envelopes = try #require(store.envelopeYear)
        #expect(envelopes.rows.first?.isUnallocated == true)
        #expect(envelopes.rows.count == store.flows.count)
        #expect(envelopes.rows.reduce(0) { $0 + $1.expense } == 1250)
        #expect(envelopes.people == year.people)
        // No template, no plan, every row with a category.
        #expect(store.fixedCosts == nil)
        #expect(store.summaryTodo.isEmpty)
        // The load carries every template, so the Ricorrenze tab needs no
        // visit of its own to be current.
        #expect(store.recurringTemplates.isEmpty)
    }

    @Test("A recurring expense shows in the summary's fixed costs and its due periods on the strip")
    func summaryFixedCostsAndStrip() async throws {
        let store = try await Self.onboarded()
        try await Self.rentSinceAMonthAgo(store)
        #expect(store.presentedError == nil)
        #expect(store.fixedCosts?.monthly == 50_000)
        #expect(!store.duePeriods.isEmpty)
        #expect(store.summaryTodo.first?.kind == .dueRecurring(count: store.duePeriods.count))
        #expect(store.summaryTodo.first?.tab == .recurring)

        // A click on that chip opens the Ricorrenze tab, nothing else moves.
        let month = store.month
        await store.open(try #require(store.summaryTodo.first))
        #expect(store.tab == .recurring)
        #expect(store.month == month)
    }

    @Test("The strip's rows without a category open the oldest of them in its own month and list, then the next")
    func stripOpensRowsWithoutCategory() async throws {
        let store = try await Self.onboarded()
        let calendar = Calendar.current
        let year = calendar.component(.year, from: Date())
        // Early in the year the strip counts: another month than the one on
        // screen, except in January, which still tells the lists apart.
        let enelDay = try #require(calendar.date(from: DateComponents(year: year, month: 1, day: 2, hour: 12)))
        let bookDay = try #require(calendar.date(from: DateComponents(year: year, month: 1, day: 3, hour: 12)))
        store.month = MonthKey(enelDay)
        store.direction = .income
        await store.settle()
        await store.addRow(day: enelDay, flowId: nil, category: nil, note: "Enel", amount: 12_201)
        let enel = try #require(store.transactions.first { $0.note == "Enel" })
        store.direction = .expenses
        await store.settle()
        await store.addRow(day: bookDay, flowId: nil, category: nil, note: "libro", amount: 1_800)
        await store.addRow(day: bookDay, flowId: nil, category: "food", note: "pizza", amount: 1_250)
        let book = try #require(store.transactions.first { $0.note == "libro" })
        #expect(store.presentedError == nil)

        // Where the user stood: this month, USCITE, a search, the summary.
        store.month = MonthKey(Date())
        store.searchText = "pizza"
        store.tab = .summary
        await store.settle()
        let chip = try #require(store.summaryTodo.first { $0.id == "uncategorized" })
        #expect(chip.kind == .uncategorized(count: 2))

        // The oldest is an income: ENTRATE, on its month, selected.
        await store.open(chip)
        await store.settle()
        #expect(store.tab == .ledger)
        #expect(store.month == MonthKey(enelDay))
        #expect(store.direction == .income)
        #expect(store.searchText.isEmpty)
        #expect(store.rows.contains { $0.id == enel.id })
        #expect(store.selection.ids == [enel.id])

        // Filed, the chip counts one less and opens the expense in USCITE.
        await store.setSelectionCategory("Fotovoltaico")
        store.tab = .summary
        await store.settle()
        let next = try #require(store.summaryTodo.first { $0.id == "uncategorized" })
        #expect(next.kind == .uncategorized(count: 1))
        await store.open(next)
        await store.settle()
        #expect(store.tab == .ledger)
        #expect(store.direction == .expenses)
        #expect(store.selection.ids == [book.id])
    }

    @Test("With no row left without a category, the chip opens the Mastro as it is")
    func stripWithNothingLeftToFile() async throws {
        let store = try await Self.onboarded()
        let month = store.month
        store.direction = .income
        await store.settle()

        await store.open(SummaryTodoItem(kind: .uncategorized(count: 1), tab: .ledger))

        #expect(store.tab == .ledger)
        #expect(store.month == month)
        #expect(store.direction == .income)
        #expect(store.revealedRow == nil)
        #expect(store.presentedError == nil)
    }

    @Test("Revealing a top expense opens the Mastro on its month, the row selected for the grid to scroll to")
    func revealTopExpense() async throws {
        let store = try await Self.onboarded()
        let monthAgo = try #require(Calendar.current.date(byAdding: .month, value: -1, to: Date()))
        store.month = MonthKey(monthAgo)
        await store.settle()
        await store.addRow(day: monthAgo, flowId: nil, category: "food", note: "pizza", amount: 1_250)
        #expect(store.presentedError == nil)
        let pizza = try #require(store.transactions.first { $0.note == "pizza" })

        // Somewhere else: this month, the income, a search, the summary.
        store.month = MonthKey(Date())
        store.direction = .income
        store.searchText = "affitto"
        store.tab = .summary
        await store.settle()

        store.reveal(transactionId: pizza.id, occurredAt: pizza.occurredAt)
        await store.settle()

        #expect(store.tab == .ledger)
        #expect(store.month == MonthKey(monthAgo))
        #expect(store.direction == .expenses)
        #expect(store.searchText.isEmpty)
        #expect(store.rows.contains { $0.id == pizza.id })
        #expect(store.selection.ids == [pizza.id])
        #expect(store.revealedRow == pizza.id)
    }

    @Test("Revealing a row of the month on screen selects it at once, and a search on it is cleared by a reload")
    func revealOnTheMonthOnScreen() async throws {
        let store = try await Self.onboarded()
        await store.submit(quickAdd: "-12.50 pizza #food")
        await store.submit(quickAdd: "-3.00 caffè #bar")
        let pizza = try #require(store.topExpenses.first { $0.note == "pizza" })
        store.tab = .summary

        store.reveal(transactionId: pizza.transactionId, occurredAt: pizza.occurredAt)
        #expect(store.selection.ids == [pizza.transactionId])
        #expect(store.tab == .ledger)

        // A search that hides the row: the list comes back whole.
        store.searchText = "caffè"
        await store.reload()
        #expect(!store.rows.contains { $0.id == pizza.transactionId })
        store.reveal(transactionId: pizza.transactionId, occurredAt: pizza.occurredAt)
        await store.settle()
        #expect(store.searchText.isEmpty)
        #expect(store.selection.ids == [pizza.transactionId])
    }

    @Test("On a vault only read, revealing moves to the row without selecting it")
    func revealReadOnly() async throws {
        let store = try await Self.onboarded()
        let monthAgo = try #require(Calendar.current.date(byAdding: .month, value: -1, to: Date()))
        store.month = MonthKey(monthAgo)
        await store.settle()
        await store.addRow(day: monthAgo, flowId: nil, category: "food", note: "pizza", amount: 1_250)
        let pizza = try #require(store.transactions.first { $0.note == "pizza" })
        store.month = MonthKey(Date())
        await store.settle()
        store.setReadOnlyVaults([try #require(store.currentVault?.id)])

        store.reveal(transactionId: pizza.id, occurredAt: pizza.occurredAt)
        await store.settle()

        #expect(store.tab == .ledger)
        #expect(store.month == MonthKey(monthAgo))
        #expect(store.revealedRow == pizza.id)
        #expect(store.rows.contains { $0.id == pizza.id })
        #expect(store.selection.isEmpty)
    }

    @Test("A date the app cannot read reveals nothing")
    func revealUnreadableDate() async throws {
        let store = try await Self.onboarded()
        let month = store.month
        store.reveal(transactionId: "nope", occurredAt: "not a date")
        #expect(store.tab == .summary)
        #expect(store.month == month)
        #expect(store.revealedRow == nil)
    }

    @Test("A row that is not in the month it was asked for is not left armed")
    func revealOfAMissingRow() async throws {
        let store = try await Self.onboarded()
        let monthAgo = try #require(Calendar.current.date(byAdding: .month, value: -1, to: Date()))

        // Another month: the load that lands has no such row.
        store.reveal(transactionId: "gone", occurredAt: CoreDate.offset(monthAgo))
        await store.settle()
        #expect(store.month == MonthKey(monthAgo))
        #expect(store.tab == .ledger)
        #expect(store.revealedRow == nil)
        #expect(store.selection.isEmpty)

        // The month on screen already, nothing searched: no load at all.
        store.reveal(transactionId: "gone", occurredAt: CoreDate.offset(monthAgo))
        #expect(store.revealedRow == nil)
    }

    @Test("The row asked for is forgotten by a load of another month, and by another vault")
    func revealedRowGoesWithTheList() async throws {
        let store = try await Self.onboarded()
        await store.submit(quickAdd: "-12.50 pizza #food")
        let pizza = try #require(store.topExpenses.first { $0.note == "pizza" })
        let vault = try #require(store.currentVault)
        // Only read, so nothing selects it and the id stays for the grid.
        store.setReadOnlyVaults([vault.id])

        store.reveal(transactionId: pizza.transactionId, occurredAt: pizza.occurredAt)
        await store.settle()
        #expect(store.revealedRow == pizza.transactionId)
        // The user moves on before the grid got to it.
        store.stepMonth(by: -1)
        await store.settle()
        #expect(store.revealedRow == nil)
        // Back on its month, it is not selected under the user either.
        store.setReadOnlyVaults([])
        store.stepMonth(by: 1)
        await store.settle()
        #expect(store.selection.isEmpty)

        store.reveal(transactionId: pizza.transactionId, occurredAt: pizza.occurredAt)
        #expect(store.revealedRow == pizza.transactionId)
        await store.select(vault)
        #expect(store.revealedRow == nil)
    }

    // MARK: - The RIEPILOGO under PERSONA and on a vault only read

    @Test("The envelope table's people and figures stay everybody's under PERSONA; the view decides what it draws")
    func envelopePeopleIgnoreThePersonFilter() async throws {
        let store = try await Self.onboarded()
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "coop", amount: 1_000, person: "elisa")
        await store.submit(quickAdd: "-12.50 pizza #food")
        #expect(store.presentedError == nil)
        let everybody = try #require(store.envelopeYear)
        #expect(everybody.people == ["elisa", "tester"])

        store.person = "elisa"
        await store.settle()
        #expect(store.year?.people == ["elisa"])
        let filtered = try #require(store.envelopeYear)
        #expect(filtered.people == ["elisa", "tester"])
        #expect(filtered.rows.reduce(0) { $0 + $1.expense } == 2_250)
        let unallocated = try #require(filtered.rows.first)
        #expect(unallocated.expenseByPerson == [1_000, 1_250])
    }

    @Test("On a vault only read the strip offers no period to decide, and still counts the rows without a category")
    func stripOnAReadOnlyVault() async throws {
        let store = try await Self.onboarded()
        try await Self.rentSinceAMonthAgo(store)
        await store.createEnvelope(name: "Affitto", mode: .unlimited, allowNegative: false, openingAllocation: 0)
        let affitto = try #require(store.flows.first { $0.name == "Affitto" })
        let saved = await store.saveAllocationLines(
            [AllocationLine(flowId: affitto.id, rule: .fixed(amount: 1_000))],
            schedule: AllocationScheduleDraft(today: CoreDate.day(Date())).schedule
        )
        #expect(saved)
        // From Unallocated: Affitto holds nothing yet.
        let unallocated = try #require(store.flows.first { $0.isUnallocated })
        await store.addRow(day: Date(), flowId: unallocated.id, category: nil, note: "boh", amount: 500)
        #expect(store.presentedError == nil)
        #expect(store.summaryTodo.map(\.tab) == [.recurring, .allocation, .ledger])

        // The role arrives after the load, and then a load again.
        store.setReadOnlyVaults([try #require(store.currentVault?.id)])
        #expect(store.summaryTodo.map(\.kind) == [.uncategorized(count: 1)])
        await store.reload()
        #expect(store.summaryTodo.map(\.kind) == [.uncategorized(count: 1)])
        // The plan's period and the recurring ones are still there to see.
        #expect(store.pendingAllocation != nil)
        #expect(!store.duePeriods.isEmpty)
    }
}

struct MonthKeyTests {
    private static let calendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Europe/Rome") ?? .gmt
        return calendar
    }()

    @Test func boundsAreTheMonthInLocalTime() {
        let bounds = MonthKey(year: 2026, month: 2).bounds(Self.calendar)
        // Rome is UTC+1 in February, so local midnight is 23:00 the day before.
        #expect(bounds.from == "2026-01-31T23:00:00Z")
        #expect(bounds.to == "2026-02-28T23:00:00Z")
    }

    @Test func steppingRollsOverTheYear() {
        #expect(MonthKey(year: 2026, month: 12).adding(months: 1, calendar: Self.calendar) == MonthKey(year: 2027, month: 1))
        #expect(MonthKey(year: 2026, month: 1).adding(months: -1, calendar: Self.calendar) == MonthKey(year: 2025, month: 12))
    }

    @Test func aYearIsTwelveBucketsAndThirteenBoundaries() {
        let bounds = MonthKey.yearBounds(2026, calendar: Self.calendar)
        #expect(bounds.count == 13)
        #expect(bounds.first == "2025-12-31T23:00:00Z")
        #expect(bounds.last == "2026-12-31T23:00:00Z")
    }

    @Test func theTrailingYearEndsWithTheMonthItself() {
        let month = MonthKey(year: 2026, month: 8)
        let trailing = month.trailingYear(calendar: Self.calendar)
        #expect(trailing.months.count == 12)
        #expect(trailing.bounds.count == 13)
        #expect(trailing.months.first == MonthKey(year: 2025, month: 9))
        #expect(trailing.months.last == month)
    }

    @Test func aBareDayStaysInsideTheMonthOnScreen() throws {
        let month = MonthKey(year: 2026, month: 8)
        let parsed = try #require(LedgerDate.parseDay("29", in: month, calendar: Self.calendar))
        let parts = Self.calendar.dateComponents([.year, .month, .day], from: parsed)
        #expect(parts.year == 2026)
        #expect(parts.month == 8)
        #expect(parts.day == 29)

        // A day the month does not have snaps back rather than rolling over.
        #expect(LedgerDate.parseDay("31", in: MonthKey(year: 2026, month: 2), calendar: Self.calendar) == nil)
        #expect(LedgerDate.parseDay("", in: month, calendar: Self.calendar) == nil)

        // `29/9` moves to another month of the same year.
        let other = try #require(LedgerDate.parseDay("29/9", in: month, calendar: Self.calendar))
        #expect(Self.calendar.component(.month, from: other) == 9)
    }
}

/// Every code `ErrorMessages.summary(for:)` is documented to cover
/// (`ErrorMessages.swift`): every `DomainError` and `QuickAddError` code,
/// plus the `"domain_error"` fallback key.
struct ErrorMessagesTests {
    private static let allCodes = [
        "insufficient_funds", "max_balance_reached", "not_found", "already_exists",
        "invalid_amount", "invalid_name", "invalid_flow", "currency_mismatch",
        "invalid_command", "invalid_cursor", "storage_error",
        "empty_input", "missing_amount", "duplicate_marker", "marker_not_allowed",
        "missing_transfer_target", "invalid_date", "duplicate_date", "ambiguous_name",
        "unknown_name", "same_target", "domain_error",
        // Sync: the server's codes and the transport's own two.
        "offline", "unauthorized", "forbidden", "author_mismatch",
        "registration_disabled", "invalid_request", "invalid_response",
        "invalid_server_url", "server_error", "not_a_member",
    ]

    @Test("Every documented error code maps to a non-empty localized summary")
    func everyCodeHasASummary() {
        for code in Self.allCodes {
            let summary = ErrorMessages.summary(for: code)
            #expect(!summary.isEmpty, "no summary for \(code)")
        }
    }

    @Test("A change refused for naming someone outside the vault says who, from the server's detail, in the app's language")
    func notAMemberNamesThePerson() {
        let command = RejectedCommand(
            commandId: UUID().uuidString,
            kind: "expense",
            code: "not_a_member",
            message: "Elisa is not a member of this vault",
            detail: "Elisa"
        )
        let change = RejectedEntry(vaultId: UUID().uuidString, vaultName: "Casa", command: command)
        #expect(change.summary == String(localized: "\("Elisa") is not a member of this vault"))
        // No detail (an older server, a row refused before it): the generic
        // headline, whatever the sentence says.
        let older = RejectedCommand(
            commandId: UUID().uuidString,
            kind: "expense",
            code: "not_a_member",
            message: "Elisa is not a member of this vault"
        )
        #expect(
            RejectedEntry(vaultId: UUID().uuidString, vaultName: "Casa", command: older).summary
                == ErrorMessages.summary(for: "not_a_member")
        )
        #expect(ErrorMessages.summary(for: "not_a_member", detail: " ") == ErrorMessages.summary(for: "not_a_member"))
        // Every other code keeps its own headline, whatever the detail.
        #expect(ErrorMessages.summary(for: "forbidden", detail: "x") == ErrorMessages.summary(for: "forbidden"))
    }
}

/// Date conversions on the FFI boundary.
struct CoreDateTests {
    @Test func offsetStringRoundTrips() throws {
        let date = Date(timeIntervalSince1970: 1_772_000_000)
        let text = CoreDate.offset(date, timeZone: try #require(TimeZone(identifier: "Europe/Rome")))
        #expect(text == "2026-02-25T07:13:20+01:00")
        #expect(CoreDate.date(text) == date)
    }

    @Test func utcStringUsesZ() {
        let date = Date(timeIntervalSince1970: 1_772_000_000)
        #expect(CoreDate.utcString(date) == "2026-02-25T06:13:20Z")
    }

    @Test func dayIsTheLocalCalendarDay() throws {
        let date = Date(timeIntervalSince1970: 1_772_000_000)
        let rome = try #require(TimeZone(identifier: "Europe/Rome"))
        #expect(CoreDate.day(date, timeZone: rome) == "2026-02-25")
        #expect(CoreDate.localDay("2026-02-25", timeZone: rome) != nil)
    }
}
