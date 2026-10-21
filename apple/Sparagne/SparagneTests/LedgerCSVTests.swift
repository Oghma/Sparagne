import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// `LedgerCSV`'s rendering, a pure function over `[TransactionRow]` (⌘E).
struct LedgerCSVTests {
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
        let row = TestRow.make(note: "bread, milk and \"treats\"\nsecond line")
        let text = LedgerCSV.render([row])
        #expect(text.contains("\"bread, milk and \"\"treats\"\"\nsecond line\""))
    }

    @Test("A plain field with none of those characters is left bare")
    func noQuotingWhenNotNeeded() {
        #expect(Self.dataLine(TestRow.make(category: "Groceries"))[3] == "Groceries")
    }

    @Test("Amount is signed by kind: expenses negative, income, refunds and transfers positive")
    func signByKind() {
        #expect(Self.dataLine(TestRow.make(kind: .expense, amount: 12_345))[6] == "-123.45")
        #expect(Self.dataLine(TestRow.make(kind: .income, amount: 12_345))[6] == "123.45")
        #expect(Self.dataLine(TestRow.make(kind: .refund, amount: 12_345))[6] == "123.45")
        #expect(Self.dataLine(TestRow.make(kind: .transferWallet, amount: 12_345))[6] == "123.45")
        #expect(Self.dataLine(TestRow.make(kind: .transferFlow, amount: 12_345))[6] == "123.45")
    }

    @Test("Kind is written snake_case")
    func kindText() {
        #expect(Self.dataLine(TestRow.make(kind: .expense))[1] == "expense")
        #expect(Self.dataLine(TestRow.make(kind: .income))[1] == "income")
        #expect(Self.dataLine(TestRow.make(kind: .refund))[1] == "refund")
        #expect(Self.dataLine(TestRow.make(kind: .transferWallet))[1] == "transfer_wallet")
        #expect(Self.dataLine(TestRow.make(kind: .transferFlow))[1] == "transfer_flow")
    }

    @Test("Voided is written as the bare words true/false")
    func voidedText() {
        #expect(Self.dataLine(TestRow.make(voided: true))[7] == "true")
        #expect(Self.dataLine(TestRow.make(voided: false))[7] == "false")
    }

    @Test("The date is an ISO day in the local calendar")
    func dateText() throws {
        let date = try #require(Calendar.current.date(from: DateComponents(year: 2026, month: 8, day: 1)))
        #expect(Self.dataLine(TestRow.make(occurredAt: date))[0] == "2026-08-01")
    }

    @Test("The default file name lowercases the vault name, dashes its spaces, and uses the direction's raw value")
    func fileName() {
        let month = MonthKey(year: 2026, month: 8)
        #expect(LedgerCSV.fileName(vault: "Casa Nostra", month: month, direction: .expenses) == "casa-nostra-2026-08-expenses.csv")
        #expect(LedgerCSV.fileName(vault: "Casa Nostra", month: month, direction: .income) == "casa-nostra-2026-08-income.csv")
    }

    @Test("The export carries the wallet column only while the grid is showing it")
    func walletColumnFollowsTheGrid() {
        #expect(LedgerCSV.render([]) == LedgerCSV.header + "\r\n")
        #expect(LedgerCSV.render([], wallet: true) == LedgerCSV.headerWithWallet + "\r\n")
        #expect(!LedgerCSV.header.contains("wallet"))
    }

    // MARK: - Export All Transactions (`Support/VaultExporter.swift`)

    @Test("The full export has its own header, both name columns, and the ledger's quoting and signs")
    func renderAll() {
        #expect(LedgerCSV.renderAll([]) == LedgerCSV.allHeader + "\r\n")

        let spend = TestRow.make(amount: 1_000, category: "Groceries", person: "Matteo", wallet: "Conto")
        let fields = LedgerCSV.renderAll([spend]).components(separatedBy: "\r\n")[1].components(separatedBy: ",")
        #expect(fields.count == 10)
        #expect(fields[1] == "expense")
        #expect(fields[2] == "-10.00")
        #expect(fields[3] == "Conto")
        #expect(fields[4] == TransactionRow.placeholder)
        #expect(fields[7] == "Matteo")
        #expect(fields[8] == "Matteo")
        #expect(fields[9] == "false")

        let quoted = LedgerCSV.renderAll([TestRow.make(note: "bread, milk")])
        #expect(quoted.contains(",\"bread, milk\","))
    }

    @Test("A row recorded for someone else: the ledger's person is who it is for, the full export says who recorded it too")
    func personAndAuthor() {
        let dinner = TestRow.make(note: "cena", person: "Elisa", recordedBy: "Matteo")
        #expect(Self.dataLine(dinner)[5] == "Elisa")

        #expect(LedgerCSV.allHeader.contains(",note,person,author,voided"))
        let fields = LedgerCSV.renderAll([dinner]).components(separatedBy: "\r\n")[1].components(separatedBy: ",")
        #expect(fields[7] == "Elisa")
        #expect(fields[8] == "Matteo")
    }

    @Test("The full export is named after the vault and today, without the characters a file name cannot hold")
    func allFileName() throws {
        let date = try #require(Calendar.current.date(from: DateComponents(year: 2026, month: 9, day: 3)))
        #expect(LedgerCSV.allFileName(vault: "Casa Nostra", date: date) == "Sparagne Casa Nostra all 2026-09-03.csv")
        #expect(LedgerCSV.allFileName(vault: "a/b:c", date: date) == "Sparagne a-b-c all 2026-09-03.csv")
        #expect(LedgerCSV.allFileName(vault: " ", date: date) == "Sparagne all 2026-09-03.csv")
    }

    @Test("With the column on, the wallet name sits between the description and the person")
    func walletFieldPosition() {
        let row = TestRow.make(amount: 1_000, category: "Groceries", person: "Matteo", wallet: "Conto")
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
