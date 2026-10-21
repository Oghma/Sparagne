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
    /// Vault `Main` with the wallets `Card` and `Bank` and the envelope
    /// `Casa`. A past `forno bianco` filed under Bakery, in Card, gives the
    /// history something to suggest.
    private static func vault() async throws -> (store: AppStore, card: Uuid, bank: Uuid, casa: Uuid) {
        let store = try await StatementFixture.vault(
            typed: ["-3.20 forno bianco #Bakery"],
            wallets: ["Bank": 100_000],
            envelopes: ["Casa"]
        )
        return (
            store,
            try StatementFixture.wallet("Card", in: store),
            try StatementFixture.wallet("Bank", in: store),
            try StatementFixture.envelope("Casa", in: store)
        )
    }

    private static func sorted(_ overrides: [StatementRowOverride]) -> [StatementRowOverride] {
        overrides.sorted { $0.line < $1.line }
    }

    // MARK: - Overrides

    @Test("Each edit of a row goes to the import as one override carrying only what changed")
    func overridesFromEdits() async throws {
        let (store, card, bank, casa) = try await Self.vault()
        let model = try await StatementFixture.cardModel(store, wallet: card)
        let forno = try StatementFixture.row(model, line: 2)
        let metro = try StatementFixture.row(model, line: 3)
        let topup = try StatementFixture.row(model, line: 4)
        let grocer = try StatementFixture.row(model, line: 5)
        #expect(metro.note == "METRO TICKET")

        model.setAction(.income, for: metro)
        model.setNote("Metro", for: 3)
        model.setAction(.transferIn(fromWalletId: bank), for: topup)
        model.setFlow(casa, for: grocer)
        model.setCategory("Spesa", for: 5)
        // A row that matched nothing takes no choice over a match.
        model.setMatchChoice(.importAnyway, for: forno)
        #expect(model.matchChoices.isEmpty)

        #expect(Self.sorted(model.overrides) == [
            // The history's suggestion travels as the row's category.
            StatementRowOverride(line: 2, category: "Bakery", note: nil, skip: false),
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
        let model = try await StatementFixture.cardModel(store, wallet: card)
        let grocer = try StatementFixture.row(model, line: 5)

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
        let model = StatementImportModel(store: store, mappings: try StatementFixture.mappings())
        await model.load(data: Data(StatementFixture.bankWithSalary.utf8), fileName: "banca.csv")
        model.mapping.dateColumn = "Data"
        model.mapping.amountColumn = "Importo"
        model.mapping.descriptionColumns = ["Descrizione"]
        model.walletId = card
        await model.refreshPreview()
        let pos = try StatementFixture.row(model, line: 2)
        let salary = try StatementFixture.row(model, line: 3)
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

        // Planned again with the user's kind, the row says what it was
        // before it (`baseAction`): put back to the kind By sign gives, the
        // edit goes.
        model.setAction(.income, for: pos)
        await model.refreshPreview()
        let edited = try StatementFixture.row(model, line: 2)
        #expect(edited.action == .income)
        #expect(edited.baseAction == .bySign)
        model.setAction(.expense, for: edited)
        #expect(model.editedActions == [3: .refund])
    }

    // MARK: - What re-runs the preview

    @Test("The preview runs again on a kind or an envelope, not on a tick, a note or a category")
    func previewInputChanges() async throws {
        let (store, card, bank, casa) = try await Self.vault()
        let model = try await StatementFixture.cardModel(store, wallet: card)
        let metro = try StatementFixture.row(model, line: 3)
        let grocer = try StatementFixture.row(model, line: 5)

        let start = try #require(model.previewInput)
        model.setIncluded(false, line: 3)
        model.setNote("Metro", for: 3)
        model.setCategory("Transport", for: 3)
        #expect(model.previewInput == start)

        model.setAction(.income, for: metro)
        let kind = try #require(model.previewInput)
        #expect(kind != start)
        #expect(kind.rowChoices == [StatementRowOverride(line: 3, category: nil, note: nil, skip: false, action: .income)])

        model.setFlow(casa, for: grocer)
        #expect(model.previewInput != kind)

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
        let defaults = try TestStore.defaults()
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
        let mappings = try StatementFixture.mappings()
        let vaultId = try #require(store.currentVault?.id)

        let model = try await StatementFixture.cardModel(store, wallet: card, mappings: mappings)
        model.flowId = casa
        model.setAlsoChecked(true, wallet: bank)
        model.since = "2026-09-01"
        await model.refreshPreview()
        await model.runImport()
        #expect(model.problem == nil)
        // The wallets also searched are not part of it.
        #expect(mappings.target(vaultId: vaultId, headers: StatementFixture.cardHeaders) == StatementImportTarget(
            walletId: card, flowId: casa, since: "2026-09-01"
        ))

        // Three wallets: without the memory the target would be the user's to choose.
        let again = StatementImportModel(store: store, mappings: mappings)
        await again.load(data: Data(StatementFixture.card.utf8), fileName: "card.csv")
        #expect(again.walletId == card)
        #expect(again.flowId == casa)
        #expect(again.since == "2026-09-01")
        #expect(!again.isAlsoChecked(bank))
        #expect(again.previewInput?.options.alsoCheckWallets == [])

        // Another file in the same sheet starts with none ticked either.
        again.setAlsoChecked(true, wallet: bank)
        await again.load(data: Data(StatementFixture.card.utf8), fileName: "card.csv")
        #expect(!again.isAlsoChecked(bank))

        // A wallet archived and an envelope gone since are left out.
        await store.archiveWallet(old)
        #expect(!store.wallets.contains { $0.id == old })
        mappings.remember(
            StatementImportTarget(walletId: old, flowId: UUID().uuidString.lowercased()),
            vaultId: vaultId,
            headers: StatementFixture.cardHeaders
        )
        let third = StatementImportModel(store: store, mappings: mappings)
        await third.load(data: Data(StatementFixture.card.utf8), fileName: "card.csv")
        #expect(third.walletId == nil)
        #expect(third.flowId == nil)
        #expect(third.since == nil)
    }

    // MARK: - Transfers that would point at the target

    @Test("A transfer naming the new target forgets its wallet, in a description rule and in a row's own kind")
    func releaseRuleAndRowTransfers() async throws {
        let (store, card, bank, _) = try await Self.vault()
        let model = try await StatementFixture.cardModel(store, wallet: card)
        let metro = try StatementFixture.row(model, line: 3)
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
        let mappings = try StatementFixture.mappings()
        let vaultId = try #require(store.currentVault?.id)
        let first = try await StatementFixture.cardModel(store, wallet: card, mappings: mappings)
        var mapping = first.mapping
        mapping.descriptionRules = [
            StatementDescriptionRule(contains: "A", action: .transferIn(fromWalletId: UUID().uuidString.lowercased())),
            StatementDescriptionRule(contains: "B", action: .transferOut(toWalletId: bank)),
            StatementDescriptionRule(contains: "C", action: .transferOut(toWalletId: card)),
            StatementDescriptionRule(contains: "D", category: "Bar"),
        ]
        mappings.remember(mapping, vaultId: vaultId, headers: StatementFixture.cardHeaders)
        mappings.remember(StatementImportTarget(walletId: card, flowId: nil), vaultId: vaultId, headers: StatementFixture.cardHeaders)

        let model = StatementImportModel(store: store, mappings: mappings)
        await model.load(data: Data(StatementFixture.card.utf8), fileName: "card.csv")

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
        let model = try await StatementFixture.cardModel(store, wallet: card)
        let forno = try StatementFixture.row(model, line: 2)
        let metro = try StatementFixture.row(model, line: 3)
        let topup = try StatementFixture.row(model, line: 4)

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
        let model = try await StatementFixture.cardModel(store, wallet: card)
        let all = try await StatementFixture.transactions(store)
        let existing = try #require(all.first { $0.note == "forno bianco" })
        #expect(existing.walletId == card)
        #expect(!StatementImportModel.isFlagged(try StatementFixture.row(model, line: 3)))

        var duplicate = try StatementFixture.row(model, line: 2)
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
        // Kept where it is, it would go on standing for another wallet's
        // spend: only moved here or imported beside.
        #expect(model.availableChoices(for: duplicate) == [.moveExisting, .importAnyway])
        #expect(model.matchChoice(for: duplicate) == .moveExisting)
        model.setMatchChoice(.skip, for: duplicate)
        #expect(model.matchChoices.isEmpty)

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

    @Test("A choice over a match goes with a change of the row's kind or of anything that plans every row again, and a preview forgets one about no match")
    func staleMatchChoices() async throws {
        let (store, card, bank, _) = try await Self.vault()
        let model = try await StatementFixture.cardModel(store, wallet: card)
        let all = try await StatementFixture.transactions(store)
        let existing = try #require(all.first { $0.note == "forno bianco" })
        var duplicate = try StatementFixture.row(model, line: 2)
        duplicate.status = .possibleDuplicate
        duplicate.existing = existing
        duplicate.matchChoice = .skip

        func choose() {
            model.setMatchChoice(.importAnyway, for: duplicate)
            #expect(model.matchChoices == [2: .importAnyway])
        }

        // The row made a transfer matches anew: the choice is about nothing.
        choose()
        model.setAction(.transferOut(toWalletId: bank), for: duplicate)
        #expect(model.matchChoices.isEmpty)
        model.setAction(.expense, for: duplicate)

        // A kind set to what it already is changes nothing.
        choose()
        model.setAction(.expense, for: duplicate)
        #expect(model.matchChoices == [2: .importAnyway])

        model.mapping.skipStatuses = []
        #expect(model.matchChoices.isEmpty)
        choose()
        model.since = "2026-09-12"
        #expect(model.matchChoices.isEmpty)
        choose()
        model.setAlsoChecked(true, wallet: bank)
        #expect(model.matchChoices.isEmpty)
        choose()
        model.walletId = bank
        #expect(model.matchChoices.isEmpty)
        model.walletId = card
        choose()
        model.makeRule(from: duplicate)
        #expect(model.matchChoices.isEmpty)

        // The preview finds line 2 matching nothing: the choice goes.
        model.removeDescriptionRule(at: 0)
        choose()
        await model.refreshPreview()
        #expect(model.matchChoices.isEmpty)
    }

    @Test("Another file whose header has nothing remembered starts with no wallet, envelope or start day of the last one")
    func anotherFileStartsOver() async throws {
        let (store, card, _, casa) = try await Self.vault()
        let model = try await StatementFixture.cardModel(store, wallet: card)
        model.flowId = casa
        model.since = "2026-09-01"

        await model.load(data: Data(StatementFixture.bankWithSalary.utf8), fileName: "banca.csv")

        // Two wallets: the user chooses again.
        #expect(model.walletId == nil)
        #expect(model.flowId == nil)
        #expect(model.since == nil)
    }

    @Test("Every matched row takes the box, and one ticked off counts as skipped and goes as a skip alone")
    func matchedRowsTicked() async throws {
        let (store, card, bank, _) = try await Self.vault()
        let model = try await StatementFixture.cardModel(store, wallet: card)
        let all = try await StatementFixture.transactions(store)
        let existing = try #require(all.first { $0.note == "forno bianco" })
        var duplicate = try StatementFixture.row(model, line: 2)
        duplicate.status = .possibleDuplicate
        duplicate.existing = existing
        duplicate.matchChoice = .skip
        #expect(StatementImportModel.takesTick(duplicate))
        var transfer = duplicate
        transfer.status = .matchedTransfer
        transfer.existing?.kind = .transferWallet
        transfer.existing?.walletId = nil
        transfer.existing?.fromId = bank
        transfer.existing?.toId = card
        #expect(StatementImportModel.takesTick(transfer))
        // A row with no match and not new takes none.
        #expect(!StatementImportModel.takesTick(try StatementFixture.row(model, line: 6)))
    }

    @Test("Only a row the rules decided offers another kind")
    func whichRowsTakeAKind() async throws {
        let (store, card, _, _) = try await Self.vault()
        let model = try await StatementFixture.cardModel(store, wallet: card)
        #expect(StatementImportModel.takesAction(try StatementFixture.row(model, line: 2)))
        // Waiting for its wallet: a kind of its own can bring it in.
        #expect(StatementImportModel.takesAction(try StatementFixture.row(model, line: 4)))
        // Skipped for its status, which no kind overrides.
        #expect(!StatementImportModel.takesAction(try StatementFixture.row(model, line: 6)))
        var imported = try StatementFixture.row(model, line: 2)
        imported.status = .alreadyImported
        #expect(!StatementImportModel.takesAction(imported))
        imported.status = .skipped(code: "before_start", reason: "")
        #expect(!StatementImportModel.takesAction(imported))
    }

    @Test("A transfer row standing for an entry offers only to replace it or import beside it, sends nothing until changed, and can be left out with its box")
    func replaceChoices() async throws {
        let (store, card, bank, _) = try await Self.vault()
        let model = try await StatementFixture.cardModel(store, wallet: card)
        let all = try await StatementFixture.transactions(store)
        // An income in the bank stands for the top-up the card's row is.
        var income = try #require(all.first { $0.note == "forno bianco" })
        income.kind = .income
        income.walletId = bank

        var topup = try StatementFixture.row(model, line: 4)
        #expect(topup.kind == .transferWallet)
        topup.status = .possibleDuplicate
        topup.existing = income
        topup.matchChoice = .replaceExisting
        topup.defaultChoice = .replaceExisting
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
        #expect(model.matchChoices == [4: .importAnyway])
        #expect(model.matchChoice(for: topup) == .importAnyway)
        // The previewed line 4 matched nothing: a choice over a match it
        // does not have is not sent.
        #expect(model.previewInput?.rowChoices == [])
        #expect(!model.overrides.contains { $0.onMatch != nil })
        model.setMatchChoice(.replaceExisting, for: topup)

        // An entry not very likely its other side: imported beside it by
        // default, which the review says may be wrong.
        var beside = topup
        beside.status = .new
        beside.matchChoice = .importAnyway
        beside.defaultChoice = .importAnyway
        #expect(StatementImportModel.isReplace(beside))
        #expect(!StatementImportModel.replaces(beside))
        #expect(StatementImportModel.isFlagged(beside))
        #expect(model.defaultChoice(for: beside) == .importAnyway)
        #expect(model.availableChoices(for: beside) == [.replaceExisting, .importAnyway])
        #expect(model.mayReplace(beside))
        #expect(!model.importsByDefault(beside))
        // A plain duplicate of the same entry is no replacement.
        var spend = try StatementFixture.row(model, line: 2)
        spend.status = .possibleDuplicate
        spend.existing = income
        #expect(!StatementImportModel.isReplace(spend))
        #expect(!model.availableChoices(for: spend).contains(.replaceExisting))
    }

    // MARK: - Direction and routes

    /// A new row as the core plans it, every field but those named empty:
    /// half of a move between two accounts when it has a `pairedLine`.
    private static func planned(
        line: UInt32,
        kind: TransactionKind? = .expense,
        action: StatementAction = .bySign,
        outflow: Bool = true,
        walletId: Uuid? = nil,
        account: String? = nil,
        pairedLine: UInt32? = nil
    ) -> StatementRow {
        StatementRow(
            line: line, commandId: "command-\(line)", status: .new, kind: kind, occurredAt: nil,
            amount: 1_000, rounded: false, payee: "", bankCategory: nil, matchedCategory: nil,
            counterWalletId: nil, original: nil, action: action, outflow: outflow, note: "",
            category: nil, descriptionRule: nil, existing: nil, matchChoice: nil,
            walletId: walletId, account: account, transferLeg: pairedLine != nil, pairedLine: pairedLine
        )
    }

    @Test("An expense is signed minus and an income or a refund plus; a transfer goes by what the row is taken for, else by the file's sign")
    func signedAmounts() {
        #expect(StatementImportModel.signedAmount(Self.planned(line: 2, kind: .expense, outflow: false)) == -1_000)
        #expect(StatementImportModel.signedAmount(Self.planned(line: 2, kind: .income)) == 1_000)
        #expect(StatementImportModel.signedAmount(Self.planned(line: 2, kind: .refund)) == 1_000)
        func transfer(_ action: StatementAction, outflow: Bool) -> Int64 {
            StatementImportModel.signedAmount(Self.planned(line: 2, kind: .transferWallet, action: action, outflow: outflow))
        }
        #expect(transfer(.transferOut(toWalletId: nil), outflow: false) == -1_000)
        #expect(transfer(.expense, outflow: false) == -1_000)
        #expect(transfer(.transferIn(fromWalletId: nil), outflow: true) == 1_000)
        #expect(transfer(.income, outflow: true) == 1_000)
        #expect(transfer(.refund, outflow: true) == 1_000)
        #expect(transfer(.bySign, outflow: true) == -1_000)
        #expect(transfer(.skip, outflow: false) == 1_000)
    }

    @Test("A move between two accounts reads from the money-out half's wallet to the other's on both halves, or between its accounts within one wallet")
    func pairedRoutes() async throws {
        let (store, card, bank, _) = try await Self.vault()
        let names = NameBook(snapshot: store.snapshot)
        let rows = [
            Self.planned(line: 2, kind: .transferWallet, outflow: false, walletId: card, account: "Revolut", pairedLine: 3),
            Self.planned(line: 3, kind: .income, outflow: true, walletId: bank, account: "BBVA", pairedLine: 2),
            Self.planned(line: 4, kind: .transferWallet, outflow: true, walletId: bank, account: "BBVA", pairedLine: 5),
            Self.planned(line: 5, kind: .income, outflow: false, walletId: bank, account: "Conto", pairedLine: 4),
            // Its other half is not in the file, and a row of its own is no move.
            Self.planned(line: 6, kind: .transferWallet, walletId: bank, pairedLine: 9),
            Self.planned(line: 7, walletId: card),
        ]

        let routes = StatementImportModel.pairedRoutes(rows, names: names)

        // Line 3 says it took money out, so line 2 is the other end.
        #expect(routes[2] == String(localized: "from \("Bank") to \("Card")"))
        #expect(routes[3] == routes[2])
        #expect(routes[4] == String(localized: "from \("BBVA") to \("Conto")"))
        #expect(routes[5] == routes[4])
        #expect(routes[6] == nil)
        #expect(routes[7] == nil)
        #expect(!StatementImportModel.isPairedHalf(rows[5]))
    }

    @Test("The kind menu ticks a By sign row as the income or expense it resolved to, nothing for one its match made a transfer, and the user's kind once there is one")
    func kindChoices() async throws {
        let (store, _, _, _) = try await Self.vault()
        let model = StatementImportModel(store: store, mappings: try StatementFixture.mappings())

        #expect(model.kindChoice(for: Self.planned(line: 2, kind: .expense, outflow: true)) == .expense)
        #expect(model.kindChoice(for: Self.planned(line: 2, kind: .income, outflow: false)) == .income)
        #expect(model.kindChoice(for: Self.planned(line: 2, kind: .transferWallet)) == .bySign)
        #expect(model.kindChoice(for: Self.planned(line: 2, action: .refund)) == .refund)

        let row = Self.planned(line: 3, kind: .expense, outflow: true)
        model.setAction(.transferOut(toWalletId: nil), for: row)
        #expect(model.kindChoice(for: row) == .transferOut)
    }

    @Test("The import has something to write with a new or replacing row, a move or a match to record, and nothing with only skips")
    func countsThatWrite() {
        #expect(!StatementImportModel.Counts(alreadyImported: 2, skipped: 3, invalid: 1, rounded: 1).writesAnything)
        #expect(StatementImportModel.Counts(new: 1).writesAnything)
        #expect(StatementImportModel.Counts(replaces: 1).writesAnything)
        #expect(StatementImportModel.Counts(moves: 1).writesAnything)
        #expect(StatementImportModel.Counts(possibleDuplicates: 1).writesAnything)
        #expect(StatementImportModel.Counts(matchedTransfers: 1).writesAnything)
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
