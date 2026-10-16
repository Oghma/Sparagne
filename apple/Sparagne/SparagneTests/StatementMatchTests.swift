import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The statement import against what the vault already holds, end to end
/// over an in-memory core: rows typed by hand that a statement row stands
/// for, kept, moved in from another wallet or imported anyway; the other
/// side of a transfer already recorded; description rules and the start day
/// as the core applies them. Every assertion is about what the review shows
/// or what the vault holds afterwards. No real statement is used.
struct StatementMatchTests {
    // MARK: - Fixtures

    private static let cardHeader = "timestamp,type,description,status,amount,currency,card,"
        + "card holder name,original amount,original currency,cashback earned,cashback currency,"
        + "category,spending mode"

    private static func cardRow(_ at: String, _ type: String, _ description: String, _ status: String, _ amount: String) -> String {
        "\(at),\(type),\(description),\(status),\(amount),EUR,1234,SAM TESTER,\(amount),EUR,,,,standard"
    }

    /// Lines count the header, so the first row is line 2.
    private static let card = ([cardHeader] + [
        cardRow("2026-09-17 08:15:33 UTC", "card_spend", "FORNO BIANCO", "CLEARED", "3.20"), // 2
        cardRow("2026-09-16 12:03:10 UTC", "card_spend", "METRO TICKET", "CLEARED", "2.10"), // 3
        cardRow("2026-09-13 10:00:00 UTC", "topup", "Top-up from bank", "", "150.00"), // 4
        cardRow("2026-09-11 18:22:41 UTC", "card_spend", "GREEN GROCER", "CLEARED", "14.75"), // 5
        cardRow("2026-09-10 19:42:07 UTC", "card_spend", "OSTERIA DEL PONTE", "PENDING", "42.50"), // 6
    ]).joined(separator: "\n") + "\n"

    /// A bank export with no type column: the top-up of the card going out,
    /// and a coffee.
    private static let bank = "Data;Importo;Descrizione\n"
        + "13/09/2026;-150,00;RICARICA CARTA 1234\n" // 2
        + "16/09/2026;-3,20;PAGAMENTO POS BAR CENTRALE\n" // 3

    private static func defaults() throws -> UserDefaults {
        try #require(UserDefaults(suiteName: "sparagne.tests.\(UUID().uuidString)"))
    }

    /// Vault `Main` with the card wallet `Card`, empty, and `Bank` holding
    /// 1000.00. Nothing typed yet.
    private static func vault() async throws -> (store: AppStore, card: Uuid, bank: Uuid) {
        let store = AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: try defaults())
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Card", openingBalance: 0)
        await store.createWallet(name: "Bank", openingBalance: 100_000)
        #expect(store.presentedError == nil)
        let card = try #require(store.wallets.first { $0.name == "Card" }).id
        let bank = try #require(store.wallets.first { $0.name == "Bank" }).id
        return (store, card, bank)
    }

    /// A line typed into the quick-add, which must be taken.
    private static func type(_ line: String, into store: AppStore) async {
        await store.submit(quickAdd: line)
        #expect(store.presentedError == nil, "the quick-add refused \(line)")
    }

    /// The card file read with its preset, aimed at the card and previewed.
    private static func cardModel(
        _ store: AppStore,
        card: Uuid,
        mappings: StatementMappingStore
    ) async throws -> StatementImportModel {
        let model = StatementImportModel(store: store, mappings: mappings)
        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        model.walletId = card
        await model.refreshPreview()
        #expect(model.previewProblem == nil)
        return model
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

    private static func balance(_ store: AppStore, _ wallet: Uuid) -> Int64? {
        store.wallets.first { $0.id == wallet }?.balance
    }

    // MARK: - A row typed by hand

    @Test("A spend typed by hand makes the statement's row a possible duplicate, kept as the one there, and the next import of the file reads it as imported before")
    func handRowKept() async throws {
        let (store, card, _) = try await Self.vault()
        await Self.type("-14.75 green grocer #Spesa @card 2026-09-11", into: store)
        let mappings = StatementMappingStore(defaults: try Self.defaults())
        let model = try await Self.cardModel(store, card: card, mappings: mappings)

        let grocer = try Self.row(model, line: 5)
        #expect(grocer.status == .possibleDuplicate)
        #expect(grocer.existing?.note == "green grocer")
        #expect(grocer.existing?.walletId == card)
        #expect(StatementImportModel.isFlagged(grocer))
        // In the statement's own wallet there is nothing to move.
        #expect(model.matchChoice(for: grocer) == .skip)
        #expect(model.availableChoices(for: grocer) == [.skip, .importAnyway])
        #expect(model.counts == .init(new: 2, alreadyImported: 0, skipped: 2, invalid: 0, rounded: 0, possibleDuplicates: 1))
        // A possible duplicate is no new entry: no category to give it.
        #expect(!StatementImportModel.takesCategory(grocer))

        await model.runImport()

        let report = try #require(model.report)
        #expect(report.executed == 2)
        #expect(report.possibleDuplicates == 1)
        #expect(report.moved == 0)
        #expect(report.rejected.isEmpty)
        let grocers = try await Self.transactions(store).filter { $0.amount == 1475 && !$0.voided }
        #expect(grocers.count == 1)
        #expect(grocers.first?.note == "green grocer")

        // The same file again: the row stands for the one typed, for good.
        let again = StatementImportModel(store: store, mappings: mappings)
        await again.load(data: Data(Self.card.utf8), fileName: "card.csv")
        #expect(again.walletId == card)
        await again.refreshPreview()
        let before = try Self.row(again, line: 5)
        #expect(before.status == .alreadyImported)
        #expect(before.existing == nil)
        #expect(before.matchChoice == nil)
        #expect(!StatementImportModel.isFlagged(before))
        #expect(again.counts.new == 0)
        #expect(again.counts.possibleDuplicates == 0)
    }

    @Test("Import anyway brings the possible duplicate back among the new rows, and the import writes it beside the one typed")
    func handRowImportedAnyway() async throws {
        let (store, card, _) = try await Self.vault()
        await Self.type("-14.75 green grocer #Spesa @card 2026-09-11", into: store)
        let model = try await Self.cardModel(store, card: card, mappings: StatementMappingStore(defaults: try Self.defaults()))
        let flagged = try Self.row(model, line: 5)

        model.setMatchChoice(.importAnyway, for: flagged)
        await model.refreshPreview()

        let anyway = try Self.row(model, line: 5)
        #expect(anyway.status == .new)
        #expect(anyway.matchChoice == .importAnyway)
        // The match stays beside it, so the review still shows it.
        #expect(anyway.existing?.note == "green grocer")
        #expect(StatementImportModel.isFlagged(anyway))
        #expect(model.matchChoice(for: anyway) == .importAnyway)
        #expect(model.counts.new == 3)
        #expect(model.counts.possibleDuplicates == 0)

        await model.runImport()

        #expect(model.report?.executed == 3)
        #expect(model.report?.possibleDuplicates == 0)
        let grocers = try await Self.transactions(store).filter { $0.amount == 1475 && $0.kind == .expense && !$0.voided }
        #expect(grocers.count == 2)
        #expect(Set(grocers.compactMap(\.note)) == ["green grocer", "GREEN GROCER"])
    }

    @Test("A spend typed into a wallet ticked under Duplicates also in is moved into the statement's wallet with its note and category")
    func handRowMovedIn() async throws {
        let (store, card, bank) = try await Self.vault()
        await Self.type("-14.75 green grocer #Spesa @bank 2026-09-11", into: store)
        #expect(Self.balance(store, bank) == 98_525)
        let model = try await Self.cardModel(store, card: card, mappings: StatementMappingStore(defaults: try Self.defaults()))

        // Not searched yet: the bank's spend is no business of the card's file.
        #expect(try Self.row(model, line: 5).status == .new)

        model.setAlsoChecked(true, wallet: bank)
        await model.refreshPreview()

        let grocer = try Self.row(model, line: 5)
        #expect(grocer.status == .possibleDuplicate)
        #expect(grocer.existing?.walletId == bank)
        #expect(grocer.matchChoice == .moveExisting)
        #expect(model.canMove(grocer))
        #expect(model.matchChoice(for: grocer) == .moveExisting)
        #expect(model.availableChoices(for: grocer) == [.moveExisting, .importAnyway])
        #expect(model.counts.moves == 1)
        #expect(model.counts.possibleDuplicates == 1)
        #expect(model.counts.new == 2)

        // Ticked off, it is neither moved nor kept: skipped, sent as a skip.
        #expect(StatementImportModel.takesTick(grocer))
        model.setIncluded(false, line: 5)
        #expect(model.counts.moves == 0)
        #expect(model.counts.possibleDuplicates == 0)
        #expect(model.counts.skipped == 3)
        #expect(model.overrides.first { $0.line == 5 } == StatementRowOverride(line: 5, category: nil, note: nil, skip: true))
        model.setIncluded(true, line: 5)

        await model.runImport()

        let report = try #require(model.report)
        #expect(report.moved == 1)
        #expect(report.executed == 2)
        #expect(report.rejected.isEmpty)
        let after = try await Self.transactions(store)
        let moved = try #require(after.first { $0.note == "green grocer" })
        #expect(moved.walletId == card)
        #expect(moved.category == "Spesa")
        #expect(moved.amount == 1475)
        // Out of the bank, into the card's own spending: counted once.
        #expect(Self.balance(store, bank) == 100_000)
        #expect(Self.balance(store, card) == -2_005) // 3.20 + 2.10 + 14.75 spent
    }

    // MARK: - The other side of a transfer

    @Test("A transfer typed between the two wallets makes the card's top-up a transfer already there, not imported, and the money moves once")
    func typedTransferMatched() async throws {
        let (store, card, bank) = try await Self.vault()
        await Self.type("tw> 150.00 @bank @card ricarica 2026-09-13", into: store)
        let model = try await Self.cardModel(store, card: card, mappings: StatementMappingStore(defaults: try Self.defaults()))

        let topup = try Self.row(model, line: 4)
        #expect(topup.status == .matchedTransfer)
        #expect(topup.existing?.kind == .transferWallet)
        #expect(topup.existing?.fromId == bank)
        #expect(topup.existing?.toId == card)
        #expect(topup.matchChoice == .skip)
        // A transfer has no wallet of its own to move from.
        #expect(!model.canMove(topup))
        #expect(model.counts.matchedTransfers == 1)
        #expect(model.counts.new == 3)

        await model.runImport()

        #expect(model.report?.matchedTransfers == 1)
        #expect(model.report?.executed == 3)
        let transfers = try await Self.transactions(store).filter { $0.kind == .transferWallet && !$0.voided }
        #expect(transfers.count == 1)
        #expect(Self.balance(store, bank) == 85_000)
        #expect(Self.balance(store, card) == 12_995) // 150.00 in, 20.05 spent
    }

    @Test("A description rule makes the bank's top-up of the card a transfer with a clean note, and the card's file then finds it instead of writing it twice")
    func ruleTransferThenMatched() async throws {
        let (store, card, bank) = try await Self.vault()
        let mappings = StatementMappingStore(defaults: try Self.defaults())

        let bankModel = StatementImportModel(store: store, mappings: mappings)
        await bankModel.load(data: Data(Self.bank.utf8), fileName: "banca.csv")
        bankModel.mapping.dateColumn = "Data"
        bankModel.mapping.amountColumn = "Importo"
        bankModel.mapping.descriptionColumns = ["Descrizione"]
        bankModel.walletId = bank
        bankModel.addDescriptionRule()
        bankModel.mapping.descriptionRules[0].contains = "ricarica carta"
        bankModel.mapping.descriptionRules[0].action = .transferOut(toWalletId: card)
        bankModel.mapping.descriptionRules[0].note = "Ricarica carta"
        await bankModel.refreshPreview()

        let recharge = try Self.row(bankModel, line: 2)
        #expect(recharge.status == .new)
        #expect(recharge.kind == .transferWallet)
        #expect(recharge.action == .transferOut(toWalletId: card))
        #expect(recharge.counterWalletId == card)
        #expect(recharge.descriptionRule == 0)
        #expect(recharge.note == "Ricarica carta")
        #expect(bankModel.matchCount(ofRule: 0) == 1)
        // The coffee keeps its sign's kind and its own text.
        let coffee = try Self.row(bankModel, line: 3)
        #expect(coffee.kind == .expense)
        #expect(coffee.descriptionRule == nil)
        #expect(coffee.note == "PAGAMENTO POS BAR CENTRALE")

        await bankModel.runImport()

        #expect(bankModel.report?.executed == 2)
        let written = try await Self.transactions(store)
        let transfer = try #require(written.first { $0.kind == .transferWallet })
        #expect(transfer.fromId == bank)
        #expect(transfer.toId == card)
        #expect(transfer.note == "Ricarica carta")

        // The card's file: its top-up is that transfer's other side.
        let cardModel = try await Self.cardModel(store, card: card, mappings: mappings)
        #expect(try Self.row(cardModel, line: 4).status == .matchedTransfer)

        await cardModel.runImport()

        #expect(cardModel.report?.matchedTransfers == 1)
        let transfers = try await Self.transactions(store).filter { $0.kind == .transferWallet && !$0.voided }
        #expect(transfers.count == 1)
        #expect(Self.balance(store, bank) == 84_680) // 1000.00 less 150.00 and 3.20
        #expect(Self.balance(store, card) == 12_995) // 150.00 in, 20.05 spent
    }

    // MARK: - An entry standing for a transfer

    /// A bank export of one row, by its sign.
    private static func bankFile(_ row: String) -> String {
        "Data;Importo;Descrizione\n\(row)\n"
    }

    /// `text` read as a continental bank export into `wallet`, with
    /// `rules`, and previewed.
    private static func bankModel(
        _ store: AppStore,
        _ text: String,
        into wallet: Uuid,
        rules: [StatementDescriptionRule] = [],
        mappings: StatementMappingStore
    ) async throws -> StatementImportModel {
        let model = StatementImportModel(store: store, mappings: mappings)
        await model.load(data: Data(text.utf8), fileName: "banca.csv")
        model.mapping.dateColumn = "Data"
        model.mapping.amountColumn = "Importo"
        model.mapping.descriptionColumns = ["Descrizione"]
        model.mapping.descriptionRules = rules
        model.walletId = wallet
        await model.refreshPreview()
        #expect(model.previewProblem == nil)
        return model
    }

    /// Vault `Main` with `Revolut`, empty, and `ING` holding 1000.00;
    /// Revolut's export imported first, with no rule, so the 200.00 from ING
    /// is an income there. Then ING's, previewed, whose rule knows the money
    /// went to Revolut.
    /// `rules` default to the one knowing the money went to Revolut;
    /// `before` runs once Revolut's file is in, before ING's is read.
    private static func revolutThenIng(
        rules: [StatementDescriptionRule]? = nil,
        ingRow: String = "01/10/2026;-200,00;BONIFICO A REVOLUT",
        before: (AppStore) async -> Void = { _ in }
    ) async throws -> (
        store: AppStore, revolut: Uuid, ing: Uuid, model: StatementImportModel
    ) {
        let store = AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: try Self.defaults())
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Revolut", openingBalance: 0)
        await store.createWallet(name: "ING", openingBalance: 100_000)
        let revolut = try #require(store.wallets.first { $0.name == "Revolut" }).id
        let ing = try #require(store.wallets.first { $0.name == "ING" }).id
        let mappings = StatementMappingStore(defaults: try Self.defaults())

        let first = try await Self.bankModel(
            store, Self.bankFile("01/10/2026;200,00;BONIFICO DA ING"), into: revolut, mappings: mappings
        )
        #expect(try Self.row(first, line: 2).kind == .income)
        await first.runImport()
        #expect(first.report?.executed == 1)
        #expect(Self.balance(store, revolut) == 20_000)
        await before(store)

        let rule = StatementDescriptionRule(contains: "revolut", action: .transferOut(toWalletId: revolut))
        let model = try await Self.bankModel(
            store, Self.bankFile(ingRow), into: ing, rules: rules ?? [rule], mappings: mappings
        )
        return (store, revolut, ing, model)
    }

    @Test("A bank's transfer whose other side another bank's file imported as an income replaces that income by default, and the money moves once")
    func transferReplacesIncome() async throws {
        let (store, revolut, ing, model) = try await Self.revolutThenIng()
        let row = try Self.row(model, line: 2)
        #expect(row.status == .possibleDuplicate)
        #expect(row.kind == .transferWallet)
        #expect(row.counterWalletId == revolut)
        #expect(row.existing?.kind == .income)
        #expect(row.existing?.walletId == revolut)
        #expect(row.matchChoice == .replaceExisting)
        #expect(StatementImportModel.replaces(row))
        #expect(model.availableChoices(for: row) == [.replaceExisting, .importAnyway])
        #expect(model.defaultChoice(for: row) == .replaceExisting)
        #expect(model.overrides.isEmpty)
        // Written, though not new: the import counts it among its rows.
        #expect(model.counts == .init(new: 0, alreadyImported: 0, skipped: 0, invalid: 0, rounded: 0, replaces: 1))
        #expect(model.counts.written == 1)
        // The line under it names what goes: an income in Revolut.
        let line = StatementText.existing(try #require(row.existing), names: NameBook(snapshot: store.snapshot), replacing: true)
        #expect(line.contains("BONIFICO DA ING"))
        #expect(line.contains(StatementText.kind(.income)))
        #expect(line.contains("Revolut"))

        await model.runImport()

        let report = try #require(model.report)
        #expect(report.executed == 1)
        #expect(report.rejected.isEmpty)
        #expect(model.replaced == 1)
        let all = try await Self.transactions(store)
        let income = try #require(all.first { $0.kind == .income && $0.amount == 20_000 })
        #expect(income.voided)
        let transfers = all.filter { $0.kind == .transferWallet && !$0.voided }
        #expect(transfers.count == 1)
        #expect(transfers.first?.fromId == ing)
        #expect(transfers.first?.toId == revolut)
        #expect(Self.balance(store, revolut) == 20_000)
        #expect(Self.balance(store, ing) == 80_000)
    }

    @Test("A transfer row standing for an entry, ticked off, writes nothing: no transfer, the entry left as it is")
    func replaceRowLeftOut() async throws {
        let (store, revolut, ing, model) = try await Self.revolutThenIng()
        let row = try Self.row(model, line: 2)
        #expect(StatementImportModel.replaces(row))
        #expect(StatementImportModel.takesTick(row))

        model.setIncluded(false, line: 2)

        #expect(model.counts.replaces == 0)
        #expect(model.counts.skipped == 1)
        #expect(model.counts.written == 0)
        #expect(model.overrides == [StatementRowOverride(line: 2, category: nil, note: nil, skip: true)])

        await model.runImport()

        let report = try #require(model.report)
        #expect(report.executed == 0)
        #expect(report.rejected.isEmpty)
        #expect(model.replaced == 0)
        let all = try await Self.transactions(store)
        #expect(all.first { $0.kind == .income && $0.amount == 20_000 }?.voided == false)
        #expect(!all.contains { $0.kind == .transferWallet })
        #expect(Self.balance(store, revolut) == 20_000)
        #expect(Self.balance(store, ing) == 100_000)
    }

    @Test("A cash withdrawal to Cash and an unrelated cash income typed two days later: the income may be its other side, so the transfer comes in beside it by default and both stay")
    func withdrawalBesideTypedIncome() async throws {
        let store = AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: try Self.defaults())
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Bank", openingBalance: 100_000)
        await store.createWallet(name: "Cash", openingBalance: 0)
        let bank = try #require(store.wallets.first { $0.name == "Bank" }).id
        let cash = try #require(store.wallets.first { $0.name == "Cash" }).id
        await Self.type("+50.00 regalo @cash 2026-10-03", into: store)

        let rule = StatementDescriptionRule(contains: "prelievo", action: .transferOut(toWalletId: cash))
        let model = try await Self.bankModel(
            store, Self.bankFile("01/10/2026;-50,00;PRELIEVO ATM"), into: bank, rules: [rule],
            mappings: StatementMappingStore(defaults: try Self.defaults())
        )

        let row = try Self.row(model, line: 2)
        #expect(row.status == .new)
        #expect(row.kind == .transferWallet)
        #expect(row.existing?.note == "regalo")
        #expect(row.existing?.walletId == cash)
        #expect(row.matchChoice == .importAnyway)
        #expect(StatementImportModel.isReplace(row))
        #expect(StatementImportModel.isFlagged(row))
        #expect(model.defaultChoice(for: row) == .importAnyway)
        #expect(model.availableChoices(for: row) == [.replaceExisting, .importAnyway])
        // The review says the income may be the withdrawal's other side.
        #expect(model.mayReplace(row))
        #expect(!model.importsByDefault(row))
        #expect(model.counts.new == 1)
        #expect(model.counts.replaces == 0)
        #expect(model.overrides.isEmpty)

        await model.runImport()

        #expect(model.report?.executed == 1)
        #expect(model.replaced == 0)
        let all = try await Self.transactions(store)
        #expect(all.first { $0.note == "regalo" }?.voided == false)
        #expect(all.filter { $0.kind == .transferWallet && !$0.voided }.count == 1)
        #expect(Self.balance(store, cash) == 10_000) // the gift and the withdrawal
        #expect(Self.balance(store, bank) == 95_000)
    }

    @Test("A choice made over one match is not carried to the next: a row imported anyway beside a typed spend, then made a transfer to Revolut, replaces Revolut's income by default instead of landing beside it")
    func choiceForgottenWithTheKind() async throws {
        let (store, revolut, ing, model) = try await Self.revolutThenIng(
            rules: [], ingRow: "01/10/2026;-200,00;BONIFICO"
        ) { store in
            await Self.type("-200.00 affitto @ing 2026-09-29", into: store)
        }
        let spend = try Self.row(model, line: 2)
        #expect(spend.status == .possibleDuplicate)
        #expect(spend.existing?.note == "affitto")
        model.setMatchChoice(.importAnyway, for: spend)
        await model.refreshPreview()
        #expect(try Self.row(model, line: 2).matchChoice == .importAnyway)

        model.setAction(.transferOut(toWalletId: revolut), for: try Self.row(model, line: 2))
        #expect(model.matchChoices.isEmpty)
        await model.refreshPreview()

        let transfer = try Self.row(model, line: 2)
        #expect(transfer.existing?.walletId == revolut)
        #expect(transfer.matchChoice == .replaceExisting)
        #expect(model.matchChoice(for: transfer) == .replaceExisting)
        #expect(model.overrides == [
            StatementRowOverride(line: 2, category: nil, note: nil, skip: false, action: .transferOut(toWalletId: revolut)),
        ])

        await model.runImport()

        let all = try await Self.transactions(store)
        #expect(all.first { $0.kind == .income && $0.amount == 20_000 }?.voided == true)
        #expect(all.filter { $0.kind == .transferWallet && !$0.voided }.count == 1)
        // Counted once in Revolut; the rent typed in ING stays.
        #expect(Self.balance(store, revolut) == 20_000)
        #expect(Self.balance(store, ing) == 60_000)
    }

    @Test("A By sign row matched to a transfer typed between the two wallets takes an expense of the user's")
    func bySignMatchedTransfer() async throws {
        let (store, card, bank) = try await Self.vault()
        await Self.type("tw> 150.00 @bank @card ricarica 2026-09-13", into: store)
        let model = try await Self.bankModel(
            store, Self.bankFile("13/09/2026;-150,00;RICARICA CARTA"), into: bank,
            mappings: StatementMappingStore(defaults: try Self.defaults())
        )
        let row = try Self.row(model, line: 2)
        #expect(row.status == .matchedTransfer)
        #expect(row.kind == .transferWallet)
        #expect(row.action == .bySign)
        #expect(row.counterWalletId == card)

        // The cell says transfer: an expense is the user's own choice, kept.
        model.setAction(.expense, for: row)
        #expect(model.editedActions[2] == .expense)
    }

    // MARK: - Near the file's edges

    @Test("A row whose match another export's row stands for, near the file's first day, is imported by default, and the review says why")
    func nearEdgeImportedByDefault() async throws {
        let (store, card, _) = try await Self.vault()
        let mappings = StatementMappingStore(defaults: try Self.defaults())
        let first = try await Self.cardModel(store, card: card, mappings: mappings)
        await first.runImport()
        #expect(first.report?.executed == 3)

        // The next export starts on the last day of the first: its own
        // 3.20 a day later may be the same bread, or the next one.
        let next = ([Self.cardHeader] + [
            Self.cardRow("2026-09-18 09:00:00 UTC", "card_spend", "FORNO B.", "CLEARED", "3.20"), // 2
            Self.cardRow("2026-09-17 07:00:00 UTC", "card_spend", "EDICOLA", "CLEARED", "1.50"), // 3
        ]).joined(separator: "\n") + "\n"
        let model = StatementImportModel(store: store, mappings: mappings)
        await model.load(data: Data(next.utf8), fileName: "card-2.csv")
        #expect(model.walletId == card)
        await model.refreshPreview()

        let bread = try Self.row(model, line: 2)
        #expect(bread.status == .new)
        #expect(bread.existing?.note == "FORNO BIANCO")
        #expect(bread.matchChoice == .importAnyway)
        #expect(StatementImportModel.isFlagged(bread))
        #expect(model.defaultChoice(for: bread) == .importAnyway)
        #expect(model.importsByDefault(bread))
        #expect(model.availableChoices(for: bread) == [.skip, .importAnyway])
        #expect(model.counts.new == 2)
        #expect(model.overrides.isEmpty)

        // Kept instead: the bread already there stands for it.
        model.setMatchChoice(.skip, for: bread)
        await model.refreshPreview()
        let kept = try Self.row(model, line: 2)
        #expect(kept.status == .possibleDuplicate)
        #expect(!model.importsByDefault(kept))
        #expect(model.defaultChoice(for: kept) == .importAnyway)
        #expect(model.counts.new == 1)

        // Back to the default: nothing sent.
        model.setMatchChoice(.importAnyway, for: kept)
        #expect(model.matchChoices.isEmpty)
    }

    // MARK: - The start day

    @Test("A start day skips the rows dated before it, which the review says in words and offers no kind for")
    func startDay() async throws {
        let (store, card, _) = try await Self.vault()
        let model = try await Self.cardModel(store, card: card, mappings: StatementMappingStore(defaults: try Self.defaults()))
        #expect(try Self.row(model, line: 5).status == .new)

        model.since = "2026-09-12"
        await model.refreshPreview()

        let grocer = try Self.row(model, line: 5)
        guard case .skipped(let code, _) = grocer.status else {
            Issue.record("the grocer, a day before the start, should be skipped, not \(grocer.status)")
            return
        }
        #expect(code == "before_start")
        #expect(StatementText.status(grocer.status) == StatementText.skipped("before_start"))
        #expect(!StatementImportModel.takesAction(grocer))
        // The rows from the start day on are untouched.
        #expect(try Self.row(model, line: 2).status == .new)
        #expect(try Self.row(model, line: 3).status == .new)
        #expect(model.counts.new == 2)

        await model.runImport()

        #expect(model.report?.executed == 2)
        let imported = try await Self.transactions(store)
        #expect(!imported.contains { $0.note == "GREEN GROCER" })
        #expect(imported.contains { $0.note == "METRO TICKET" })
    }
}
