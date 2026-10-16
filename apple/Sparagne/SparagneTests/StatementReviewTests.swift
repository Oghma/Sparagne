import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// What the review lets the user change about each row (kind, note,
/// category, envelope, what becomes of a match), the description rules,
/// and where a file went, remembered for the next one. Every assertion is
/// about what the model sends the core or keeps, over an in-memory core;
/// none needs the core to act on the new choices. No real statement is used.
struct StatementReviewTests {
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

    private static let cardHeaders = cardHeader.components(separatedBy: ",")

    /// A bank export with no type column: every row by its sign.
    private static let bank = "Data;Importo;Descrizione\n"
        + "16/09/2026;-3,20;PAGAMENTO POS BAR CENTRALE\n" // 2
        + "17/09/2026;1500,00;STIPENDIO SETTEMBRE\n" // 3

    private static func defaults() throws -> UserDefaults {
        try #require(UserDefaults(suiteName: "sparagne.tests.\(UUID().uuidString)"))
    }

    /// Vault `Main` with the wallets `Card` and `Bank` and the envelope
    /// `Casa`. A past `forno bianco` filed under Bakery, in Card, gives the
    /// history something to suggest.
    private static func vault() async throws -> (store: AppStore, card: Uuid, bank: Uuid, casa: Uuid) {
        let store = AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: try defaults())
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Card", openingBalance: 0)
        await store.submit(quickAdd: "-3.20 forno bianco #Bakery")
        await store.createWallet(name: "Bank", openingBalance: 100_000)
        await store.createEnvelope(name: "Casa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        #expect(store.presentedError == nil)
        let card = try #require(store.wallets.first { $0.name == "Card" }).id
        let bank = try #require(store.wallets.first { $0.name == "Bank" }).id
        let casa = try #require(store.flows.first { $0.name == "Casa" }).id
        return (store, card, bank, casa)
    }

    /// The card file read, aimed at `wallet` and previewed.
    private static func cardModel(
        _ store: AppStore,
        wallet: Uuid,
        mappings: StatementMappingStore? = nil
    ) async throws -> StatementImportModel {
        let model = StatementImportModel(store: store, mappings: try mappings ?? StatementMappingStore(defaults: defaults()))
        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        model.walletId = wallet
        await model.refreshPreview()
        #expect(model.previewProblem == nil)
        return model
    }

    private static func row(_ model: StatementImportModel, line: UInt32) throws -> StatementRow {
        try #require(model.preview?.rows.first { $0.line == line })
    }

    private static func transactions(_ store: AppStore) async throws -> [TransactionView] {
        let vaultId = try #require(store.currentVault?.id)
        let filter = TransactionFilter(includeVoided: true, includeTransfers: true, ascending: true)
        return try await store.core.transactions(vaultId: vaultId, filter: filter, limit: 500, cursor: nil).items
    }

    private static func sorted(_ overrides: [StatementRowOverride]) -> [StatementRowOverride] {
        overrides.sorted { $0.line < $1.line }
    }

    // MARK: - Overrides

    @Test("Each edit of a row goes to the import as one override carrying only what changed")
    func overridesFromEdits() async throws {
        let (store, card, bank, casa) = try await Self.vault()
        let model = try await Self.cardModel(store, wallet: card)
        let forno = try Self.row(model, line: 2)
        let metro = try Self.row(model, line: 3)
        let topup = try Self.row(model, line: 4)
        let grocer = try Self.row(model, line: 5)
        #expect(metro.note == "METRO TICKET")

        model.setAction(.income, for: metro)
        model.setNote("Metro", for: 3)
        model.setAction(.transferIn(fromWalletId: bank), for: topup)
        model.setFlow(casa, for: grocer)
        model.setCategory("Spesa", for: 5)
        model.setMatchChoice(.importAnyway, for: forno)

        #expect(Self.sorted(model.overrides) == [
            // The history's suggestion travels with the choice over a match.
            StatementRowOverride(line: 2, category: "Bakery", note: nil, skip: false, onMatch: .importAnyway),
            StatementRowOverride(line: 3, category: nil, note: "Metro", skip: false, action: .income),
            // The top-up waits for its wallet, so it is not new: only its kind goes.
            StatementRowOverride(line: 4, category: nil, note: nil, skip: false, action: .transferIn(fromWalletId: bank)),
            StatementRowOverride(line: 5, category: "Spesa", note: nil, skip: false, flowId: casa),
        ])

        // Put back as the row was, each edit sends nothing.
        let unallocated = try #require(store.flows.first { $0.isUnallocated }).id
        model.setAction(.expense, for: metro)
        model.setNote("METRO TICKET", for: 3)
        model.setAction(.transferIn(fromWalletId: nil), for: topup)
        model.setFlow(unallocated, for: grocer)
        model.setCategory("", for: 5)
        model.setMatchChoice(.skip, for: forno)

        #expect(model.overrides == [StatementRowOverride(line: 2, category: "Bakery", note: nil, skip: false)])
        #expect(model.editedActions.isEmpty)
        #expect(model.editedFlows.isEmpty)
        #expect(model.matchChoices.isEmpty)

        // A row ticked off is only skipped: what was typed into it stays home.
        model.setNote("Metro", for: 3)
        model.setIncluded(false, line: 3)
        #expect(Self.sorted(model.overrides).last == StatementRowOverride(line: 3, category: nil, note: nil, skip: true))
    }

    @Test("A row's envelope is sent only while it differs from the file's")
    func envelopeAgainstTheFile() async throws {
        let (store, card, _, casa) = try await Self.vault()
        let model = try await Self.cardModel(store, wallet: card)
        let grocer = try Self.row(model, line: 5)

        model.setFlow(casa, for: grocer)
        #expect(model.flow(for: grocer) == casa)
        #expect(model.previewInput?.rowChoices == [
            StatementRowOverride(line: 5, category: nil, note: nil, skip: false, flowId: casa),
        ])

        // The whole file goes to Casa now: the row's own Casa says nothing more.
        model.flowId = casa
        #expect(model.previewInput?.rowChoices == [])
        #expect(model.overrides.allSatisfy { $0.flowId == nil })

        // Unallocated, for one row of a file that goes to Casa, is the
        // vault's real Unallocated id.
        let unallocated = try #require(store.flows.first { $0.isUnallocated }).id
        model.setFlow(unallocated, for: grocer)
        #expect(model.previewInput?.rowChoices == [
            StatementRowOverride(line: 5, category: nil, note: nil, skip: false, flowId: unallocated),
        ])
    }

    @Test("A kind put back to what the rules say, or to what By sign resolved it to, is forgotten")
    func bySignDefault() async throws {
        let (store, card, _, _) = try await Self.vault()
        let model = StatementImportModel(store: store, mappings: StatementMappingStore(defaults: try Self.defaults()))
        await model.load(data: Data(Self.bank.utf8), fileName: "banca.csv")
        model.mapping.dateColumn = "Data"
        model.mapping.amountColumn = "Importo"
        model.mapping.descriptionColumns = ["Descrizione"]
        model.walletId = card
        await model.refreshPreview()
        let pos = try Self.row(model, line: 2)
        let salary = try Self.row(model, line: 3)
        #expect(pos.action == .bySign && pos.outflow)
        #expect(salary.action == .bySign && !salary.outflow)

        model.setAction(.expense, for: pos)
        model.setAction(.income, for: salary)
        #expect(model.editedActions.isEmpty)

        model.setAction(.income, for: pos)
        model.setAction(.refund, for: salary)
        #expect(model.editedActions == [2: .income, 3: .refund])

        model.setAction(.bySign, for: pos)
        #expect(model.overrides == [StatementRowOverride(line: 3, category: nil, note: nil, skip: false, action: .refund)])
    }

    // MARK: - What re-runs the preview

    @Test("The preview runs again on a kind, an envelope or a choice over a match, not on a tick, a note or a category")
    func previewInputChanges() async throws {
        let (store, card, bank, casa) = try await Self.vault()
        let model = try await Self.cardModel(store, wallet: card)
        let forno = try Self.row(model, line: 2)
        let metro = try Self.row(model, line: 3)
        let grocer = try Self.row(model, line: 5)

        let start = try #require(model.previewInput)
        model.setIncluded(false, line: 3)
        model.setNote("Metro", for: 3)
        model.setCategory("Transport", for: 3)
        #expect(model.previewInput == start)

        model.setAction(.income, for: metro)
        let kind = try #require(model.previewInput)
        #expect(kind != start)
        #expect(kind.rowChoices == [StatementRowOverride(line: 3, category: nil, note: nil, skip: false, action: .income)])

        model.setMatchChoice(.importAnyway, for: forno)
        let choice = try #require(model.previewInput)
        #expect(choice != kind)
        #expect(choice.rowChoices.first == StatementRowOverride(line: 2, category: nil, note: nil, skip: false, onMatch: .importAnyway))

        model.setFlow(casa, for: grocer)
        #expect(model.previewInput != choice)

        // The wallets also searched and the start day are options of the file.
        model.setAlsoChecked(true, wallet: bank)
        model.setAlsoChecked(true, wallet: card)
        model.since = "2026-09-12"
        #expect(model.previewInput?.options.alsoCheckWallets == [bank])
        #expect(model.previewInput?.options.since == "2026-09-12")

        // The preview takes them all; the stubbed core only reads them.
        await model.refreshPreview()
        #expect(model.previewProblem == nil)

        // The bank made the target is always searched, so it is no longer ticked.
        model.walletId = bank
        #expect(!model.isAlsoChecked(bank))
        #expect(model.previewInput?.options.alsoCheckWallets == [])
    }

    // MARK: - Remembered

    @Test("A mapping saved before description rules existed reads back with none, and one with rules round-trips")
    func oldMappingDecodes() throws {
        let defaults = try Self.defaults()
        let headers = ["Data", "Importo", "Descrizione"]
        let key = StatementMappingStore.key(vaultId: "v-1", headers: headers)
        let old = #"{"delimiter":";","date_column":"Data","date_format":{"kind":"day_month_year"},"#
            + #""amount_column":"Importo","amount_sign":"outflow_negative","decimal_comma":true,"#
            + #""description_columns":["Descrizione"],"type_column":null,"type_rules":[],"#
            + #""default_action":{"kind":"by_sign"},"status_column":null,"skip_statuses":[],"#
            + #""currency_column":null,"category_column":null,"original_amount_column":null,"#
            + #""original_currency_column":null}"#
        defaults.set([key: old], forKey: StatementMappingStore.defaultsKey)
        let mappings = StatementMappingStore(defaults: defaults)

        let mapping = try #require(mappings.mapping(vaultId: "v-1", headers: headers))
        #expect(mapping.descriptionRules.isEmpty)
        #expect(mapping.dateColumn == "Data")
        #expect(mapping.decimalComma)
        // Nothing was remembered of where it went.
        #expect(mappings.target(vaultId: "v-1", headers: headers) == nil)

        var ruled = mapping
        ruled.descriptionRules = [
            StatementDescriptionRule(contains: "BAR CENTRALE", action: .expense, category: "Bar", note: "Bar Centrale"),
            StatementDescriptionRule(contains: "STIPENDIO", note: "Stipendio"),
        ]
        mappings.remember(ruled, vaultId: "v-1", headers: headers)
        #expect(mappings.mapping(vaultId: "v-1", headers: headers) == ruled)

        // A target stored with fields missing, or with one this version
        // does not keep, still reads.
        defaults.set([key: #"{"walletId":"w-1","alsoCheckWallets":["w-2"]}"#], forKey: StatementMappingStore.targetsKey)
        #expect(mappings.target(vaultId: "v-1", headers: headers) == StatementImportTarget(walletId: "w-1", flowId: nil))
    }

    @Test("Where a file went is remembered at the import and comes back with the next file, as far as it still exists")
    func targetRoundTrip() async throws {
        let (store, card, bank, casa) = try await Self.vault()
        await store.createWallet(name: "Old", openingBalance: 0)
        let old = try #require(store.wallets.first { $0.name == "Old" }).id
        let mappings = StatementMappingStore(defaults: try Self.defaults())
        let vaultId = try #require(store.currentVault?.id)

        let model = try await Self.cardModel(store, wallet: card, mappings: mappings)
        model.flowId = casa
        model.setAlsoChecked(true, wallet: bank)
        model.since = "2026-09-01"
        await model.refreshPreview()
        await model.runImport()
        #expect(model.problem == nil)
        // The wallets also searched are not part of it.
        #expect(mappings.target(vaultId: vaultId, headers: Self.cardHeaders) == StatementImportTarget(
            walletId: card, flowId: casa, since: "2026-09-01"
        ))

        // Three wallets: without the memory the target would be the user's to choose.
        let again = StatementImportModel(store: store, mappings: mappings)
        await again.load(data: Data(Self.card.utf8), fileName: "card.csv")
        #expect(again.walletId == card)
        #expect(again.flowId == casa)
        #expect(again.since == "2026-09-01")
        #expect(!again.isAlsoChecked(bank))
        #expect(again.previewInput?.options.alsoCheckWallets == [])

        // Another file in the same sheet starts with none ticked either.
        again.setAlsoChecked(true, wallet: bank)
        await again.load(data: Data(Self.card.utf8), fileName: "card.csv")
        #expect(!again.isAlsoChecked(bank))

        // A wallet archived and an envelope gone since are left out.
        await store.archiveWallet(old)
        #expect(!store.wallets.contains { $0.id == old })
        mappings.remember(
            StatementImportTarget(walletId: old, flowId: UUID().uuidString.lowercased()),
            vaultId: vaultId,
            headers: Self.cardHeaders
        )
        let third = StatementImportModel(store: store, mappings: mappings)
        await third.load(data: Data(Self.card.utf8), fileName: "card.csv")
        #expect(third.walletId == nil)
        #expect(third.flowId == nil)
        #expect(third.since == nil)
    }

    // MARK: - Transfers that would point at the target

    @Test("A transfer naming the new target forgets its wallet, in a description rule and in a row's own kind")
    func releaseRuleAndRowTransfers() async throws {
        let (store, card, bank, _) = try await Self.vault()
        let model = try await Self.cardModel(store, wallet: card)
        let metro = try Self.row(model, line: 3)
        model.mapping.descriptionRules = [
            StatementDescriptionRule(contains: "Top-up", action: .transferIn(fromWalletId: bank)),
            StatementDescriptionRule(contains: "GROCER", note: "Grocer"),
        ]
        model.setAction(.transferOut(toWalletId: bank), for: metro)

        model.walletId = bank

        #expect(model.mapping.descriptionRules == [
            StatementDescriptionRule(contains: "Top-up", action: .transferIn(fromWalletId: nil)),
            StatementDescriptionRule(contains: "GROCER", note: "Grocer"),
        ])
        #expect(model.editedActions[3] == .transferOut(toWalletId: nil))
    }

    @Test("A remembered description rule naming a wallet gone or the target is adopted without it")
    func adoptDescriptionRules() async throws {
        let (store, card, bank, _) = try await Self.vault()
        let mappings = StatementMappingStore(defaults: try Self.defaults())
        let vaultId = try #require(store.currentVault?.id)
        let first = try await Self.cardModel(store, wallet: card, mappings: mappings)
        var mapping = first.mapping
        mapping.descriptionRules = [
            StatementDescriptionRule(contains: "A", action: .transferIn(fromWalletId: UUID().uuidString.lowercased())),
            StatementDescriptionRule(contains: "B", action: .transferOut(toWalletId: bank)),
            StatementDescriptionRule(contains: "C", action: .transferOut(toWalletId: card)),
            StatementDescriptionRule(contains: "D", category: "Bar"),
        ]
        mappings.remember(mapping, vaultId: vaultId, headers: Self.cardHeaders)
        mappings.remember(StatementImportTarget(walletId: card, flowId: nil), vaultId: vaultId, headers: Self.cardHeaders)

        let model = StatementImportModel(store: store, mappings: mappings)
        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")

        #expect(model.source == .remembered)
        #expect(model.walletId == card)
        #expect(model.mapping.descriptionRules.map(\.action) == [
            .transferIn(fromWalletId: nil),
            .transferOut(toWalletId: bank),
            .transferOut(toWalletId: nil),
            nil,
        ])
    }

    // MARK: - Description rules

    @Test("A rule made from a row takes its text, kind, category and clean note, and drives the row in place of its edits")
    func makeRuleFromRow() async throws {
        let (store, card, _, casa) = try await Self.vault()
        let model = try await Self.cardModel(store, wallet: card)
        let forno = try Self.row(model, line: 2)
        let metro = try Self.row(model, line: 3)
        let topup = try Self.row(model, line: 4)

        model.setAction(.refund, for: metro)
        model.setCategory("Transport", for: 3)
        model.setNote("Metro", for: 3)
        model.setFlow(casa, for: metro)
        #expect(model.makeRule(from: metro) == 0)
        #expect(model.mapping.descriptionRules == [
            StatementDescriptionRule(contains: "METRO TICKET", action: .refund, category: "Transport", note: "Metro"),
        ])
        #expect(model.editedActions[3] == nil)
        #expect(model.editedCategories[3] == nil)
        #expect(model.editedNotes[3] == nil)
        // Its envelope is not the rule's business.
        #expect(model.editedFlows[3] == casa)

        // The bank's own text is no note to stamp on other rows; the
        // history's suggestion becomes the category.
        #expect(model.makeRule(from: forno) == 1)
        #expect(model.mapping.descriptionRules[1] == StatementDescriptionRule(
            contains: "FORNO BIANCO", action: .expense, category: "Bakery", note: nil
        ))

        // A transfer has no category.
        #expect(model.makeRule(from: topup) == 2)
        #expect(model.mapping.descriptionRules[2] == StatementDescriptionRule(
            contains: "Top-up from bank", action: .transferIn(fromWalletId: nil), category: nil, note: nil
        ))

        // A row another rule decided gets the new one ahead of it.
        var decided = metro
        decided.descriptionRule = 1
        #expect(model.makeRule(from: decided) == 1)
        #expect(model.mapping.descriptionRules.map(\.contains) == [
            "METRO TICKET", "METRO TICKET", "FORNO BIANCO", "Top-up from bank",
        ])

        model.removeDescriptionRule(at: 1)
        model.addDescriptionRule()
        #expect(model.mapping.descriptionRules.map(\.contains) == [
            "METRO TICKET", "FORNO BIANCO", "Top-up from bank", "",
        ])
        #expect(model.mapping.descriptionRules.last == StatementDescriptionRule(contains: ""))
    }

    // MARK: - Matches

    @Test("A flagged row can move only a match entered in another wallet, which is then its default")
    func matchChoices() async throws {
        let (store, card, bank, _) = try await Self.vault()
        let model = try await Self.cardModel(store, wallet: card)
        let all = try await Self.transactions(store)
        let existing = try #require(all.first { $0.note == "forno bianco" })
        #expect(existing.walletId == card)
        #expect(!StatementImportModel.isFlagged(try Self.row(model, line: 3)))

        var duplicate = try Self.row(model, line: 2)
        duplicate.status = .possibleDuplicate
        duplicate.existing = existing
        duplicate.matchChoice = .skip
        #expect(StatementImportModel.isFlagged(duplicate))
        #expect(!model.canMove(duplicate))
        #expect(model.defaultChoice(for: duplicate) == .skip)
        #expect(model.availableChoices(for: duplicate) == [.skip, .importAnyway])

        // The same match seen from the bank's statement sits in another wallet.
        model.walletId = bank
        duplicate.matchChoice = .moveExisting
        #expect(model.canMove(duplicate))
        #expect(model.defaultChoice(for: duplicate) == .moveExisting)
        #expect(model.availableChoices(for: duplicate) == [.skip, .moveExisting, .importAnyway])
        #expect(model.matchChoice(for: duplicate) == .moveExisting)

        model.setMatchChoice(.moveExisting, for: duplicate)
        #expect(model.matchChoices.isEmpty)
        model.setMatchChoice(.importAnyway, for: duplicate)
        #expect(model.matchChoice(for: duplicate) == .importAnyway)

        // Imported anyway, the row comes back new with its match, still flagged.
        var anyway = duplicate
        anyway.status = .new
        anyway.matchChoice = .importAnyway
        #expect(StatementImportModel.isFlagged(anyway))

        // The other side of a transfer has no wallet of its own to move from.
        var transfer = duplicate
        transfer.status = .matchedTransfer
        transfer.existing?.kind = .transferWallet
        transfer.existing?.walletId = nil
        transfer.existing?.fromId = card
        transfer.existing?.toId = bank
        #expect(!model.canMove(transfer))
        #expect(model.availableChoices(for: transfer) == [.skip, .importAnyway])
    }

    @Test("Only a row the rules decided offers another kind")
    func whichRowsTakeAKind() async throws {
        let (store, card, _, _) = try await Self.vault()
        let model = try await Self.cardModel(store, wallet: card)
        #expect(StatementImportModel.takesAction(try Self.row(model, line: 2)))
        // Waiting for its wallet: a kind of its own can bring it in.
        #expect(StatementImportModel.takesAction(try Self.row(model, line: 4)))
        // Skipped for its status, which no kind overrides.
        #expect(!StatementImportModel.takesAction(try Self.row(model, line: 6)))
        var imported = try Self.row(model, line: 2)
        imported.status = .alreadyImported
        #expect(!StatementImportModel.takesAction(imported))
        imported.status = .skipped(code: "before_start", reason: "")
        #expect(!StatementImportModel.takesAction(imported))
    }

    @Test("A transfer row standing for an entry offers only to replace it or import beside it, sends nothing until changed, and can be left out with its box")
    func replaceChoices() async throws {
        let (store, card, bank, _) = try await Self.vault()
        let model = try await Self.cardModel(store, wallet: card)
        let all = try await Self.transactions(store)
        // An income in the bank stands for the top-up the card's row is.
        var income = try #require(all.first { $0.note == "forno bianco" })
        income.kind = .income
        income.walletId = bank

        var topup = try Self.row(model, line: 4)
        #expect(topup.kind == .transferWallet)
        topup.status = .possibleDuplicate
        topup.existing = income
        topup.matchChoice = .replaceExisting
        #expect(StatementImportModel.isReplace(topup))
        #expect(StatementImportModel.replaces(topup))
        #expect(StatementImportModel.isFlagged(topup))
        // In another wallet, yet replaced, never moved; nor kept in the
        // transfer's place: the box leaves it out instead.
        #expect(!model.canMove(topup))
        #expect(model.availableChoices(for: topup) == [.replaceExisting, .importAnyway])
        #expect(StatementImportModel.takesTick(topup))
        #expect(model.defaultChoice(for: topup) == .replaceExisting)
        #expect(model.matchChoice(for: topup) == .replaceExisting)
        #expect(!model.mayReplace(topup))
        #expect(!model.importsByDefault(topup))

        model.setMatchChoice(.replaceExisting, for: topup)
        #expect(model.matchChoices.isEmpty)
        #expect(!model.overrides.contains { $0.line == 4 })

        model.setMatchChoice(.importAnyway, for: topup)
        #expect(model.previewInput?.rowChoices == [
            StatementRowOverride(line: 4, category: nil, note: nil, skip: false, onMatch: .importAnyway),
        ])
        model.setMatchChoice(.replaceExisting, for: topup)

        // An entry not very likely its other side: imported beside it by
        // default, which the review says may be wrong.
        var beside = topup
        beside.status = .new
        beside.matchChoice = .importAnyway
        #expect(StatementImportModel.isReplace(beside))
        #expect(!StatementImportModel.replaces(beside))
        #expect(StatementImportModel.isFlagged(beside))
        #expect(model.defaultChoice(for: beside) == .importAnyway)
        #expect(model.availableChoices(for: beside) == [.replaceExisting, .importAnyway])
        #expect(model.mayReplace(beside))
        #expect(!model.importsByDefault(beside))
        // A plain duplicate of the same entry is no replacement.
        var spend = try Self.row(model, line: 2)
        spend.status = .possibleDuplicate
        spend.existing = income
        #expect(!StatementImportModel.isReplace(spend))
        #expect(!model.availableChoices(for: spend).contains(.replaceExisting))
    }

    // MARK: - Words

    @Test("Every choice over a match, every new status and every kind of a row has words of its own")
    func words() {
        #expect(!StatementText.importedByDefault.isEmpty)
        let choices = StatementMatchChoice.allCases.map(StatementText.matchChoice)
        #expect(Set(choices).count == StatementMatchChoice.allCases.count)
        let details = StatementMatchChoice.allCases.map(StatementText.matchChoiceDetail)
        #expect(Set(details).count == StatementMatchChoice.allCases.count)

        let statuses = [StatementRowStatus.new, .alreadyImported, .possibleDuplicate, .matchedTransfer]
            .map(StatementText.status)
        #expect(Set(statuses).count == statuses.count)

        let kinds = StatementActionChoice.rowChoices.map { StatementText.rowKind($0.action(keeping: .skip), resolved: nil) }
        #expect(Set(kinds).count == StatementActionChoice.rowChoices.count)
        #expect(StatementText.rowKind(.bySign, resolved: .income) == StatementText.kind(.income))
        #expect(StatementText.rowKind(.bySign, resolved: .expense) == StatementText.rowKind(.expense, resolved: nil))
    }
}
