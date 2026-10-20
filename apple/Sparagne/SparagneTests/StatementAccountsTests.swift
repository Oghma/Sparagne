import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// An export holding every account of the user's, as the BudgetBakers
/// Wallet app writes it: each account's rows go to the wallet chosen for it,
/// the file's or another, or nowhere; the two halves of a move between two
/// accounts make one transfer, or nothing inside one wallet. Over an
/// in-memory core; no real export is used.
struct StatementAccountsTests {
    /// Three accounts; lines count the header. A move from Revolut to BBVA
    /// (3 and 4), one from Conto Arancio to Revolut (7 and 8), and a half
    /// whose other half is in no account of the file (9).
    private static let export = """
        account;category;currency;amount;ref_currency_amount;type;payment_type;note;date;transfer;payee;labels
        Revolut;Groceries;EUR;23.40;23.40;Expense;Debit card;Esselunga;2026-10-14T17:20:00.000Z;false;;
        Revolut;Transfer, withdraw;EUR;500.00;500.00;Expense;Transfer;To BBVA;2026-10-12T08:00:00.000Z;true;;
        BBVA;Transfer, withdraw;EUR;500.00;500.00;Income;Transfer;From Revolut;2026-10-12T09:30:00.000Z;true;;
        BBVA;Restaurant, fast-food;EUR;35.00;35.00;Expense;Debit card;Pizzeria;2026-10-10T18:15:00.000Z;false;;
        Conto Arancio;Income;EUR;1850.00;1850.00;Income;Transfer;Stipendio;2026-10-01T07:00:00.000Z;false;;
        Conto Arancio;Transfer, withdraw;EUR;200.00;200.00;Expense;Transfer;To Revolut;2026-10-02T10:00:00.000Z;true;;
        Revolut;Transfer, withdraw;EUR;200.00;200.00;Income;Transfer;From Conto Arancio;2026-10-03T08:00:00.000Z;true;;
        Revolut;Transfer, withdraw;EUR;75.00;75.00;Income;Transfer;From a friend;2026-10-05T12:00:00.000Z;true;;
        Revolut;Culture, sport events;EUR;100.79;100.79;Expense;Transfer;Biglietto;2026-10-16T09:34:44.000Z;false;;

        """

    private static let headers = [
        "account", "category", "currency", "amount", "ref_currency_amount", "type",
        "payment_type", "note", "date", "transfer", "payee", "labels",
    ]

    /// Vault `Main` with `Cash`, where everything has gone so far, and the
    /// empty `Revolut` and `BBVA`.
    private static func vault() async throws -> (store: AppStore, cash: Uuid, revolut: Uuid, bbva: Uuid) {
        let store = try await StatementFixture.vault(wallet: "Cash", wallets: ["Revolut": 0, "BBVA": 0])
        let wallet = { try StatementFixture.wallet($0, in: store) }
        return (store, try wallet("Cash"), try wallet("Revolut"), try wallet("BBVA"))
    }

    /// The export read with its preset, aimed at Cash, its accounts listed.
    private static func model(
        _ store: AppStore, cash: Uuid, mappings: StatementMappingStore
    ) async throws -> StatementImportModel {
        let model = StatementImportModel(store: store, mappings: mappings)
        await model.load(data: Data(Self.export.utf8), fileName: "wallet.csv")
        model.walletId = cash
        await model.accountsListed()
        return model
    }

    private static func code(_ row: StatementRow) -> String? {
        if case .skipped(let code, _) = row.status { return code }
        return nil
    }

    // MARK: - Choosing where accounts go

    @Test("The accounts are listed with their rows, and those sent elsewhere than the file's wallet become the core's account wallets")
    func accountChoices() async throws {
        let (store, cash, revolut, _) = try await Self.vault()
        let model = try await Self.model(store, cash: cash, mappings: try StatementFixture.mappings())

        #expect(model.mapping.accountColumn == "account")
        #expect(model.mapping.transferColumn == "transfer")
        #expect(model.accounts.map(\.value) == ["Revolut", "BBVA", "Conto Arancio"])
        #expect(model.accounts.map(\.rows) == [5, 2, 2])
        #expect(model.previewInput?.options.accountWallets == [])

        model.setDestination(.wallet(revolut), ofAccount: "Revolut")
        model.setDestination(.leftOut, ofAccount: "Conto Arancio")
        // The file's own wallet is no choice to send.
        model.setDestination(.wallet(cash), ofAccount: "BBVA")
        #expect(model.destination(ofAccount: "bbva") == nil)
        #expect(model.destination(ofAccount: " revolut ") == .wallet(revolut))
        #expect(model.previewInput?.options.accountWallets == [
            StatementAccountWallet(account: "Conto Arancio", walletId: nil),
            StatementAccountWallet(account: "Revolut", walletId: revolut),
        ])

        model.setDestination(nil, ofAccount: "Revolut")
        #expect(model.previewInput?.options.accountWallets == [
            StatementAccountWallet(account: "Conto Arancio", walletId: nil),
        ])
    }

    @Test("Where each account went is remembered with the import, counts among the wallets statements go to, and comes back without a wallet gone since")
    func accountChoicesRemembered() async throws {
        let (store, cash, revolut, bbva) = try await Self.vault()
        await store.createWallet(name: "Old", openingBalance: 0)
        let old = try #require(store.wallets.first { $0.name == "Old" }).id
        let mappings = try StatementFixture.mappings()
        let vaultId = try #require(store.currentVault?.id)

        let model = try await Self.model(store, cash: cash, mappings: mappings)
        model.setDestination(.wallet(revolut), ofAccount: "Revolut")
        model.setDestination(.wallet(bbva), ofAccount: "BBVA")
        model.setDestination(.leftOut, ofAccount: "Conto Arancio")
        await model.refreshPreview()
        await model.runImport()
        #expect(model.problem == nil)

        let target = try #require(mappings.target(vaultId: vaultId, headers: Self.headers))
        #expect(target.walletId == cash)
        #expect(Set(target.accounts.map(\.account)) == ["Revolut", "BBVA", "Conto Arancio"])
        #expect(target.accounts.first { $0.account == "Conto Arancio" }?.walletId == nil)
        #expect(mappings.targetWallets(vaultId: vaultId) == [cash, revolut, bbva])

        let again = try await Self.model(store, cash: cash, mappings: mappings)
        #expect(again.destination(ofAccount: "Revolut") == .wallet(revolut))
        #expect(again.destination(ofAccount: "BBVA") == .wallet(bbva))
        #expect(again.destination(ofAccount: "Conto Arancio") == .leftOut)

        // A wallet archived since: its account goes to the file's wallet.
        mappings.remember(
            StatementImportTarget(walletId: cash, flowId: nil, accounts: [.init(account: "Revolut", walletId: old)]),
            vaultId: vaultId,
            headers: Self.headers
        )
        await store.archiveWallet(old)
        let third = try await Self.model(store, cash: cash, mappings: mappings)
        #expect(third.destination(ofAccount: "Revolut") == nil)
        #expect(third.previewInput?.options.accountWallets == [])
    }

    // MARK: - The core

    @Test("Every account into Cash: the Wallet preset is recognised, both halves of each move are a transfer within one wallet, and a half without its other is imported")
    func everythingIntoCash() async throws {
        let (store, cash, _, _) = try await Self.vault()
        let model = try await Self.model(store, cash: cash, mappings: try StatementFixture.mappings())
        #expect(model.source == .preset(StatementImportModel.presetName(
            try #require(statementPresets().first { $0.id == "budgetbakers-wallet" })
        )))
        await model.refreshPreview()
        #expect(model.previewProblem == nil)

        for line: UInt32 in [3, 4, 7, 8] {
            let half = try StatementFixture.row(model, line: line)
            #expect(Self.code(half) == "internal_transfer", "line \(line)")
            #expect(half.transferLeg)
            #expect(half.pairedLine != nil)
        }
        #expect(try StatementFixture.row(model, line: 3).pairedLine == 4)
        let friend = try StatementFixture.row(model, line: 9)
        #expect(friend.status == .new)
        #expect(friend.transferLeg)
        #expect(friend.pairedLine == nil)
        #expect(friend.kind == .income)
        let groceries = try StatementFixture.row(model, line: 2)
        #expect(groceries.account == "Revolut")
        #expect(groceries.walletId == cash)
        #expect(model.counts.new == 5)

        await model.runImport()

        #expect(model.report?.executed == 5)
        let all = try await StatementFixture.transactions(store, voided: false)
        #expect(!all.contains { $0.kind == .transferWallet })
        // 1850.00 + 75.00 in, 23.40 + 35.00 + 100.79 out.
        #expect(StatementFixture.balance(store, cash) == 176_581)
    }

    @Test("Revolut and BBVA into their wallets and Conto Arancio left out: one transfer, a half whose other was left out imported, and both balances right")
    func accountsIntoWallets() async throws {
        let (store, cash, revolut, bbva) = try await Self.vault()
        let model = try await Self.model(store, cash: cash, mappings: try StatementFixture.mappings())
        model.setDestination(.wallet(revolut), ofAccount: "Revolut")
        model.setDestination(.wallet(bbva), ofAccount: "BBVA")
        model.setDestination(.leftOut, ofAccount: "Conto Arancio")
        await model.refreshPreview()
        #expect(model.previewProblem == nil)

        let out = try StatementFixture.row(model, line: 3)
        #expect(out.status == .new)
        #expect(out.kind == .transferWallet)
        #expect(out.walletId == revolut)
        #expect(out.pairedLine == 4)
        let into = try StatementFixture.row(model, line: 4)
        #expect(Self.code(into) == "transfer_pair")
        #expect(into.pairedLine == 3)
        #expect(into.walletId == bbva)
        #expect(StatementText.otherHalf(of: 3) != StatementText.skipped("transfer_pair"))
        for line: UInt32 in [6, 7] {
            #expect(Self.code(try StatementFixture.row(model, line: line)) == "skipped_account", "line \(line)")
        }
        // Its other half's account left out, Revolut's 200.00 is an income.
        let lonely = try StatementFixture.row(model, line: 8)
        #expect(lonely.status == .new)
        #expect(lonely.kind == .income)
        #expect(lonely.pairedLine == nil)
        #expect(model.counts.new == 6)

        await model.runImport()

        #expect(model.report?.executed == 6)
        let transfers = try await StatementFixture.transactions(store, voided: false).filter { $0.kind == .transferWallet }
        #expect(transfers.count == 1)
        #expect(transfers.first?.fromId == revolut)
        #expect(transfers.first?.toId == bbva)
        #expect(transfers.first?.amount == 50_000)
        // Revolut: 200.00 + 75.00 in, 23.40 + 500.00 + 100.79 out.
        #expect(StatementFixture.balance(store, revolut) == -34_919)
        // BBVA: 500.00 in, 35.00 out.
        #expect(StatementFixture.balance(store, bbva) == 46_500)
        #expect(StatementFixture.balance(store, cash) == 0)
    }

    // MARK: - Words

    @Test("The new skipped codes have words of their own")
    func words() {
        let codes = ["skipped_account", "internal_transfer", "transfer_pair", "needs_wallet", "skipped_by_you"]
            .map(StatementText.skipped)
        #expect(Set(codes).count == codes.count)
        #expect(!codes.contains(StatementText.skipped("something_new")))
        #expect(!StatementText.unpairedLeg.isEmpty)
    }
}
