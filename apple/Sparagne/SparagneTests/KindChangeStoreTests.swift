import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Turning rows into another kind over a real in-memory core
/// (`AppStore+KindChange.swift`): the balances it moves, the batch it is,
/// its undo step, and what the Mastro's cursor and toast do after it.
struct KindChangeStoreTests {
    /// Vault `Casa` with wallet `Conto` at 1,000.00 in Unallocated, envelope
    /// `Spesa`, which may go below zero, and `Fondo`, which may not. On Spesa
    /// two expenses of today, `mutuo` 950.00 and `coop` 50.00. The Mastro on
    /// USCITE, and an undo manager attached once all that is written.
    private static func household() async throws -> (store: AppStore, manager: UndoManager) {
        let store = try await TestStore.make()
        await store.createEnvelope(name: "Spesa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        await store.createEnvelope(name: "Fondo", mode: .unlimited, allowNegative: false, openingAllocation: 0)
        let spesa = try Self.flow("Spesa", in: store)
        await store.addRow(day: Date(), flowId: spesa.id, category: "Casa", note: "mutuo", amount: 95_000)
        await store.addRow(day: Date(), flowId: spesa.id, category: "Spesa", note: "coop", amount: 5_000)
        store.tab = .ledger
        #expect(store.presentedError == nil)
        #expect(store.rows.map(\.note) == ["mutuo", "coop"])
        return (store, try LedgerHistoryTests.attachManager(to: store))
    }

    private static func flow(_ name: String, in store: AppStore) throws -> FlowView {
        try #require(store.flows.first { $0.name == name })
    }

    private static func row(_ note: String, in store: AppStore) throws -> TransactionRow {
        try #require(store.rows.first { $0.note == note })
    }

    private static func view(_ id: Uuid, in store: AppStore) throws -> TransactionView {
        try #require(store.transactions.first { $0.id == id })
    }

    private static func cell(_ id: Uuid?, _ field: RowField) -> CellFocus {
        CellFocus(row: id, field: field)
    }

    // MARK: - The change

    @Test("An expense turned into an income comes back the same row under a new id, and the balances turn with it")
    func expenseToIncome() async throws {
        let (store, _) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let before = try Self.view(coop.id, in: store)
        #expect(store.kindTargets(of: coop.id) == [.income, .refund])

        let created = try #require(await store.changeKind(of: coop.id, to: .income))
        #expect(store.presentedError == nil)
        #expect(created != coop.id)
        // 1,000.00 - 950.00 + 50.00, and Spesa -950.00 + 50.00.
        #expect(store.wallets.first?.balance == 10_000)
        #expect(try Self.flow("Spesa", in: store).balance == -90_000)

        store.direction = .income
        await store.settle()
        let income = try Self.view(created, in: store)
        #expect(income.kind == .income)
        #expect(income.amount == before.amount)
        #expect(income.occurredAt == before.occurredAt)
        #expect(income.walletId == before.walletId)
        #expect(income.flowId == before.flowId)
        #expect(income.categoryId == before.categoryId)
        #expect(income.note == "coop")
        #expect(income.person == before.person)

        // The old row is deleted, not edited.
        store.direction = .expenses
        store.showVoided = true
        await store.settle()
        #expect(try Self.view(coop.id, in: store).voided)
    }

    @Test("An income turned into an expense on an envelope that may not go below zero is refused, and nothing changes")
    func refusedOnTheFinalBalance() async throws {
        let (store, manager) = try await Self.household()
        store.direction = .income
        await store.settle()
        await store.addRow(day: Date(), flowId: try Self.flow("Fondo", in: store).id, category: "Regali", note: "regalo", amount: 5_000)
        let gift = try Self.row("regalo", in: store)
        #expect(gift.kind == .income)

        // Voided first, the income leaves Fondo at zero, and the expense
        // would take it below.
        #expect(await store.changeKind(of: gift.id, to: .expense) == nil)
        #expect(store.presentedError?.code == "insufficient_funds")
        #expect(try Self.flow("Fondo", in: store).balance == 5_000)
        let stored = try Self.view(gift.id, in: store)
        #expect(stored.kind == .income)
        #expect(!stored.voided)
        #expect(store.kindMoveNotice == nil)
        #expect(manager.undoActionName == String(localized: "Add Row"))
    }

    @Test("The amount typed with the kind is the new row's; the same amount is no change of it")
    func amountWithTheKind() async throws {
        let (store, _) = try await Self.household()
        let coop = try Self.row("coop", in: store)

        let refund = try #require(await store.changeKind(of: coop.id, to: .refund, amount: 6_000))
        #expect(try Self.view(refund, in: store).amount == 6_000)
        #expect(try Self.view(refund, in: store).kind == .refund)
        // Expense and refund share USCITE: no toast, the row is right there.
        #expect(store.kindMoveNotice == nil)
        #expect(store.rows.contains { $0.id == refund })
    }

    @Test("A selection changes in one batch and one undo step; transfers, deleted rows and rows already of the kind are passed over and stay selected")
    func selectionIsOneBatch() async throws {
        let (store, manager) = try await Self.household()
        await store.createWallet(name: "Contanti", openingBalance: 0)
        await store.submit(quickAdd: "tw> 10.00 @conto @contanti")
        await store.addRow(day: Date(), flowId: nil, category: "Spesa", note: "reso", amount: 1_000, kind: .refund)
        store.showTransfers = true
        await store.settle()
        #expect(store.presentedError == nil)
        let mutuo = try Self.row("mutuo", in: store)
        let coop = try Self.row("coop", in: store)
        let reso = try Self.row("reso", in: store)
        let transfer = try #require(store.rows.first { $0.isTransfer })
        store.selectAllRows()
        // Any row at least can become each of them.
        #expect(store.kindTargets(rows: store.selectedRows.map(\.id)) == [.expense, .income, .refund])
        #expect(store.kindTargets(rows: [mutuo.id, coop.id]) == [.income, .refund])
        #expect(store.kindTargets(rows: [transfer.id]).isEmpty)

        let executed = ExecutedCount()
        await store.core.setOnExecuted { executed.record() }
        let created = await store.changeKind(rows: store.selectedRows.map(\.id), to: .refund)
        #expect(store.presentedError == nil)
        #expect(executed.count == 1)
        #expect(Set(created.keys) == [mutuo.id, coop.id])
        let refunds = Set(created.values)
        #expect(store.rows.filter { refunds.contains($0.id) }.allSatisfy { $0.kind == .refund })
        // Selected as they were, under the new ids.
        #expect(store.selection.ids == refunds.union([reso.id, transfer.id]))
        #expect(manager.undoActionName == String(localized: "Kind Change"))

        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(executed.count == 2)
        let expenses = store.rows.filter { $0.note == "mutuo" || $0.note == "coop" }
        #expect(expenses.count == 2)
        #expect(expenses.allSatisfy { $0.kind == .expense })
        #expect(manager.canRedo)
        #expect(manager.undoActionName == String(localized: "Add Row"))
    }

    @Test("⌘Z and ⇧⌘Z go back and forth, each time under a new id, the toast going with the row it shows")
    func undoAndRedo() async throws {
        let (store, manager) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let income = try #require(await store.changeKind(of: coop.id, to: .income, amount: 6_000))
        #expect(store.kindMoveNotice?.transactionId == income)
        #expect(store.wallets.first?.balance == 11_000)

        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        // What the toast would show is deleted: the toast goes.
        #expect(store.kindMoveNotice == nil)
        let back = try Self.row("coop", in: store)
        #expect(back.id != coop.id && back.id != income)
        #expect(back.kind == .expense)
        #expect(back.absoluteAmount == 5_000)
        #expect(store.wallets.first?.balance == 0)

        manager.redo()
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(!store.rows.contains { $0.note == "coop" })
        #expect(store.wallets.first?.balance == 11_000)

        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(try Self.row("coop", in: store).absoluteAmount == 5_000)
        #expect(store.wallets.first?.balance == 0)
    }

    @Test("The steps about the row deleted by the change are forgotten, and so are the new row's once undo deletes it")
    func oldStepsAreForgotten() async throws {
        let (store, manager) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        await store.update(transactionId: coop.id, patch: TransactionPatch(note: "conad"))
        #expect(manager.undoActionName == String(localized: "Edit Row"))

        let refund = try #require(await store.changeKind(of: coop.id, to: .refund))
        await store.update(transactionId: refund, patch: TransactionPatch(note: "esselunga"))

        manager.undo()
        manager.undo()
        await store.settle()
        #expect(store.presentedError == nil)
        let back = try Self.row("conad", in: store)
        #expect(back.kind == .expense)
        // The edit made before the change could only be refused now.
        #expect(!manager.canUndo)

        // The edit of the refund went with the refund: redo has the change
        // alone, and the note it had.
        manager.redo()
        await store.settle()
        #expect(store.presentedError == nil)
        #expect(try Self.row("conad", in: store).kind == .refund)
        #expect(!manager.canRedo)
    }

    @Test("The toast's Mostra opens the list the rows went to, the row selected, and goes")
    func showMovedRow() async throws {
        let (store, _) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        let income = try #require(await store.changeKind(of: coop.id, to: .income))
        let notice = try #require(store.kindMoveNotice)
        #expect(notice.direction == .income)
        #expect(notice.count == 1)
        #expect(notice.duration == AppStore.kindMoveNoticeDuration)

        store.showMovedRow(notice)
        #expect(store.kindMoveNotice == nil)
        await store.settle()
        #expect(store.direction == .income)
        #expect(store.selection.ids == [income])

        // Closed or elapsed, a toast goes; an older toast's id leaves it.
        store.direction = .expenses
        await store.settle()
        let mutuo = try Self.row("mutuo", in: store)
        let again = try #require(await store.changeKind(of: mutuo.id, to: .income))
        let shown = try #require(store.kindMoveNotice)
        #expect(shown.transactionId == again)
        store.dismissKindMoveNotice(notice.id)
        #expect(store.kindMoveNotice == shown)
        store.dismissKindMoveNotice(shown.id)
        #expect(store.kindMoveNotice == nil)
    }

    // MARK: - IMPORTO with a sign

    @Test("+ on an expense's IMPORTO makes it an income at the number typed; the row leaves USCITE and the toast says so")
    func plusOnAnExpense() async throws {
        let (store, _) = try await Self.household()
        let editor = store.ledgerEditor
        let coop = try Self.row("coop", in: store)
        editor.select(Self.cell(coop.id, .amount))
        editor.type("+60")
        editor.confirm()
        await store.settle()

        #expect(store.presentedError == nil)
        #expect(!store.rows.contains { $0.note == "coop" })
        #expect(editor.cursor == nil)
        let notice = try #require(store.kindMoveNotice)
        #expect(notice.direction == .income)
        store.direction = .income
        await store.settle()
        let income = try #require(store.rows.first { $0.note == "coop" })
        #expect(income.id == notice.transactionId)
        #expect(income.absoluteAmount == 6_000)
    }

    @Test("r on an expense makes it a refund that stays on screen, the cursor following it to its new id")
    func refundFollowsTheCursor() async throws {
        let (store, _) = try await Self.household()
        let editor = store.ledgerEditor
        let coop = try Self.row("coop", in: store)
        editor.select(Self.cell(coop.id, .amount))
        editor.type("r50")
        editor.confirm()
        await store.settle()

        #expect(store.presentedError == nil)
        let refund = try Self.row("coop", in: store)
        #expect(refund.kind == .refund)
        #expect(refund.id != coop.id)
        #expect(refund.absoluteAmount == 5_000)
        #expect(editor.cursor == Self.cell(refund.id, .amount))
        #expect(store.kindMoveNotice == nil)
    }

    @Test("The row's own sign, or none, changes the amount alone under the same id")
    func ownSignIsTheAmount() async throws {
        let (store, _) = try await Self.household()
        let editor = store.ledgerEditor
        let coop = try Self.row("coop", in: store)
        for (text, amount) in [("-60", Int64(6_000)), ("\u{2212}70", 7_000), ("80", 8_000)] {
            editor.select(Self.cell(coop.id, .amount))
            editor.type(text)
            editor.confirm()
            await store.settle()
            #expect(store.presentedError == nil)
            #expect(try Self.row("coop", in: store).id == coop.id)
            #expect(try Self.row("coop", in: store).absoluteAmount == amount)
        }
    }

    @Test("A sign with no number, or on a row that cannot change kind, keeps the cell open with the reason")
    func refusedSigns() async throws {
        let (store, _) = try await Self.household()
        let editor = store.ledgerEditor
        let coop = try Self.row("coop", in: store)

        editor.select(Self.cell(coop.id, .amount))
        editor.type("r")
        editor.confirm()
        #expect(store.presentedError?.code == "invalid_amount")
        #expect(editor.isEditing)
        #expect(editor.text == "r")
        editor.cancel()
        store.presentedError = nil

        // The wallet's opening balance, on ENTRATE.
        store.direction = .income
        await store.settle()
        let opening = try #require(store.rows.first { $0.kind == .income })
        #expect(store.kindTargets(of: opening.id).isEmpty)
        editor.select(Self.cell(opening.id, .amount))
        editor.type("-5")
        editor.confirm()
        #expect(store.presentedError?.code == "invalid_command")
        #expect(store.presentedError?.summary == String(localized: "An opening balance cannot change kind"))
        #expect(editor.isEditing)
        editor.cancel()
        store.presentedError = nil

        // A transfer.
        await store.createWallet(name: "Contanti", openingBalance: 0)
        await store.submit(quickAdd: "tw> 10.00 @conto @contanti")
        store.showTransfers = true
        await store.settle()
        let transfer = try #require(store.rows.first { $0.isTransfer })
        editor.select(Self.cell(transfer.id, .amount))
        editor.type("+5")
        editor.confirm()
        #expect(store.presentedError?.summary == String(localized: "A transfer cannot change kind"))
        #expect(editor.isEditing)
        editor.cancel()
        await store.settle()
        #expect(try #require(store.rows.first { $0.id == transfer.id }).absoluteAmount == 1_000)
    }

    @Test("On the new line a sign wins over the list on screen; no sign is the list's kind")
    func signOnTheNewLine() async throws {
        let (store, _) = try await Self.household()
        let editor = store.ledgerEditor
        for (note, amount) in [("stipendio", "+300"), ("reso", "r12"), ("pane", "2")] {
            editor.select(Self.cell(nil, .note))
            editor.type(note)
            editor.select(Self.cell(nil, .amount))
            editor.type(amount)
            editor.confirm()
        }
        store.direction = .income
        await store.settle()
        editor.select(Self.cell(nil, .note))
        editor.type("multa")
        editor.select(Self.cell(nil, .amount))
        editor.type("-40")
        editor.confirm()
        await store.settle()
        #expect(store.presentedError == nil)

        #expect(try #require(store.rows.first { $0.note == "stipendio" }).absoluteAmount == 30_000)
        store.direction = .expenses
        await store.settle()
        #expect(try Self.row("reso", in: store).kind == .refund)
        #expect(try Self.row("pane", in: store).kind == .expense)
        #expect(try Self.row("multa", in: store).kind == .expense)
        #expect(try Self.row("multa", in: store).absoluteAmount == 4_000)
    }

    // MARK: - A vault only read

    @Test("A vault only read offers no kind to change to, and refuses a change that gets through")
    func readOnly() async throws {
        let (store, _) = try await Self.household()
        let coop = try Self.row("coop", in: store)
        store.setReadOnlyVaults([try #require(store.currentVault).id])
        #expect(store.kindTargets(of: coop.id).isEmpty)
        #expect(store.kindTargets(rows: [coop.id]).isEmpty)
        #expect(await store.changeKind(of: coop.id, to: .income) == nil)
        #expect(store.presentedError?.code == "forbidden")
        #expect(try Self.row("coop", in: store).kind == .expense)
    }
}
