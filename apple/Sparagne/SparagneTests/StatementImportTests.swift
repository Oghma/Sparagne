import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The statement import end to end over an in-memory core: the model reads a
/// synthetic card export, the core previews and imports it, and every
/// assertion is about what the vault holds afterwards. No real statement is
/// used anywhere.
@MainActor
struct StatementImportTests {
    // MARK: - Fixtures

    private static let cardHeader = "timestamp,type,description,status,amount,currency,card,"
        + "card holder name,original amount,original currency,cashback earned,cashback currency,"
        + "category,spending mode"

    /// A card row; the columns the import does not look at are filled in.
    private static func cardRow(
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

    /// Eight rows, newest first as card exports are. Lines count the header,
    /// so the first row is line 2.
    private static let card = ([cardHeader] + [
        cardRow("2026-09-18 19:42:07 UTC", "card_spend", "OSTERIA DEL PONTE", "PENDING", "42.50"), // 2
        cardRow("2026-09-17 08:15:33 UTC", "card_spend", "FORNO BIANCO", "CLEARED", "3.20", category: "5462 - Bakeries"), // 3
        cardRow("2026-09-16 12:03:10 UTC", "card_spend", "METRO TICKET", "CLEARED", "2.10"), // 4
        cardRow("2026-09-15 16:20:00 UTC", "card_refund", "SHOE SHOP", "CLEARED", "-24.99"), // 5
        cardRow("2026-09-14 21:05:12 UTC", "card_spend", "CINEMA NUOVO", "CANCELLED", "9.00"), // 6
        cardRow("2026-09-13 10:00:00 UTC", "topup", "Top-up from bank", "", "150.00"), // 7
        cardRow("2026-09-12 09:30:00 UTC", "liquid_deposit", "Liquid deposit", "", "50.00", currency: "USD"), // 8
        cardRow("2026-09-11 18:22:41 UTC", "card_spend", "GREEN GROCER", "CLEARED", "14.75"), // 9
    ]).joined(separator: "\n") + "\n"

    private static func defaults() throws -> UserDefaults {
        try #require(UserDefaults(suiteName: "sparagne.tests.\(UUID().uuidString)"))
    }

    /// Vault `Main` with the card wallet `Card` and a `Bank` holding 1000.00.
    /// A past `forno bianco` filed under Bakery gives the history something
    /// to suggest; it is added while `Card` is the only wallet, so the
    /// quick-add needs no wallet marker.
    private static func vault() async throws -> (store: AppStore, card: Uuid, bank: Uuid) {
        let store = AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: try defaults())
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Card", openingBalance: 0)
        await store.submit(quickAdd: "-3.20 forno bianco #Bakery")
        await store.createWallet(name: "Bank", openingBalance: 100_000)
        #expect(store.presentedError == nil)
        let card = try #require(store.wallets.first { $0.name == "Card" }).id
        let bank = try #require(store.wallets.first { $0.name == "Bank" }).id
        return (store, card, bank)
    }

    private static func row(_ model: StatementImportModel, line: UInt32) throws -> StatementRow {
        try #require(model.preview?.rows.first { $0.line == line })
    }

    /// Every transaction of the vault, voided and transfers included.
    private static func transactions(_ store: AppStore) async throws -> [TransactionView] {
        let vaultId = try #require(store.currentVault?.id)
        let filter = TransactionFilter(includeVoided: true, includeTransfers: true, ascending: true)
        return try await store.core.transactions(vaultId: vaultId, filter: filter, limit: 500, cursor: nil).items
    }

    // MARK: - The mapping store

    @Test("A remembered mapping comes back for the same vault and header, in any case or padding")
    func mappingStoreRoundTrip() throws {
        let mappings = StatementMappingStore(defaults: try Self.defaults())
        var mapping = StatementImportModel.blankMapping(delimiter: ";")
        mapping.dateColumn = "Data"
        mapping.amountColumn = "Importo"
        // A real id: the core parses it on the way to JSON.
        let wallet = UUID().uuidString.lowercased()
        mapping.typeRules = [StatementTypeRule(value: "ricarica", action: .transferIn(fromWalletId: wallet))]

        mappings.remember(mapping, vaultId: "v-1", headers: ["Data", "Importo", "Descrizione"])

        #expect(mappings.mapping(vaultId: "v-1", headers: ["Data", "Importo", "Descrizione"]) == mapping)
        #expect(mappings.mapping(vaultId: "v-1", headers: [" data", "IMPORTO ", "descrizione"]) == mapping)
    }

    @Test("Another header or another vault remembers nothing")
    func mappingStoreKeying() throws {
        let mappings = StatementMappingStore(defaults: try Self.defaults())
        let mapping = StatementImportModel.blankMapping(delimiter: ",")
        mappings.remember(mapping, vaultId: "v-1", headers: ["date", "amount"])

        #expect(mappings.mapping(vaultId: "v-1", headers: ["date", "amount", "note"]) == nil)
        #expect(mappings.mapping(vaultId: "v-1", headers: ["amount", "date"]) == nil)
        #expect(mappings.mapping(vaultId: "v-2", headers: ["date", "amount"]) == nil)
    }

    // MARK: - Reading the file

    @Test("A file that is not UTF-8 is read as Windows-1252, euro sign included")
    func windowsCodePageFallback() {
        // "Caffè €" in Windows-1252: è is 0xE8, € is 0x80. Neither is valid
        // UTF-8 on its own.
        let bytes = Array("Caff".utf8) + [0xE8, 0x20, 0x80]
        #expect(StatementImportModel.decode(Data(bytes)) == "Caffè €")
        // The same text in UTF-8 stays UTF-8, not "CaffÃ¨".
        #expect(StatementImportModel.decode(Data("Caffè €".utf8)) == "Caffè €")
    }

    @Test("A Latin-1 bank export with no preset opens on a blank continental mapping and previews once mapped")
    func latinBankExport() async throws {
        let (store, card, _) = try await Self.vault()
        let model = StatementImportModel(store: store, mappings: StatementMappingStore(defaults: try Self.defaults()))
        let lines = "Data;Importo;Descrizione\n16/09/2026;-3,20;Caff\u{E8} della stazione\n"
        let data = try #require(lines.data(using: .isoLatin1))
        #expect(String(data: data, encoding: .utf8) == nil)

        await model.load(data: data, fileName: "banca.csv")

        #expect(model.problem == nil)
        #expect(model.step == .mapping)
        #expect(model.source == .blank)
        #expect(model.detection?.headers == ["Data", "Importo", "Descrizione"])
        #expect(model.mapping.decimalComma)
        #expect(model.mapping.dateFormat == .dayMonthYear)
        #expect(model.previewInput == nil)

        model.mapping.dateColumn = "Data"
        model.mapping.amountColumn = "Importo"
        model.mapping.descriptionColumns = ["Descrizione"]
        model.walletId = card
        await model.refreshPreview()

        let row = try Self.row(model, line: 2)
        #expect(row.status == .new)
        #expect(row.kind == .expense)
        #expect(row.amount == 320)
        #expect(row.payee == "Caffè della stazione")
    }

    // MARK: - Preview and import

    @Test("The card header opens on the preset, and the preview counts every row")
    func presetAndPreviewCounts() async throws {
        let (store, card, _) = try await Self.vault()
        let model = StatementImportModel(store: store, mappings: StatementMappingStore(defaults: try Self.defaults()))

        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")

        #expect(model.detection?.presetId == "card-transactions")
        guard case .preset = model.source else {
            Issue.record("expected the card preset, got \(model.source)")
            return
        }
        #expect(model.mapping.typeColumn == "type")
        #expect(model.unruledTypeValues.isEmpty)
        // Two wallets: the target is the user's to choose.
        #expect(model.walletId == nil)
        #expect(model.previewInput == nil)

        model.walletId = card
        await model.refreshPreview()

        #expect(model.previewProblem == nil)
        #expect(model.counts == .init(new: 4, alreadyImported: 0, skipped: 4, invalid: 0, rounded: 0))
        let pending = try Self.row(model, line: 2)
        #expect(pending.status == .skipped(code: "skipped_status", reason: "status PENDING is skipped"))
        let cancelled = try Self.row(model, line: 6)
        #expect(StatementText.status(cancelled.status) == StatementText.skipped("skipped_status"))
        let topup = try Self.row(model, line: 7)
        #expect(topup.kind == .transferWallet)
        guard case .skipped(let code, _) = topup.status else {
            Issue.record("the top-up should wait for its wallet")
            return
        }
        #expect(code == "needs_wallet")
        let deposit = try Self.row(model, line: 8)
        guard case .skipped(let depositCode, _) = deposit.status else {
            Issue.record("the liquid deposit should be skipped by its rule")
            return
        }
        #expect(depositCode == "skipped_by_rule")
        // The history's category for the payee fills the field in.
        let bakery = try Self.row(model, line: 3)
        let metro = try Self.row(model, line: 4)
        #expect(bakery.matchedCategory == nil)
        #expect(model.categoryText(for: bakery) == "Bakery")
        #expect(model.categoryText(for: metro) == "")
    }

    @Test("A top-up with its source wallet chosen becomes a transfer into the card")
    func topupBecomesTransfer() async throws {
        let (store, card, bank) = try await Self.vault()
        let model = StatementImportModel(store: store, mappings: StatementMappingStore(defaults: try Self.defaults()))
        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        model.walletId = card
        // The target is not offered as the other side of a transfer.
        #expect(model.counterWallets.map(\.id) == [bank])

        let topupRule = try #require(model.mapping.typeRules.firstIndex { $0.value == "topup" })
        model.mapping.typeRules[topupRule].action = StatementActionChoice.transferIn
            .action(keeping: model.mapping.typeRules[topupRule].action)
            .withCounterWallet(bank)
        await model.refreshPreview()

        let topup = try Self.row(model, line: 7)
        #expect(topup.status == .new)
        #expect(topup.counterWalletId == bank)
        #expect(model.counts.new == 5)

        await model.runImport()

        #expect(model.problem == nil)
        #expect(model.step == .report)
        #expect(model.report?.executed == 5)
        let imported = try await Self.transactions(store)
        let transfer = try #require(imported.first { $0.kind == .transferWallet })
        #expect(transfer.fromId == bank)
        #expect(transfer.toId == card)
        #expect(transfer.amount == 15_000)
        #expect(store.wallets.first { $0.id == bank }?.balance == 85_000)

        // Making the bank the target releases the rule instead of pointing
        // a transfer at itself.
        model.walletId = bank
        #expect(model.mapping.typeRules[topupRule].action == .transferIn(fromWalletId: nil))
    }

    @Test("A typed category and a ticked-off row reach the import, and the next import finds nothing new")
    func overridesAndReimport() async throws {
        let (store, card, bank) = try await Self.vault()
        let mappings = StatementMappingStore(defaults: try Self.defaults())
        let model = StatementImportModel(store: store, mappings: mappings)
        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        model.walletId = card
        let topupRule = try #require(model.mapping.typeRules.firstIndex { $0.value == "topup" })
        model.mapping.typeRules[topupRule].action = .transferIn(fromWalletId: bank)
        await model.refreshPreview()

        model.setCategory("Transport", for: 4)
        model.setIncluded(false, line: 9)

        #expect(model.counts == .init(new: 4, alreadyImported: 0, skipped: 4, invalid: 0, rounded: 0))
        // The suggestion travels as an override, the typed category too; the
        // refund keeps its empty field, so it sends nothing.
        #expect(model.overrides.sorted { $0.line < $1.line } == [
            StatementRowOverride(line: 3, category: "Bakery", note: nil, skip: false),
            StatementRowOverride(line: 4, category: "Transport", note: nil, skip: false),
            StatementRowOverride(line: 9, category: nil, note: nil, skip: true),
        ])

        await model.runImport()

        let report = try #require(model.report)
        #expect(report.executed == 4)
        #expect(report.skipped == 4)
        #expect(report.deduplicated == 0)
        #expect(report.rejected.isEmpty)

        let imported = try await Self.transactions(store)
        #expect(imported.first { $0.note == "FORNO BIANCO" }?.category == "Bakery")
        #expect(imported.first { $0.note == "METRO TICKET" }?.category == "Transport")
        #expect(imported.first { $0.note == "SHOE SHOP" }?.kind == .refund)
        #expect(!imported.contains { $0.note == "GREEN GROCER" })
        // The typed category was created; the window reloaded after the import.
        #expect(store.categories.contains { $0.name == "Transport" })

        // Same file again, in a new sheet: the mapping is remembered, top-up
        // wallet included, and every row already in the vault is recognised.
        let again = StatementImportModel(store: store, mappings: mappings)
        await again.load(data: Data(Self.card.utf8), fileName: "card.csv")
        #expect(again.source == .remembered)
        #expect(again.mapping.typeRules[topupRule].action == .transferIn(fromWalletId: bank))
        again.walletId = card
        await again.refreshPreview()

        // The row ticked off last time was never imported, so it is new.
        #expect(again.counts == .init(new: 1, alreadyImported: 4, skipped: 3, invalid: 0, rounded: 0))
        let grocer = try Self.row(again, line: 9)
        #expect(grocer.status == .new)
        again.setIncluded(false, line: 9)

        await again.runImport()

        #expect(again.report?.executed == 0)
        #expect(again.report?.deduplicated == 4)
        let after = try await Self.transactions(store)
        #expect(after.count == imported.count)
    }

    // MARK: - The steps

    @Test("The step strip goes back to any page passed, ahead only as far as the footer's button, and nowhere after the import")
    func reachableSteps() async throws {
        let (store, card, _) = try await Self.vault()
        let model = StatementImportModel(store: store, mappings: StatementMappingStore(defaults: try Self.defaults()))
        #expect(model.isReachable(.file))
        #expect(!model.isReachable(.mapping))
        #expect(!model.isReachable(.review))

        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        #expect(model.isReachable(.file))
        #expect(model.isReachable(.mapping))
        // Two wallets and none chosen yet: nothing to review.
        #expect(!model.isReachable(.review))

        model.walletId = card
        await model.refreshPreview()
        #expect(model.isReachable(.review))
        #expect(!model.isReachable(.report))

        // A date column the file does not have: the core refuses the
        // preview, and the review closes again.
        let dateColumn = model.mapping.dateColumn
        model.mapping.dateColumn = "nowhere"
        await model.refreshPreview()
        #expect(model.previewProblem != nil)
        #expect(!model.isReachable(.review))
        model.mapping.dateColumn = dateColumn
        await model.refreshPreview()

        model.step = .review
        await model.runImport()
        #expect(model.step == .report)
        #expect(!model.isReachable(.file))
        #expect(!model.isReachable(.mapping))
        #expect(!model.isReachable(.review))
        #expect(model.isReachable(.report))
    }

    // MARK: - Words

    @Test("Every status code the core sends has its own translation")
    func statusCodesAreTranslated() {
        let skipped = ["skipped_by_rule", "skipped_status", "needs_wallet", "zero_amount", "skipped_by_you"]
            .map(StatementText.skipped)
        let invalid = ["invalid_row", "invalid_date", "invalid_amount", "currency_mismatch"]
            .map(StatementText.invalid)
        #expect(Set(skipped).count == skipped.count)
        #expect(Set(invalid).count == invalid.count)
        #expect(!skipped.contains(StatementText.skipped("something_new")))
        #expect(!invalid.contains(StatementText.invalid("something_new")))
    }
}
