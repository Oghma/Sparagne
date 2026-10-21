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
    /// Vault `Main` with the card wallet `Card`, empty, and `Bank` holding
    /// 1000.00. Nothing typed yet.
    private static func vault() async throws -> (store: AppStore, card: Uuid, bank: Uuid) {
        let store = try await StatementFixture.vault(wallets: ["Bank": 100_000])
        return (store, try StatementFixture.wallet("Card", in: store), try StatementFixture.wallet("Bank", in: store))
    }

    /// A line typed into the quick-add, which must be taken.
    private static func type(_ line: String, into store: AppStore) async {
        await store.submit(quickAdd: line)
        #expect(store.presentedError == nil, "the quick-add refused \(line)")
    }

    // MARK: - A row typed by hand

    @Test("A spend typed by hand makes the statement's row a possible duplicate, kept as the one there, and the next import of the file reads it as imported before")
    func handRowKept() async throws {
        let (store, card, _) = try await Self.vault()
        await Self.type("-14.75 green grocer #Spesa @card 2026-09-11", into: store)
        let mappings = try StatementFixture.mappings()
        let model = try await StatementFixture.cardModel(store, wallet: card, mappings: mappings)

        let grocer = try StatementFixture.row(model, line: 5)
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
        let grocers = try await StatementFixture.transactions(store).filter { $0.amount == 1475 && !$0.voided }
        #expect(grocers.count == 1)
        #expect(grocers.first?.note == "green grocer")

        // The same file again: the row stands for the one typed, for good.
        let again = StatementImportModel(store: store, mappings: mappings)
        await again.load(data: Data(StatementFixture.card.utf8), fileName: "card.csv")
        #expect(again.walletId == card)
        await again.refreshPreview()
        let before = try StatementFixture.row(again, line: 5)
        #expect(before.status == .alreadyImported)
        #expect(before.existing == nil)
        #expect(before.matchChoice == nil)
        #expect(!StatementImportModel.isFlagged(before))
        #expect(again.counts.new == 0)
        #expect(again.counts.possibleDuplicates == 0)
    }

    @Test("A matched row ticked off writes and records nothing, and the next import of the file finds its match again")
    func matchedRowLeftOut() async throws {
        let (store, card, _) = try await Self.vault()
        await Self.type("-14.75 green grocer #Spesa @card 2026-09-11", into: store)
        let mappings = try StatementFixture.mappings()
        let model = try await StatementFixture.cardModel(store, wallet: card, mappings: mappings)
        #expect(try StatementFixture.row(model, line: 5).status == .possibleDuplicate)

        model.setIncluded(false, line: 5)
        #expect(model.counts.possibleDuplicates == 0)
        #expect(model.counts.skipped == 3)
        #expect(model.overrides == [StatementRowOverride(line: 5, category: nil, note: nil, skip: true)])

        await model.runImport()

        let report = try #require(model.report)
        #expect(report.executed == 2)
        #expect(report.possibleDuplicates == 0)
        #expect(report.skipped == 3)
        #expect(report.rejected.isEmpty)

        // Nothing claimed the typed grocer for the row: it matches again.
        let again = StatementImportModel(store: store, mappings: mappings)
        await again.load(data: Data(StatementFixture.card.utf8), fileName: "card.csv")
        await again.refreshPreview()
        let grocer = try StatementFixture.row(again, line: 5)
        #expect(grocer.status == .possibleDuplicate)
        #expect(grocer.existing?.note == "green grocer")
        #expect(grocer.defaultChoice == .skip)
    }

    @Test("What becomes of a match by default is the core's: a choice equal to it sends nothing, also once ticking a wallet changed it")
    func defaultsFromTheCore() async throws {
        let (store, card, bank) = try await Self.vault()
        // The grocer typed twice: into the card a day off, into the bank on its day.
        await Self.type("-14.75 verdura @card 2026-09-12", into: store)
        await Self.type("-14.75 green grocer #Spesa @bank 2026-09-11", into: store)
        let model = try await StatementFixture.cardModel(store, wallet: card)

        let kept = try StatementFixture.row(model, line: 5)
        #expect(kept.existing?.note == "verdura")
        #expect(kept.defaultChoice == .skip)
        #expect(model.defaultChoice(for: kept) == .skip)
        model.setMatchChoice(.skip, for: kept)
        #expect(model.matchChoices.isEmpty)
        model.setMatchChoice(.importAnyway, for: kept)
        #expect(model.overrides.first { $0.line == 5 }?.onMatch == .importAnyway)

        // The bank ticked: the bank's grocer, on the row's own day, is the
        // closer match, which by default moves here.
        model.setAlsoChecked(true, wallet: bank)
        #expect(model.matchChoices.isEmpty)
        await model.refreshPreview()
        let moved = try StatementFixture.row(model, line: 5)
        #expect(moved.existing?.note == "green grocer")
        #expect(moved.defaultChoice == .moveExisting)
        #expect(model.matchChoice(for: moved) == .moveExisting)
        #expect(!model.overrides.contains { $0.onMatch != nil })
        model.setMatchChoice(.moveExisting, for: moved)
        #expect(model.matchChoices.isEmpty)
        model.setMatchChoice(.importAnyway, for: moved)
        #expect(model.overrides.first { $0.line == 5 }?.onMatch == .importAnyway)
    }

    @Test("The preview names a wallet whose entries look like the file's new rows until it is ticked")
    func lookalikeAdvisory() async throws {
        let (store, card, bank) = try await Self.vault()
        await Self.type("-14.75 green grocer #Spesa @bank 2026-09-11", into: store)
        let model = try await StatementFixture.cardModel(store, wallet: card)

        #expect(model.preview?.lookalikes == [StatementLookalike(walletId: bank, rows: 1)])
        #expect(model.lookalikes.map(\.wallet.id) == [bank])
        #expect(model.lookalikes.map(\.rows) == [1])

        // Ticked, the advice goes at once, and the core gives none either.
        model.setAlsoChecked(true, wallet: bank)
        #expect(model.lookalikes.isEmpty)
        await model.refreshPreview()
        #expect(model.preview?.lookalikes.isEmpty == true)
        #expect(try StatementFixture.row(model, line: 5).status == .possibleDuplicate)
    }

    @Test("The advice never names a wallet a statement goes to, as a remembered import says")
    func lookalikeNeverAStatementWallet() async throws {
        let (store, card, bank) = try await Self.vault()
        await Self.type("-14.75 green grocer #Spesa @bank 2026-09-11", into: store)
        let mappings = try StatementFixture.mappings()
        let vaultId = try #require(store.currentVault?.id)
        // The bank's own export went to Bank once.
        mappings.remember(StatementImportTarget(walletId: bank, flowId: nil), vaultId: vaultId, headers: ["Data", "Importo"])
        let model = try await StatementFixture.cardModel(store, wallet: card, mappings: mappings)

        // Told Bank is a statement's, the core does not advise it; the app
        // would not show it either.
        #expect(model.statementWallets == [bank, card])
        #expect(model.previewInput?.options.statementWallets == [bank])
        #expect(model.preview?.lookalikes.isEmpty == true)
        #expect(model.lookalikes.isEmpty)
    }

    @Test("A choice is about the transaction it was made over: another row's change that gives the line another match takes it away")
    func choiceFollowsItsMatch() async throws {
        let store = try await StatementFixture.vault(wallet: "Bank", opening: 100_000, wallets: ["Cash": 10_000])
        let bank = try StatementFixture.wallet("Bank", in: store)
        let cash = try StatementFixture.wallet("Cash", in: store)
        await Self.type("-3.50 caffe bar @bank 2026-10-01", into: store)
        await Self.type("-3.50 caffe cash @cash 2026-10-04", into: store)
        // Line 3 skipped by a rule at first: line 2 alone takes the bank's
        // coffee, a day off.
        let model = try await Self.bankModel(
            store,
            "Data;Importo;Descrizione\n02/10/2026;-3,50;CAFFE A\n01/10/2026;-3,50;CAFFE B\n",
            into: bank,
            rules: [StatementDescriptionRule(contains: "caffe b", action: .skip)]
        )
        model.setAlsoChecked(true, wallet: cash)
        await model.refreshPreview()
        let first = try StatementFixture.row(model, line: 2)
        #expect(first.existing?.note == "caffe bar")
        model.setMatchChoice(.importAnyway, for: first)
        await model.refreshPreview()
        #expect(model.matchChoices == [2: .importAnyway])

        // Line 3 an expense after all: it takes the bank's coffee, its own
        // day, and line 2 the cash one, which the choice was not about.
        model.setAction(.expense, for: try StatementFixture.row(model, line: 3))
        await model.refreshPreview()
        #expect(try StatementFixture.row(model, line: 2).existing?.note == "caffe cash")
        // The choice went with the preview that found the other match; that
        // preview, planned with it, is no plan to import from.
        #expect(model.matchChoices.isEmpty)
        #expect(!model.isPlanCurrent)
        await model.refreshPreview()
        let second = try StatementFixture.row(model, line: 2)
        #expect(second.existing?.note == "caffe cash")
        #expect(model.matchChoice(for: second) == .moveExisting)
        #expect(!model.overrides.contains { $0.onMatch != nil })
        #expect(model.isPlanCurrent)

        await model.runImport()

        let all = try await StatementFixture.transactions(store)
        let coffees = all.filter { $0.amount == 350 && !$0.voided }
        #expect(coffees.count == 2)
        #expect(coffees.allSatisfy { $0.walletId == bank })
        #expect(StatementFixture.balance(store, cash) == 10_000)
    }

    @Test("A match dated before the start day is pointed out on the row")
    func matchBeforeStartDay() async throws {
        let (store, card, bank) = try await Self.vault()
        await Self.type("-2.10 metro @bank 2026-09-14", into: store)
        let model = try await StatementFixture.cardModel(store, wallet: card)
        model.setAlsoChecked(true, wallet: bank)
        model.since = "2026-09-16"
        await model.refreshPreview()

        let metro = try StatementFixture.row(model, line: 3)
        #expect(metro.status == .possibleDuplicate)
        #expect(metro.existing?.note == "metro")
        #expect(model.matchPredatesStart(metro))
        #expect(!model.matchPredatesStart(try StatementFixture.row(model, line: 2)))
        #expect(!StatementText.typedBeforeStart.isEmpty)
    }

    @Test("An import pressed before the preview of the last change came back writes nothing and plans again; the next one writes")
    func stalePreviewWritesNothing() async throws {
        let (store, card, bank) = try await Self.vault()
        await Self.type("-14.75 green grocer #Spesa @bank 2026-09-11", into: store)
        let model = try await StatementFixture.cardModel(store, wallet: card)
        #expect(model.isPlanCurrent)

        // Ticked, and imported before the preview runs.
        model.setAlsoChecked(true, wallet: bank)
        #expect(!model.isPlanCurrent)
        await model.runImport()

        #expect(model.report == nil)
        #expect(model.previewWasStale)
        #expect(model.isPlanCurrent)
        #expect(try StatementFixture.row(model, line: 5).status == .possibleDuplicate)
        #expect(try await StatementFixture.transactions(store).filter { $0.amount == 1475 }.count == 1)

        // Any change takes the note away; the next import writes.
        model.setNote("Fruit and veg", for: 2)
        #expect(!model.previewWasStale)
        await model.runImport()
        #expect(model.report?.moved == 1)
        #expect(model.report?.executed == 2)
    }

    @Test("Import anyway brings the possible duplicate back among the new rows, and the import writes it beside the one typed")
    func handRowImportedAnyway() async throws {
        let (store, card, _) = try await Self.vault()
        await Self.type("-14.75 green grocer #Spesa @card 2026-09-11", into: store)
        let model = try await StatementFixture.cardModel(store, wallet: card)
        let flagged = try StatementFixture.row(model, line: 5)

        model.setMatchChoice(.importAnyway, for: flagged)
        await model.refreshPreview()

        let anyway = try StatementFixture.row(model, line: 5)
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
        let grocers = try await StatementFixture.transactions(store).filter { $0.amount == 1475 && $0.kind == .expense && !$0.voided }
        #expect(grocers.count == 2)
        #expect(Set(grocers.compactMap(\.note)) == ["green grocer", "GREEN GROCER"])
    }

    @Test("A spend typed into a wallet ticked under Duplicates also in is moved into the statement's wallet with its note and category")
    func handRowMovedIn() async throws {
        let (store, card, bank) = try await Self.vault()
        await Self.type("-14.75 green grocer #Spesa @bank 2026-09-11", into: store)
        #expect(StatementFixture.balance(store, bank) == 98_525)
        let model = try await StatementFixture.cardModel(store, wallet: card)

        // Not searched yet: the bank's spend is no business of the card's file.
        #expect(try StatementFixture.row(model, line: 5).status == .new)

        model.setAlsoChecked(true, wallet: bank)
        await model.refreshPreview()

        let grocer = try StatementFixture.row(model, line: 5)
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
        let after = try await StatementFixture.transactions(store)
        let moved = try #require(after.first { $0.note == "green grocer" })
        #expect(moved.walletId == card)
        #expect(moved.category == "Spesa")
        #expect(moved.amount == 1475)
        // Out of the bank, into the card's own spending: counted once.
        #expect(StatementFixture.balance(store, bank) == 100_000)
        #expect(StatementFixture.balance(store, card) == -2_005) // 3.20 + 2.10 + 14.75 spent
    }

    // MARK: - The other side of a transfer

    @Test("A transfer typed between the two wallets makes the card's top-up a transfer already there, not imported, and the money moves once")
    func typedTransferMatched() async throws {
        let (store, card, bank) = try await Self.vault()
        await Self.type("tw> 150.00 @bank @card ricarica 2026-09-13", into: store)
        let model = try await StatementFixture.cardModel(store, wallet: card)

        let topup = try StatementFixture.row(model, line: 4)
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
        let transfers = try await StatementFixture.transactions(store).filter { $0.kind == .transferWallet && !$0.voided }
        #expect(transfers.count == 1)
        #expect(StatementFixture.balance(store, bank) == 85_000)
        #expect(StatementFixture.balance(store, card) == 12_995) // 150.00 in, 20.05 spent
    }

    @Test("A description rule makes the bank's top-up of the card a transfer with a clean note, and the card's file then finds it instead of writing it twice")
    func ruleTransferThenMatched() async throws {
        let (store, card, bank) = try await Self.vault()
        let mappings = try StatementFixture.mappings()

        let bankModel = StatementImportModel(store: store, mappings: mappings)
        await bankModel.load(data: Data(StatementFixture.bankWithTopUp.utf8), fileName: "banca.csv")
        bankModel.mapping.dateColumn = "Data"
        bankModel.mapping.amountColumn = "Importo"
        bankModel.mapping.descriptionColumns = ["Descrizione"]
        bankModel.walletId = bank
        bankModel.addDescriptionRule()
        bankModel.mapping.descriptionRules[0].contains = "ricarica carta"
        bankModel.mapping.descriptionRules[0].action = .transferOut(toWalletId: card)
        bankModel.mapping.descriptionRules[0].note = "Ricarica carta"
        await bankModel.refreshPreview()

        let recharge = try StatementFixture.row(bankModel, line: 2)
        #expect(recharge.status == .new)
        #expect(recharge.kind == .transferWallet)
        #expect(recharge.action == .transferOut(toWalletId: card))
        #expect(recharge.counterWalletId == card)
        #expect(recharge.descriptionRule == 0)
        #expect(recharge.note == "Ricarica carta")
        #expect(bankModel.matchCount(ofRule: 0) == 1)
        // The coffee keeps its sign's kind and its own text.
        let coffee = try StatementFixture.row(bankModel, line: 3)
        #expect(coffee.kind == .expense)
        #expect(coffee.descriptionRule == nil)
        #expect(coffee.note == "PAGAMENTO POS BAR CENTRALE")

        await bankModel.runImport()

        #expect(bankModel.report?.executed == 2)
        let written = try await StatementFixture.transactions(store)
        let transfer = try #require(written.first { $0.kind == .transferWallet })
        #expect(transfer.fromId == bank)
        #expect(transfer.toId == card)
        #expect(transfer.note == "Ricarica carta")

        // The card's file: its top-up is that transfer's other side.
        let cardModel = try await StatementFixture.cardModel(store, wallet: card, mappings: mappings)
        #expect(try StatementFixture.row(cardModel, line: 4).status == .matchedTransfer)

        await cardModel.runImport()

        #expect(cardModel.report?.matchedTransfers == 1)
        let transfers = try await StatementFixture.transactions(store).filter { $0.kind == .transferWallet && !$0.voided }
        #expect(transfers.count == 1)
        #expect(StatementFixture.balance(store, bank) == 84_680) // 1000.00 less 150.00 and 3.20
        #expect(StatementFixture.balance(store, card) == 12_995) // 150.00 in, 20.05 spent
    }

    // MARK: - An entry standing for a transfer

    /// `text` read as a continental bank export into `wallet`, with
    /// `rules`, and previewed; `mappings` defaults to a store of its own.
    private static func bankModel(
        _ store: AppStore,
        _ text: String,
        into wallet: Uuid,
        rules: [StatementDescriptionRule] = [],
        mappings: StatementMappingStore? = nil
    ) async throws -> StatementImportModel {
        let model = StatementImportModel(store: store, mappings: try mappings ?? StatementFixture.mappings())
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
    /// is an income there. Then ING's, previewed with `rules`, by default the
    /// one knowing the money went to Revolut; `before` runs once Revolut's
    /// file is in, before ING's is read.
    private static func revolutThenIng(
        rules: [StatementDescriptionRule]? = nil,
        ingRow: String = "01/10/2026;-200,00;BONIFICO A REVOLUT",
        before: (AppStore) async -> Void = { _ in }
    ) async throws -> (
        store: AppStore, revolut: Uuid, ing: Uuid, model: StatementImportModel
    ) {
        let store = try await StatementFixture.vault(wallet: "Revolut", wallets: ["ING": 100_000])
        let revolut = try StatementFixture.wallet("Revolut", in: store)
        let ing = try StatementFixture.wallet("ING", in: store)
        let mappings = try StatementFixture.mappings()

        let first = try await Self.bankModel(
            store, StatementFixture.bankFile("01/10/2026;200,00;BONIFICO DA ING"), into: revolut, mappings: mappings
        )
        #expect(try StatementFixture.row(first, line: 2).kind == .income)
        await first.runImport()
        #expect(first.report?.executed == 1)
        #expect(StatementFixture.balance(store, revolut) == 20_000)
        await before(store)

        let rule = StatementDescriptionRule(contains: "revolut", action: .transferOut(toWalletId: revolut))
        let model = try await Self.bankModel(
            store, StatementFixture.bankFile(ingRow), into: ing, rules: rules ?? [rule], mappings: mappings
        )
        return (store, revolut, ing, model)
    }

    @Test("A bank's transfer whose other side another bank's file imported as an income replaces that income by default, and the money moves once")
    func transferReplacesIncome() async throws {
        let (store, revolut, ing, model) = try await Self.revolutThenIng()
        let row = try StatementFixture.row(model, line: 2)
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
        let all = try await StatementFixture.transactions(store)
        let income = try #require(all.first { $0.kind == .income && $0.amount == 20_000 })
        #expect(income.voided)
        let transfers = all.filter { $0.kind == .transferWallet && !$0.voided }
        #expect(transfers.count == 1)
        #expect(transfers.first?.fromId == ing)
        #expect(transfers.first?.toId == revolut)
        #expect(StatementFixture.balance(store, revolut) == 20_000)
        #expect(StatementFixture.balance(store, ing) == 80_000)
    }

    /// Vault `Main` with `Revolut`, empty, and `ING` holding 1000.00. The
    /// 200.00 sent from ING on Thursday reaches Revolut on Saturday, which
    /// Revolut's export (a header of its own), imported first with no rule,
    /// books as an income. Then ING's export, previewed with the mapping
    /// store Revolut's import was remembered in, or, `remembered` false,
    /// with one that knows nothing of it; its rule knows the money went to
    /// Revolut.
    private static func weekendBonifico(remembered: Bool) async throws -> (
        store: AppStore, revolut: Uuid, ing: Uuid, model: StatementImportModel
    ) {
        let store = try await StatementFixture.vault(wallet: "Revolut", wallets: ["ING": 100_000])
        let revolut = try StatementFixture.wallet("Revolut", in: store)
        let ing = try StatementFixture.wallet("ING", in: store)
        let mappings = try StatementFixture.mappings()

        let first = try await Self.bankModel(
            store, "Data;Importo;Descrizione;Saldo\n03/10/2026;200,00;BONIFICO DA ING;200,00\n",
            into: revolut, mappings: mappings
        )
        #expect(try StatementFixture.row(first, line: 2).kind == .income)
        await first.runImport()
        #expect(first.report?.executed == 1)

        let rule = StatementDescriptionRule(contains: "revolut", action: .transferOut(toWalletId: revolut))
        let model = try await Self.bankModel(
            store, StatementFixture.bankFile("01/10/2026;-200,00;BONIFICO A REVOLUT"), into: ing, rules: [rule],
            mappings: remembered ? mappings : nil
        )
        return (store, revolut, ing, model)
    }

    @Test("The weekend bonifico: once Revolut is a wallet statements go to, its income two days after ING's transfer is replaced by default")
    func weekendBonificoReplaced() async throws {
        let (store, revolut, ing, model) = try await Self.weekendBonifico(remembered: true)
        #expect(model.previewInput?.options.statementWallets == [revolut])

        let row = try StatementFixture.row(model, line: 2)
        #expect(row.existing?.kind == .income)
        #expect(row.existing?.walletId == revolut)
        #expect(row.defaultChoice == .replaceExisting)
        #expect(row.matchChoice == .replaceExisting)
        #expect(StatementImportModel.replaces(row))
        #expect(model.counts.replaces == 1)

        await model.runImport()

        #expect(model.report?.executed == 1)
        #expect(model.replaced == 1)
        let all = try await StatementFixture.transactions(store)
        #expect(all.first { $0.kind == .income && $0.amount == 20_000 }?.voided == true)
        #expect(all.filter { $0.kind == .transferWallet && !$0.voided }.count == 1)
        #expect(StatementFixture.balance(store, ing) == 80_000)
        #expect(StatementFixture.balance(store, revolut) == 20_000)
    }

    @Test("The weekend bonifico with nothing remembered of Revolut: two days apart, the transfer comes in beside the income by default")
    func weekendBonificoUnknown() async throws {
        let (_, revolut, _, model) = try await Self.weekendBonifico(remembered: false)
        #expect(model.previewInput?.options.statementWallets == [])

        let row = try StatementFixture.row(model, line: 2)
        #expect(row.existing?.walletId == revolut)
        #expect(row.defaultChoice == .importAnyway)
        #expect(row.status == .new)
        #expect(model.mayReplace(row))
    }

    @Test("The wallets statements go to are an option of the preview: one remembered meanwhile plans the file again")
    func statementWalletsInTheInput() async throws {
        let (store, card, bank) = try await Self.vault()
        let mappings = try StatementFixture.mappings()
        let model = try await StatementFixture.cardModel(store, wallet: card, mappings: mappings)
        let before = try #require(model.previewInput)
        #expect(before.options.statementWallets == [])

        let vaultId = try #require(store.currentVault?.id)
        mappings.remember(StatementImportTarget(walletId: bank, flowId: nil), vaultId: vaultId, headers: ["Data", "Importo"])
        // A target of the card's own header names the card: not sent.
        mappings.remember(StatementImportTarget(walletId: card, flowId: nil), vaultId: vaultId, headers: ["Other"])

        let after = try #require(model.previewInput)
        #expect(after.options.statementWallets == [bank])
        #expect(after != before)
        #expect(!model.isPlanCurrent)
    }

    @Test("A transfer row standing for an entry, ticked off, writes nothing: no transfer, the entry left as it is")
    func replaceRowLeftOut() async throws {
        let (store, revolut, ing, model) = try await Self.revolutThenIng()
        let row = try StatementFixture.row(model, line: 2)
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
        let all = try await StatementFixture.transactions(store)
        #expect(all.first { $0.kind == .income && $0.amount == 20_000 }?.voided == false)
        #expect(!all.contains { $0.kind == .transferWallet })
        #expect(StatementFixture.balance(store, revolut) == 20_000)
        #expect(StatementFixture.balance(store, ing) == 100_000)
    }

    @Test("A cash withdrawal to Cash and an unrelated cash income typed two days later: the income may be its other side, so the transfer comes in beside it by default and both stay")
    func withdrawalBesideTypedIncome() async throws {
        let store = try await StatementFixture.vault(wallet: "Bank", opening: 100_000, wallets: ["Cash": 0])
        let bank = try StatementFixture.wallet("Bank", in: store)
        let cash = try StatementFixture.wallet("Cash", in: store)
        await Self.type("+50.00 regalo @cash 2026-10-03", into: store)

        let rule = StatementDescriptionRule(contains: "prelievo", action: .transferOut(toWalletId: cash))
        let model = try await Self.bankModel(
            store, StatementFixture.bankFile("01/10/2026;-50,00;PRELIEVO ATM"), into: bank, rules: [rule]
        )

        let row = try StatementFixture.row(model, line: 2)
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
        let all = try await StatementFixture.transactions(store)
        #expect(all.first { $0.note == "regalo" }?.voided == false)
        #expect(all.filter { $0.kind == .transferWallet && !$0.voided }.count == 1)
        #expect(StatementFixture.balance(store, cash) == 10_000) // the gift and the withdrawal
        #expect(StatementFixture.balance(store, bank) == 95_000)
    }

    @Test("A choice made over one match is not carried to the next: a row imported anyway beside a typed spend, then made a transfer to Revolut, replaces Revolut's income by default instead of landing beside it")
    func choiceForgottenWithTheKind() async throws {
        let (store, revolut, ing, model) = try await Self.revolutThenIng(
            rules: [], ingRow: "01/10/2026;-200,00;BONIFICO"
        ) { store in
            await Self.type("-200.00 affitto @ing 2026-09-29", into: store)
        }
        let spend = try StatementFixture.row(model, line: 2)
        #expect(spend.status == .possibleDuplicate)
        #expect(spend.existing?.note == "affitto")
        model.setMatchChoice(.importAnyway, for: spend)
        await model.refreshPreview()
        #expect(try StatementFixture.row(model, line: 2).matchChoice == .importAnyway)

        model.setAction(.transferOut(toWalletId: revolut), for: try StatementFixture.row(model, line: 2))
        #expect(model.matchChoices.isEmpty)
        await model.refreshPreview()

        let transfer = try StatementFixture.row(model, line: 2)
        #expect(transfer.existing?.walletId == revolut)
        #expect(transfer.matchChoice == .replaceExisting)
        #expect(model.matchChoice(for: transfer) == .replaceExisting)
        #expect(model.overrides == [
            StatementRowOverride(line: 2, category: nil, note: nil, skip: false, action: .transferOut(toWalletId: revolut)),
        ])

        await model.runImport()

        let all = try await StatementFixture.transactions(store)
        #expect(all.first { $0.kind == .income && $0.amount == 20_000 }?.voided == true)
        #expect(all.filter { $0.kind == .transferWallet && !$0.voided }.count == 1)
        // Counted once in Revolut; the rent typed in ING stays.
        #expect(StatementFixture.balance(store, revolut) == 20_000)
        #expect(StatementFixture.balance(store, ing) == 60_000)
    }

    @Test("A By sign row matched to a transfer typed between the two wallets takes an expense of the user's")
    func bySignMatchedTransfer() async throws {
        let (store, card, bank) = try await Self.vault()
        await Self.type("tw> 150.00 @bank @card ricarica 2026-09-13", into: store)
        let model = try await Self.bankModel(
            store, StatementFixture.bankFile("13/09/2026;-150,00;RICARICA CARTA"), into: bank
        )
        let row = try StatementFixture.row(model, line: 2)
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
        let mappings = try StatementFixture.mappings()
        let first = try await StatementFixture.cardModel(store, wallet: card, mappings: mappings)
        await first.runImport()
        #expect(first.report?.executed == 3)

        // The next export starts on the last day of the first: its own
        // 3.20 a day later may be the same bread, or the next one.
        let next = StatementFixture.cardFile(
            StatementFixture.cardRow("2026-09-18 09:00:00 UTC", "card_spend", "FORNO B.", "CLEARED", "3.20"), // 2
            StatementFixture.cardRow("2026-09-17 07:00:00 UTC", "card_spend", "EDICOLA", "CLEARED", "1.50") // 3
        )
        let model = StatementImportModel(store: store, mappings: mappings)
        await model.load(data: Data(next.utf8), fileName: "card-2.csv")
        #expect(model.walletId == card)
        await model.refreshPreview()

        let bread = try StatementFixture.row(model, line: 2)
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
        let kept = try StatementFixture.row(model, line: 2)
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
        let model = try await StatementFixture.cardModel(store, wallet: card)
        #expect(try StatementFixture.row(model, line: 5).status == .new)

        model.since = "2026-09-12"
        await model.refreshPreview()

        let grocer = try StatementFixture.row(model, line: 5)
        guard case .skipped(let code, _) = grocer.status else {
            Issue.record("the grocer, a day before the start, should be skipped, not \(grocer.status)")
            return
        }
        #expect(code == "before_start")
        #expect(StatementText.status(grocer.status) == StatementText.skipped("before_start"))
        #expect(!StatementImportModel.takesAction(grocer))
        // The rows from the start day on are untouched.
        #expect(try StatementFixture.row(model, line: 2).status == .new)
        #expect(try StatementFixture.row(model, line: 3).status == .new)
        #expect(model.counts.new == 2)

        await model.runImport()

        #expect(model.report?.executed == 2)
        let imported = try await StatementFixture.transactions(store)
        #expect(!imported.contains { $0.note == "GREEN GROCER" })
        #expect(imported.contains { $0.note == "METRO TICKET" })
    }
}
