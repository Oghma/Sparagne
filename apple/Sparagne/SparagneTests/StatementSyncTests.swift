import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// The statement import brings the vault up to date before it plans a file
/// and again before it imports, through a sync round handed to the model;
/// here a hook stands in for the engine, so no server is needed. What the
/// hook writes plays a command pulled from another device.
struct StatementSyncTests {
    private static let cardHeader = "timestamp,type,description,status,amount,currency,card,"
        + "card holder name,original amount,original currency,cashback earned,cashback currency,"
        + "category,spending mode"

    private static func cardRow(_ at: String, _ type: String, _ description: String, _ status: String, _ amount: String) -> String {
        "\(at),\(type),\(description),\(status),\(amount),EUR,1234,SAM TESTER,\(amount),EUR,,,,standard"
    }

    /// Three spends; lines count the header.
    private static let card = ([cardHeader] + [
        cardRow("2026-09-17 08:15:33 UTC", "card_spend", "FORNO BIANCO", "CLEARED", "3.20"), // 2
        cardRow("2026-09-16 12:03:10 UTC", "card_spend", "METRO TICKET", "CLEARED", "2.10"), // 3
        cardRow("2026-09-11 18:22:41 UTC", "card_spend", "GREEN GROCER", "CLEARED", "14.75"), // 4
    ]).joined(separator: "\n") + "\n"

    /// What the hook was asked, and what it answers.
    @MainActor
    final class Hook {
        var rounds = 0
        var answer: StatementImportModel.SyncResult = .synced
        /// Runs inside a round, as a pull would.
        var during: [Int: @MainActor () async -> Void] = [:]
        /// Held open until released, to ask for previews while it runs.
        var gate: CheckedContinuation<Void, Never>?
        var holds = false

        func round() async -> StatementImportModel.SyncResult {
            rounds += 1
            if holds { await withCheckedContinuation { gate = $0 } }
            await during[rounds]?()
            return answer
        }

        func release() {
            holds = false
            gate?.resume()
            gate = nil
        }
    }

    private static func vault() async throws -> (store: AppStore, card: Uuid) {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.tests.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "tester"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Main", walletName: "Card", openingBalance: 0)
        #expect(store.presentedError == nil)
        return (store, try #require(store.wallets.first).id)
    }

    private static func model(_ store: AppStore, hook: Hook) throws -> StatementImportModel {
        let mappings = StatementMappingStore(
            defaults: try #require(UserDefaults(suiteName: "sparagne.tests.\(UUID().uuidString)"))
        )
        return StatementImportModel(store: store, mappings: mappings, sync: { await hook.round() })
    }

    private static func row(_ model: StatementImportModel, line: UInt32) throws -> StatementRow {
        try #require(model.preview?.rows.first { $0.line == line })
    }

    @Test("A file's first preview waits for a sync and the next ones do not; the import syncs again, and a plan that changed is shown, marked, before the next import writes it")
    @MainActor
    func syncsBeforePreviewAndImport() async throws {
        let (store, card) = try await Self.vault()
        let hook = Hook()
        // The import's round brings in the grocer, typed on another device.
        hook.during[2] = { await store.submit(quickAdd: "-14.75 green grocer #Spesa 2026-09-11") }
        let model = try Self.model(store, hook: hook)

        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        #expect(model.walletId == card)
        #expect(hook.rounds == 0)
        await model.refreshPreview()
        #expect(hook.rounds == 1)
        #expect(try Self.row(model, line: 4).status == .new)
        #expect(model.counts.new == 3)

        model.mapping.skipStatuses = []
        await model.refreshPreview()
        #expect(hook.rounds == 1)

        await model.runImport()

        #expect(hook.rounds == 2)
        #expect(!model.syncFailed)
        // Planned again after the round: the grocer typed elsewhere now
        // stands for the bank's row. That is not what the user saw, so
        // nothing is written yet and the row is marked.
        #expect(try Self.row(model, line: 4).status == .possibleDuplicate)
        #expect(model.report == nil)
        #expect(model.step == .mapping)
        #expect(model.changedLines == [4])
        #expect(!model.isLocked)
        let vaultId = try #require(store.currentVault?.id)
        let filter = TransactionFilter(includeVoided: false, includeTransfers: true, ascending: true)
        func grocers() async throws -> Int {
            try await store.core.transactions(vaultId: vaultId, filter: filter, limit: 50, cursor: nil)
                .items.filter { $0.amount == 1475 }.count
        }
        #expect(try await grocers() == 1)

        // Any change of the user's takes the marks away.
        model.setNote("Bread", for: 2)
        #expect(model.changedLines.isEmpty)

        // Seen, it goes: the next round changes nothing, and the import writes.
        await model.runImport()

        #expect(hook.rounds == 3)
        #expect(model.changedLines.isEmpty)
        let report = try #require(model.report)
        #expect(report.executed == 2)
        #expect(report.possibleDuplicates == 1)
        #expect(try await grocers() == 1)

        // Another file: its first preview syncs again.
        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        await model.refreshPreview()
        #expect(hook.rounds == 4)
    }

    @Test("The lines planned otherwise after a sync are what the user decides on: status, match, choice, kind, other wallet")
    func changedLines() async throws {
        let (store, _) = try await Self.vault()
        let model = try Self.model(store, hook: Hook())
        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        await model.refreshPreview()
        let seen = try #require(model.preview)
        #expect(StatementImportModel.changedLines(from: seen, to: seen).isEmpty)

        var fresh = seen
        fresh.rows[0].status = .alreadyImported
        fresh.rows[1].kind = .income
        fresh.rows[2].note = "a note says nothing of the plan"
        #expect(StatementImportModel.changedLines(from: seen, to: fresh) == [2, 3])

        fresh = seen
        fresh.rows.removeLast()
        #expect(StatementImportModel.changedLines(from: seen, to: fresh) == [4])
    }

    @Test("A round that does not get through stops neither the preview nor the import, and only leaves a quiet note until one does")
    @MainActor
    func failedSyncDoesNotBlock() async throws {
        let (store, _) = try await Self.vault()
        let hook = Hook()
        hook.answer = .failed
        let model = try Self.model(store, hook: hook)

        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        await model.refreshPreview()
        #expect(model.preview != nil)
        #expect(model.syncFailed)
        #expect(!model.isSyncing)

        await model.runImport()
        #expect(model.report?.executed == 3)
        #expect(model.problem == nil)
        #expect(model.syncFailed)

        hook.answer = .synced
        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        await model.refreshPreview()
        #expect(!model.syncFailed)
    }

    @Test("A preview asked for while the round runs waits for that same round, which the sheet shows as syncing")
    @MainActor
    func previewsShareTheRound() async throws {
        let (store, _) = try await Self.vault()
        let hook = Hook()
        hook.holds = true
        let model = try Self.model(store, hook: hook)
        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")

        let first = Task { await model.refreshPreview() }
        while hook.gate == nil { await Task.yield() }
        #expect(model.isSyncing)
        #expect(model.preview == nil)
        let second = Task { await model.refreshPreview() }
        await Task.yield()
        hook.release()
        await first.value
        await second.value

        #expect(hook.rounds == 1)
        #expect(!model.isSyncing)
        #expect(model.preview != nil)
        #expect(!model.isPreviewing)
    }

    @Test("Without a sync round, as for a database that never syncs, nothing waits and nothing is noted")
    @MainActor
    func noSync() async throws {
        let (store, _) = try await Self.vault()
        let defaults = try #require(UserDefaults(suiteName: "sparagne.tests.\(UUID().uuidString)"))
        let model = StatementImportModel(store: store, mappings: StatementMappingStore(defaults: defaults))
        await model.load(data: Data(Self.card.utf8), fileName: "card.csv")
        await model.refreshPreview()
        await model.runImport()
        #expect(model.report?.executed == 3)
        #expect(!model.syncFailed)
        #expect(!model.isSyncing)
    }
}
