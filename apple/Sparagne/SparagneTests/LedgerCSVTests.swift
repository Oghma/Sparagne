import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// `LedgerCSV`'s rendering, a pure function over `[TransactionRow]`
/// (`docs/v2/UI.md` §6, ⌘E). Rows are built straight from `TransactionView`
/// through `TransactionRow.init(view:names:)`, with an empty `NameBook` since
/// none of these cases depend on a name resolving.
struct LedgerCSVTests {
    private static func row(
        kind: TransactionKind = .expense,
        amount: Int64 = 12_345,
        category: String = "Groceries",
        note: String = "",
        person: String = "Matteo",
        voided: Bool = false,
        occurredAt: Date = Date()
    ) -> TransactionRow {
        let isTransfer = kind == .transferWallet || kind == .transferFlow
        let view = TransactionView(
            id: UUID().uuidString,
            kind: kind,
            occurredAt: CoreDate.offset(occurredAt),
            amount: amount,
            categoryId: UUID().uuidString,
            category: category,
            categoryIsSystem: false,
            note: note.isEmpty ? nil : note,
            createdBy: person,
            voided: voided,
            walletId: isTransfer ? nil : UUID().uuidString,
            flowId: isTransfer ? nil : UUID().uuidString,
            fromId: isTransfer ? UUID().uuidString : nil,
            toId: isTransfer ? UUID().uuidString : nil,
            legs: []
        )
        return TransactionRow(view: view, names: NameBook(snapshot: nil))
    }

    /// The one data line of a single-row export, as its raw comma-split
    /// fields (none of the fixtures here embed a comma inside a field, so a
    /// naive split is enough to index into it).
    private static func dataLine(_ row: TransactionRow) -> [String] {
        let lines = LedgerCSV.render([row]).components(separatedBy: "\r\n")
        return lines[1].components(separatedBy: ",")
    }

    @Test("An empty list still exports the header, CRLF-terminated, and nothing else")
    func emptyListYieldsOnlyHeader() {
        #expect(LedgerCSV.render([]) == LedgerCSV.header + "\r\n")
    }

    @Test("A field carrying a comma, a quote or a newline is quoted, with inner quotes doubled")
    func quoting() {
        let row = Self.row(note: "bread, milk and \"treats\"\nsecond line")
        let text = LedgerCSV.render([row])
        #expect(text.contains("\"bread, milk and \"\"treats\"\"\nsecond line\""))
    }

    @Test("A plain field with none of those characters is left bare")
    func noQuotingWhenNotNeeded() {
        #expect(Self.dataLine(Self.row(category: "Groceries"))[3] == "Groceries")
    }

    @Test("Amount is signed by kind: expenses negative, income, refunds and transfers positive")
    func signByKind() {
        #expect(Self.dataLine(Self.row(kind: .expense, amount: 12_345))[6] == "-123.45")
        #expect(Self.dataLine(Self.row(kind: .income, amount: 12_345))[6] == "123.45")
        #expect(Self.dataLine(Self.row(kind: .refund, amount: 12_345))[6] == "123.45")
        #expect(Self.dataLine(Self.row(kind: .transferWallet, amount: 12_345))[6] == "123.45")
        #expect(Self.dataLine(Self.row(kind: .transferFlow, amount: 12_345))[6] == "123.45")
    }

    @Test("Kind is written snake_case")
    func kindText() {
        #expect(Self.dataLine(Self.row(kind: .expense))[1] == "expense")
        #expect(Self.dataLine(Self.row(kind: .income))[1] == "income")
        #expect(Self.dataLine(Self.row(kind: .refund))[1] == "refund")
        #expect(Self.dataLine(Self.row(kind: .transferWallet))[1] == "transfer_wallet")
        #expect(Self.dataLine(Self.row(kind: .transferFlow))[1] == "transfer_flow")
    }

    @Test("Voided is written as the bare words true/false")
    func voidedText() {
        #expect(Self.dataLine(Self.row(voided: true))[7] == "true")
        #expect(Self.dataLine(Self.row(voided: false))[7] == "false")
    }

    @Test("The date is an ISO day in the local calendar")
    func dateText() throws {
        let date = try #require(Calendar.current.date(from: DateComponents(year: 2026, month: 8, day: 1)))
        #expect(Self.dataLine(Self.row(occurredAt: date))[0] == "2026-08-01")
    }

    @Test("The default file name lowercases the vault name, dashes its spaces, and uses the direction's raw value")
    func fileName() {
        let month = MonthKey(year: 2026, month: 8)
        #expect(LedgerCSV.fileName(vault: "Casa Nostra", month: month, direction: .expenses) == "casa-nostra-2026-08-expenses.csv")
        #expect(LedgerCSV.fileName(vault: "Casa Nostra", month: month, direction: .income) == "casa-nostra-2026-08-income.csv")
    }

    // MARK: - The optional WALLET column (`docs/v2/UI.md` §3)

    /// The same fixture, with a wallet the `NameBook` can resolve to a name.
    private static func namedWalletRow(wallet: String) -> TransactionRow {
        let walletId = UUID().uuidString
        let view = TransactionView(
            id: UUID().uuidString,
            kind: .expense,
            occurredAt: CoreDate.offset(Date()),
            amount: 1_000,
            categoryId: UUID().uuidString,
            category: "Groceries",
            categoryIsSystem: false,
            note: nil,
            createdBy: "Matteo",
            voided: false,
            walletId: walletId,
            flowId: nil,
            fromId: nil,
            toId: nil,
            legs: []
        )
        let snapshot = VaultSnapshot(
            id: UUID().uuidString,
            name: "Casa",
            currency: .eur,
            wallets: [WalletView(id: walletId, name: wallet, balance: 0, archived: false)],
            flows: [],
            unallocatedFlowId: UUID().uuidString
        )
        return TransactionRow(view: view, names: NameBook(snapshot: snapshot))
    }

    @Test("The export carries the wallet column only while the grid is showing it")
    func walletColumnFollowsTheGrid() {
        #expect(LedgerCSV.render([]) == LedgerCSV.header + "\r\n")
        #expect(LedgerCSV.render([], wallet: true) == LedgerCSV.headerWithWallet + "\r\n")
        #expect(!LedgerCSV.header.contains("wallet"))
    }

    @Test("With the column on, the wallet name sits between the description and the person")
    func walletFieldPosition() {
        let row = Self.namedWalletRow(wallet: "Conto")
        let hidden = LedgerCSV.render([row]).components(separatedBy: "\r\n")[1].components(separatedBy: ",")
        #expect(hidden.count == 8)
        #expect(hidden[5] == "Matteo")

        let shown = LedgerCSV.render([row], wallet: true).components(separatedBy: "\r\n")[1]
            .components(separatedBy: ",")
        #expect(shown.count == 9)
        #expect(shown[5] == "Conto")
        #expect(shown[6] == "Matteo")
    }
}
