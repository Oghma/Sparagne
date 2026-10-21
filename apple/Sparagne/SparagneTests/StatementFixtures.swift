import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Synthetic statements and the vault they are imported into, for the
/// statement import's tests. No real statement is used anywhere: the card
/// export has the columns a card app writes, the bank export the three a
/// current account's CSV holds. Lines count the header, so a file's first
/// row is line 2.
enum StatementFixture {
    // MARK: - The card export

    static let cardHeader = "timestamp,type,description,status,amount,currency,card,"
        + "card holder name,original amount,original currency,cashback earned,cashback currency,"
        + "category,spending mode"

    /// The header's columns, as the mapping lists them.
    static let cardHeaders = cardHeader.components(separatedBy: ",")

    /// A card row; the columns the import does not look at are filled in.
    static func cardRow(
        _ at: String,
        _ type: String,
        _ description: String,
        _ status: String,
        _ amount: String,
        currency: String = "EUR",
        category: String = ""
    ) -> String {
        "\(at),\(type),\(description),\(status),\(amount),\(currency),1234,SAM TESTER,"
            + "\(amount),\(currency),,,\(category),standard"
    }

    /// `rows` under the header, newest first as card exports are.
    static func cardFile(_ rows: String...) -> String {
        ([cardHeader] + rows).joined(separator: "\n") + "\n"
    }

    /// Four spends, one of them pending, and a top-up from the bank.
    static let card = cardFile(
        cardRow("2026-09-17 08:15:33 UTC", "card_spend", "FORNO BIANCO", "CLEARED", "3.20"), // 2
        cardRow("2026-09-16 12:03:10 UTC", "card_spend", "METRO TICKET", "CLEARED", "2.10"), // 3
        cardRow("2026-09-13 10:00:00 UTC", "topup", "Top-up from bank", "", "150.00"), // 4
        cardRow("2026-09-11 18:22:41 UTC", "card_spend", "GREEN GROCER", "CLEARED", "14.75"), // 5
        cardRow("2026-09-10 19:42:07 UTC", "card_spend", "OSTERIA DEL PONTE", "PENDING", "42.50") // 6
    )

    /// Three spends, all cleared.
    static let cardOfThreeSpends = cardFile(
        cardRow("2026-09-17 08:15:33 UTC", "card_spend", "FORNO BIANCO", "CLEARED", "3.20"), // 2
        cardRow("2026-09-16 12:03:10 UTC", "card_spend", "METRO TICKET", "CLEARED", "2.10"), // 3
        cardRow("2026-09-11 18:22:41 UTC", "card_spend", "GREEN GROCER", "CLEARED", "14.75") // 4
    )

    /// Eight rows with every case a card export holds: a pending spend, one
    /// with the merchant's category, a refund, a cancelled spend, a top-up
    /// and a deposit in another currency.
    static let cardOfEveryKind = cardFile(
        cardRow("2026-09-18 19:42:07 UTC", "card_spend", "OSTERIA DEL PONTE", "PENDING", "42.50"), // 2
        cardRow("2026-09-17 08:15:33 UTC", "card_spend", "FORNO BIANCO", "CLEARED", "3.20", category: "5462 - Bakeries"), // 3
        cardRow("2026-09-16 12:03:10 UTC", "card_spend", "METRO TICKET", "CLEARED", "2.10"), // 4
        cardRow("2026-09-15 16:20:00 UTC", "card_refund", "SHOE SHOP", "CLEARED", "-24.99"), // 5
        cardRow("2026-09-14 21:05:12 UTC", "card_spend", "CINEMA NUOVO", "CANCELLED", "9.00"), // 6
        cardRow("2026-09-13 10:00:00 UTC", "topup", "Top-up from bank", "", "150.00"), // 7
        cardRow("2026-09-12 09:30:00 UTC", "liquid_deposit", "Liquid deposit", "", "50.00", currency: "USD"), // 8
        cardRow("2026-09-11 18:22:41 UTC", "card_spend", "GREEN GROCER", "CLEARED", "14.75") // 9
    )

    // MARK: - The bank export

    /// `rows`, each `date;amount;description`, under the header of a bank
    /// export with no type column: every row goes by its sign.
    static func bankFile(_ rows: String...) -> String {
        "Data;Importo;Descrizione\n" + rows.map { $0 + "\n" }.joined()
    }

    /// A coffee and a salary.
    static let bankWithSalary = bankFile(
        "16/09/2026;-3,20;PAGAMENTO POS BAR CENTRALE", // 2
        "17/09/2026;1500,00;STIPENDIO SETTEMBRE" // 3
    )

    /// The top-up of the card going out, and a coffee.
    static let bankWithTopUp = bankFile(
        "13/09/2026;-150,00;RICARICA CARTA 1234", // 2
        "16/09/2026;-3,20;PAGAMENTO POS BAR CENTRALE" // 3
    )

    // MARK: - The vault

    /// Where the mappings are remembered, on a suite of its own.
    static func mappings(sourceLocation: SourceLocation = #_sourceLocation) throws -> StatementMappingStore {
        StatementMappingStore(defaults: try TestStore.defaults(sourceLocation: sourceLocation))
    }

    /// Vault `Main`, written by `tester`, with `wallet` opened at `opening`;
    /// then the quick-add lines `typed`, taken while `wallet` is the only
    /// one so they need no wallet marker; then `wallets` with their opening
    /// balances, and `envelopes`, unlimited and allowed below zero.
    static func vault(
        wallet: String = "Card",
        opening: Int64 = 0,
        typed: [String] = [],
        wallets: KeyValuePairs<String, Int64> = [:],
        envelopes: [String] = [],
        sourceLocation: SourceLocation = #_sourceLocation
    ) async throws -> AppStore {
        let store = try await TestStore.make(
            vault: "Main",
            wallet: wallet,
            opening: opening,
            author: "tester",
            sourceLocation: sourceLocation
        )
        for line in typed {
            await store.submit(quickAdd: line)
            #expect(store.presentedError == nil, "the quick-add refused \(line)", sourceLocation: sourceLocation)
        }
        for (name, balance) in wallets {
            await store.createWallet(name: name, openingBalance: balance)
        }
        for name in envelopes {
            await store.createEnvelope(name: name, mode: .unlimited, allowNegative: true, openingAllocation: 0)
        }
        #expect(store.presentedError == nil, sourceLocation: sourceLocation)
        return store
    }

    static func wallet(
        _ name: String,
        in store: AppStore,
        sourceLocation: SourceLocation = #_sourceLocation
    ) throws -> Uuid {
        try #require(store.wallets.first { $0.name == name }, sourceLocation: sourceLocation).id
    }

    static func envelope(
        _ name: String,
        in store: AppStore,
        sourceLocation: SourceLocation = #_sourceLocation
    ) throws -> Uuid {
        try #require(store.flows.first { $0.name == name }, sourceLocation: sourceLocation).id
    }

    static func balance(_ store: AppStore, _ wallet: Uuid) -> Int64? {
        store.wallets.first { $0.id == wallet }?.balance
    }

    /// Every transaction of the vault, oldest first, transfers included and
    /// the voided ones unless `voided` is false.
    static func transactions(
        _ store: AppStore,
        voided: Bool = true,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async throws -> [TransactionView] {
        let vaultId = try #require(store.currentVault?.id, sourceLocation: sourceLocation)
        let filter = TransactionFilter(includeVoided: voided, includeTransfers: true, ascending: true)
        return try await store.core.transactions(vaultId: vaultId, filter: filter, limit: 500, cursor: nil).items
    }

    // MARK: - The review

    /// `file` read as `card.csv` with its preset, aimed at `wallet` and
    /// previewed; `mappings` defaults to a store of its own.
    static func cardModel(
        _ store: AppStore,
        wallet: Uuid,
        mappings: StatementMappingStore? = nil,
        file: String = card,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async throws -> StatementImportModel {
        let model = StatementImportModel(
            store: store,
            mappings: try mappings ?? Self.mappings(sourceLocation: sourceLocation)
        )
        await model.load(data: Data(file.utf8), fileName: "card.csv")
        model.walletId = wallet
        await model.refreshPreview()
        #expect(model.previewProblem == nil, sourceLocation: sourceLocation)
        return model
    }

    /// The previewed row at `line`.
    static func row(
        _ model: StatementImportModel,
        line: UInt32,
        sourceLocation: SourceLocation = #_sourceLocation
    ) throws -> StatementRow {
        try #require(model.preview?.rows.first { $0.line == line }, sourceLocation: sourceLocation)
    }
}
