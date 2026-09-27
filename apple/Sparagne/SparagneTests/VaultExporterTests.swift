import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Back Up Database and Export All Transactions (`Support/VaultExporter.swift`)
/// over an in-memory core: the backup is opened again as a database, the
/// export is read back line by line.
@MainActor
struct VaultExporterTests {
    /// A store through onboarding: vault `Casa`, wallet `Cash` with 10,000.00.
    private static func onboarded() async throws -> AppStore {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.tests.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Cash", openingBalance: 1_000_000)
        #expect(store.presentedError == nil)
        return store
    }

    /// A fresh folder in the temporary directory, removed by the caller.
    private static func folder() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appending(path: "sparagne-b2-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    /// The comma-split fields of every data line (no fixture here puts a
    /// comma in a field).
    private static func lines(_ csv: String) -> [[String]] {
        csv.components(separatedBy: "\r\n")
            .dropFirst()
            .filter { !$0.isEmpty }
            .map { $0.components(separatedBy: ",") }
    }

    @Test("A backup is a database the core opens with the same vaults, and replaces a file already there")
    func backupOpensWithTheSameVaults() async throws {
        let store = try await Self.onboarded()
        await store.createVault(name: "Viaggi", walletName: "Carta", openingBalance: 0)
        let folder = try Self.folder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let destination = folder.appending(path: VaultExporter.backupFileName(), directoryHint: .notDirectory)
        // The save panel already asked to replace this one.
        try Data("not a database".utf8).write(to: destination)

        try await VaultExporter.backup(core: store.core, to: destination)

        let copy = try CoreHandle.open(path: destination.path(percentEncoded: false))
        let restored = try copy.vaults()
        #expect(restored.map(\.id).sorted() == store.vaults.map(\.id).sorted())
        #expect(Set(restored.map(\.name)) == ["Casa", "Viaggi"])
        let casa = try #require(restored.first { $0.name == "Casa" })
        let wallets = try copy.snapshot(vaultId: casa.id).wallets
        #expect(wallets.first?.balance == 1_000_000)
        // Nothing is left behind in the temporary directory.
        let leftovers = try FileManager.default.contentsOfDirectory(atPath: FileManager.default.temporaryDirectory.path)
        #expect(!leftovers.contains { $0.hasPrefix("Sparagne-backup-") })
    }

    @Test("The full export pages past a thousand rows and carries voided rows and both kinds of transfer")
    func exportAllPagesAndKeepsEverything() async throws {
        let store = try await Self.onboarded()
        await store.createWallet(name: "Bank", openingBalance: 0)
        await store.createEnvelope(name: "Holidays", mode: .unlimited, allowNegative: false, openingAllocation: 0)
        let vault = try #require(store.currentVault?.id)
        let cash = try #require(store.wallets.first { $0.name == "Cash" }).id
        let bank = try #require(store.wallets.first { $0.name == "Bank" }).id
        let snapshot = try #require(store.snapshot)
        let holidays = try #require(snapshot.flows.first { $0.name == "Holidays" }).id

        let start = Date(timeIntervalSince1970: 1_780_000_000)
        let spends: [Command] = (0..<1001).map { index in
            .expense(Entry(
                amount: 100 + Int64(index),
                walletId: cash,
                flowId: nil,
                category: "Food",
                note: "row \(index)",
                occurredAt: CoreDate.offset(start.addingTimeInterval(Double(index) * 60))
            ))
        }
        try await store.core.executeBatch(vaultId: vault, spends)
        let later = CoreDate.offset(start.addingTimeInterval(86_400 * 2))
        try await store.core.execute(
            vaultId: vault,
            .transferWallet(amount: 5000, fromWalletId: cash, toWalletId: bank, note: "to bank", occurredAt: later)
        )
        try await store.core.execute(
            vaultId: vault,
            .transferFlow(amount: 2000, fromFlowId: snapshot.unallocatedFlowId, toFlowId: holidays, note: "saving", occurredAt: later)
        )
        let pizza = try await store.core.execute(
            vaultId: vault,
            .expense(Entry(amount: 1250, walletId: cash, flowId: holidays, category: "Food", note: "pizza", occurredAt: later))
        )
        try await store.core.execute(vaultId: vault, .voidTransaction(transactionId: try #require(pizza.resultId)))

        let all = try await VaultExporter.allTransactions(core: store.core, vaultId: vault)
        #expect(all.count > 1004)
        #expect(Set(all.map(\.id)).count == all.count)
        // Small pages join into the same list, in the same order.
        let paged = try await VaultExporter.allTransactions(core: store.core, vaultId: vault, pageSize: 7)
        #expect(paged.map(\.id) == all.map(\.id))

        let csv = try await VaultExporter.allTransactionsCSV(core: store.core, vaultId: vault)
        #expect(csv.hasPrefix(LedgerCSV.allHeader + "\r\n"))
        let lines = Self.lines(csv)
        #expect(lines.count == all.count)
        #expect(lines.allSatisfy { $0.count == 9 })

        let first = try #require(lines.first { $0[6] == "row 0" })
        #expect(first[1] == "expense")
        #expect(first[2] == "-1.00")
        #expect(first[3] == "Cash")
        #expect(first[4] == NameBook.unallocatedLabel)
        #expect(first[5] == "Food")
        #expect(first[7] == "tester")
        #expect(first[8] == "false")

        let wallets = try #require(lines.first { $0[6] == "to bank" })
        #expect(wallets[1] == "transfer_wallet")
        #expect(wallets[2] == "50.00")
        #expect(wallets[3] == "Cash \u{2192} Bank")
        #expect(wallets[4] == TransactionRow.placeholder)

        let envelopes = try #require(lines.first { $0[6] == "saving" })
        #expect(envelopes[1] == "transfer_flow")
        #expect(envelopes[3] == TransactionRow.placeholder)
        #expect(envelopes[4] == "\(NameBook.unallocatedLabel) \u{2192} Holidays")

        let voided = try #require(lines.first { $0[6] == "pizza" })
        #expect(voided[2] == "-12.50")
        #expect(voided[4] == "Holidays")
        #expect(voided[8] == "true")
    }

    @Test("The backup is named after today")
    func backupFileName() throws {
        let date = try #require(Calendar.current.date(from: DateComponents(year: 2026, month: 9, day: 3)))
        #expect(VaultExporter.backupFileName(date: date) == "Sparagne 2026-09-03.sqlite")
    }
}
