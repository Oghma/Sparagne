import AppKit
import SwiftUI
import SparagneCore
import UniformTypeIdentifiers

/// The two files the File menu writes besides the ledger's CSV: a backup of
/// the whole database and every transaction of the vault on screen. The work
/// is here, free of any view, so the tests run it; `VaultExportHandlers`
/// only asks where to save.
enum VaultExporter {
    /// Transactions per call to the core while exporting: the ledger's own
    /// page size, so a vault of a few years takes a handful of calls.
    static let defaultPageSize: UInt32 = 1000

    /// `Sparagne 2026-09-23.sqlite`, today in the local calendar.
    static func backupFileName(date: Date = Date()) -> String {
        "Sparagne \(CoreDate.day(date)).sqlite"
    }

    /// A consistent copy of the whole database at `destination`, replacing
    /// whatever is there (the save panel has asked).
    ///
    /// The core writes into a fresh file in the temporary directory first:
    /// `VACUUM INTO` refuses an existing path, and SQLite writing straight
    /// into a folder the user picked could leave a journal beside the backup
    /// if it were interrupted. The copy then moves as one file, off the main
    /// actor.
    @concurrent
    static func backup(core: CoreActor, to destination: URL) async throws {
        let manager = FileManager.default
        let staging = manager.temporaryDirectory
            .appending(path: "Sparagne-backup-\(UUID().uuidString).sqlite", directoryHint: .notDirectory)
        defer { try? manager.removeItem(at: staging) }
        try await core.backup(to: staging.path(percentEncoded: false))
        if manager.fileExists(atPath: destination.path(percentEncoded: false)) {
            try manager.removeItem(at: destination)
        }
        try manager.copyItem(at: staging, to: destination)
    }

    /// Every transaction of the vault, oldest first, voided ones and
    /// transfers included, a page at a time until the core has no more.
    static func allTransactions(
        core: CoreActor,
        vaultId: Uuid,
        pageSize: UInt32 = defaultPageSize
    ) async throws -> [TransactionView] {
        let filter = TransactionFilter(
            from: nil,
            to: nil,
            kinds: nil,
            includeVoided: true,
            includeTransfers: true,
            walletId: nil,
            flowId: nil,
            text: nil,
            person: nil,
            ascending: true
        )
        var all: [TransactionView] = []
        var cursor: String?
        repeat {
            let page = try await core.transactions(vaultId: vaultId, filter: filter, limit: pageSize, cursor: cursor)
            all.append(contentsOf: page.items)
            cursor = page.nextCursor
        } while cursor != nil
        return all
    }

    /// The text of Export All Transactions (`LedgerCSV.renderAll`), with the
    /// wallets and envelopes named as the vault names them now, archived
    /// ones included. Rendered off the main actor: a vault of years is
    /// thousands of lines.
    @concurrent
    static func allTransactionsCSV(
        core: CoreActor,
        vaultId: Uuid,
        pageSize: UInt32 = defaultPageSize
    ) async throws -> String {
        let views = try await allTransactions(core: core, vaultId: vaultId, pageSize: pageSize)
        let names = NameBook(snapshot: try await core.snapshot(vaultId: vaultId))
        return LedgerCSV.renderAll(views.map { TransactionRow(view: $0, names: names) })
    }

    /// How to put a backup back, with the folder the database lives in (the
    /// app container's Application Support under the sandbox). The core runs
    /// SQLite in WAL mode, so the two side files have to go with the old
    /// database or they would be replayed onto the backup.
    static func restoreHint(databaseFolder: String) -> String {
        String(localized: "To restore it, quit Sparagne, delete sparagne.sqlite-wal and sparagne.sqlite-shm if they are there, and replace sparagne.sqlite with this backup, renamed sparagne.sqlite. The database is in:\n\(databaseFolder)")
    }

    /// The folder `CoreActor.databaseURL()` points into.
    static var databaseFolder: String {
        guard let url = try? CoreActor.databaseURL() else { return "~/Library/Application Support/Sparagne" }
        return url.deletingLastPathComponent().path(percentEncoded: false)
    }
}

/// The File menu's Back Up Database… and Export All Transactions… (and the
/// palette's same actions): listens for their notifications, asks where to
/// save with a save panel, and hands the work to `VaultExporter`. Failures
/// go through the window's error alert.
struct VaultExportHandlers: ViewModifier {
    let store: AppStore

    /// Where the last backup went, while the alert that says so is up.
    @State private var savedBackup: URL?

    func body(content: Content) -> some View {
        content
            .onReceive(NotificationCenter.default.publisher(for: .backupDatabase)) { _ in
                Task { await backUp() }
            }
            .onReceive(NotificationCenter.default.publisher(for: .exportAllTransactions)) { _ in
                Task { await exportAll() }
            }
            .alert(String(localized: "Backup saved"), item: $savedBackup) { url in
                Button(String(localized: "Show in Finder")) {
                    NSWorkspace.shared.activateFileViewerSelecting([url])
                }
                Button(String(localized: "OK"), role: .cancel) {}
            } message: { _ in
                Text(VaultExporter.restoreHint(databaseFolder: VaultExporter.databaseFolder))
            }
    }

    @MainActor
    private func backUp() async {
        let type = UTType(filenameExtension: "sqlite") ?? .data
        guard let destination = await Self.chooseDestination(name: VaultExporter.backupFileName(), type: type) else {
            return
        }
        do {
            try await VaultExporter.backup(core: store.core, to: destination)
            savedBackup = destination
        } catch {
            store.report(error)
        }
    }

    @MainActor
    private func exportAll() async {
        guard let vault = store.currentVault else { return }
        let name = LedgerCSV.allFileName(vault: vault.name)
        guard let destination = await Self.chooseDestination(name: name, type: .commaSeparatedText) else { return }
        do {
            let text = try await VaultExporter.allTransactionsCSV(core: store.core, vaultId: vault.id)
            try Data(text.utf8).write(to: destination)
        } catch {
            store.report(error)
        }
    }

    /// A save panel, as a sheet on the key window when there is one. The
    /// panel asks before replacing a file; `nil` when cancelled.
    @MainActor
    private static func chooseDestination(name: String, type: UTType) async -> URL? {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = name
        panel.allowedContentTypes = [type]
        panel.canCreateDirectories = true
        panel.isExtensionHidden = false
        let response: NSApplication.ModalResponse
        if let window = NSApp.keyWindow {
            response = await panel.beginSheetModal(for: window)
        } else {
            response = panel.runModal()
        }
        return response == .OK ? panel.url : nil
    }
}
