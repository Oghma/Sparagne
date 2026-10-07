import SwiftUI
import SparagneCore

/// Everything that manages the vault's entities, in one sheet (⌘⇧M).
///
/// The ledger window has no sidebar (`docs/v2/UI.md` §2), so the vault picker,
/// the wallet and envelope balances and their management actions moved here.
/// Archived entities are behind a collapsed "Archived" group; capped envelopes
/// show `balance / cap` with the 70%/90% tinted bar
/// (docs/v2/DISTILLATO_V1.md §3.4).
struct ManagementSheet: View {
    let store: AppStore
    /// Says what the account may do to this vault on the server — share,
    /// rename, leave, delete (`docs/v2/SYNC.md` §3) — and which vaults are no
    /// longer shared with it.
    let engine: SyncEngine?
    /// Requests one of `MainWindow`'s sheets; `MainWindow` owns the
    /// presentation state so every sheet, new or management, goes through
    /// one `.sheet(item:)`.
    let present: (MainWindow.SheetKind) -> Void

    @Environment(\.dismiss) private var dismiss
    /// Why removing a vault no longer shared failed, shown under its row.
    @State private var removalFailure: String?

    /// Writes are refused on a vault the account only reads, so what would
    /// write is not offered.
    private var writable: Bool { store.currentVault != nil && !store.isReadOnly }

    var body: some View {
        VStack(spacing: 0) {
            list
            Divider()
            HStack {
                Spacer()
                Button(String(localized: "Done")) { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
            .padding(12)
        }
        .frame(width: 420, height: 520)
    }

    private var list: some View {
        List {
            Section {
                Menu {
                    ForEach(store.vaults, id: \.id) { vault in
                        Button(VaultNaming.label(for: vault, among: store.vaults)) {
                            Task { await store.select(vault) }
                        }
                    }
                    Divider()
                    Button(String(localized: "New Vault…")) { present(.vault) }
                    if let vault = store.currentVault {
                        // Each entry only when it would go through: the core
                        // and the server refuse the rest anyway.
                        if engine?.mayRenameVault(vault.id) ?? true {
                            Button(String(localized: "Rename…")) { present(.renameVault(vault)) }
                        }
                        if let engine, engine.isLoggedIn, engine.isOwner(ofVault: vault.id) {
                            Button(String(localized: "Share…")) { present(.share(vault)) }
                        }
                        if engine?.mayLeaveVault(vault.id) ?? false {
                            Divider()
                            Button(String(localized: "Leave…")) { present(.leaveVault(vault)) }
                        }
                        if engine?.mayDeleteVault(vault.id) ?? true {
                            Divider()
                            Button(String(localized: "Delete…"), role: .destructive) {
                                present(.deleteVault(vault))
                            }
                        }
                    }
                } label: {
                    Label(
                        store.currentVault.map { VaultNaming.label(for: $0, among: store.vaults) }
                            ?? String(localized: "No vault"),
                        systemImage: "chevron.up.chevron.down"
                    )
                }
                .menuStyle(.borderlessButton)
                if store.isReadOnly, let vault = store.currentVault, !(engine?.hasLostAccess(to: vault.id) ?? false) {
                    Text(String(localized: "You can read this vault but not change it."))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
            }

            if let engine, !engine.lostVaults.isEmpty {
                lostSection(engine)
            }

            Section(String(localized: "Wallets")) {
                ForEach(store.wallets, id: \.id) { wallet in
                    WalletManagementRow(wallet: wallet)
                        .contextMenu {
                            if writable {
                                Button(String(localized: "Rename…")) { present(.renameWallet(wallet)) }
                                Button(String(localized: "Archive"), role: .destructive) {
                                    Task { await store.archiveWallet(wallet.id) }
                                }
                            }
                        }
                }
                if !store.archivedWallets.isEmpty {
                    DisclosureGroup(String(localized: "Archived")) {
                        ForEach(store.archivedWallets, id: \.id) { wallet in
                            ArchivedRow(name: wallet.name, canRestore: writable) {
                                Task { await store.restoreWallet(wallet.id) }
                            }
                        }
                    }
                }
                Button(String(localized: "New Wallet…")) { present(.wallet) }
                    .buttonStyle(.link)
                    .disabled(!writable)
            }

            Section(String(localized: "Envelopes")) {
                ForEach(store.flows, id: \.id) { flow in
                    EnvelopeManagementRow(
                        flow: flow,
                        name: store.flowName(flow)
                    )
                    .contextMenu {
                        // Unallocated is a system envelope: the core
                        // refuses to update or archive it.
                        if !flow.isUnallocated, writable {
                            Button(String(localized: "Rename…")) { present(.renameEnvelope(flow)) }
                            Button(String(localized: "Edit…")) { present(.editEnvelope(flow)) }
                            Button(String(localized: "Archive"), role: .destructive) {
                                Task { await store.archiveEnvelope(flow.id) }
                            }
                        }
                    }
                }
                if !store.archivedFlows.isEmpty {
                    DisclosureGroup(String(localized: "Archived")) {
                        ForEach(store.archivedFlows, id: \.id) { flow in
                            ArchivedRow(name: flow.name, canRestore: writable) {
                                Task { await store.restoreEnvelope(flow.id) }
                            }
                        }
                    }
                }
                Button(String(localized: "New Envelope…")) { present(.envelope) }
                    .buttonStyle(.link)
                    .disabled(!writable)
            }

            Section(String(localized: "Recurring")) {
                // The templates live on the Ricorrenze tab now; the sheet
                // gets out of its way.
                Button(String(localized: "Manage Recurring…")) {
                    store.tab = .recurring
                    dismiss()
                }
                    .buttonStyle(.link)
                    .disabled(store.currentVault == nil)
            }
        }
        .listStyle(.inset)
        // A row that crosses the archive/restore boundary moves between two
        // separate `ForEach`s (the active `Section` and the archived
        // `DisclosureGroup`); `List` on macOS is NSTableView-backed and does
        // not reliably retire the outgoing cell across that boundary, so the
        // restored row can keep rendering with the archived row's look until
        // something forces the table to rebuild. Keying the whole `List` on
        // the active/archived membership does that rebuild automatically,
        // the same way leaving and reopening the sheet already does by hand.
        .id(listIdentity)
    }

    /// Vaults the server no longer shares with the account. Their copy stays
    /// here, read-only, until the user removes it: never dropped behind
    /// their back.
    private func lostSection(_ engine: SyncEngine) -> some View {
        Section(String(localized: "No longer shared with you")) {
            ForEach(engine.lostVaults, id: \.id) { vault in
                HStack {
                    Text(VaultNaming.label(for: vault, among: store.vaults))
                    Spacer()
                    Button(String(localized: "Remove from This Mac"), role: .destructive) {
                        Task {
                            do {
                                try await engine.removeFromThisMac(vault.id)
                                removalFailure = nil
                            } catch let error as DomainError {
                                removalFailure = "\(ErrorMessages.summary(for: error.code)): \(error.message)"
                            } catch {
                                removalFailure = error.localizedDescription
                            }
                        }
                    }
                    .buttonStyle(.link)
                }
            }
            Text(String(localized: "The owner stopped sharing these vaults with you, or they were removed from the server. What was synced before stays here, read-only, until you remove it."))
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let removalFailure {
                Text(removalFailure).font(.callout).foregroundStyle(.red)
            }
        }
    }

    /// One tag per wallet and envelope, active or archived. Changes whenever
    /// something crosses that boundary, which is exactly when `list` needs a
    /// fresh `List` identity (see the comment above).
    private var listIdentity: [String] {
        store.wallets.map { "wallet:\($0.id)" }
            + store.archivedWallets.map { "archived-wallet:\($0.id)" }
            + store.flows.map { "flow:\($0.id)" }
            + store.archivedFlows.map { "archived-flow:\($0.id)" }
    }
}

/// One archived wallet or envelope, with its Restore action when the vault
/// can be written.
private struct ArchivedRow: View {
    let name: String
    let canRestore: Bool
    let restore: () -> Void

    var body: some View {
        HStack {
            Text(name).foregroundStyle(.secondary)
            Spacer()
            if canRestore {
                Button(String(localized: "Restore"), action: restore)
                    .buttonStyle(.link)
            }
        }
    }
}

private struct WalletManagementRow: View {
    let wallet: WalletView

    var body: some View {
        HStack {
            Text(wallet.name)
            Spacer()
            Text(LedgerMoney.amount(wallet.balance))
                .monospacedDigit()
                .foregroundStyle(.secondary)
        }
    }
}

private struct EnvelopeManagementRow: View {
    let flow: FlowView
    let name: String

    /// The cap, when the mode has one.
    private var cap: Int64? {
        switch flow.mode {
        case .unlimited: nil
        case .netCapped(let cap), .incomeCapped(let cap): cap
        }
    }

    /// What fills the bar: the balance for a net cap, the cumulative income
    /// for an income cap (docs/v2/DISTILLATO_V1.md §2.2).
    private var filled: Int64 {
        switch flow.mode {
        case .incomeCapped: flow.incomeTotal ?? flow.balance
        case .unlimited, .netCapped: flow.balance
        }
    }

    private var progress: Double? {
        guard let cap, cap > 0 else { return nil }
        return min(max(Double(filled) / Double(cap), 0), 1)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(name)
                Spacer()
                if let cap {
                    Text("\(LedgerMoney.bare(flow.balance)) / \(LedgerMoney.amount(cap))")
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                } else {
                    Text(LedgerMoney.amount(flow.balance))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
            }
            if let progress {
                ProgressView(value: progress)
                    .tint(Ink.progressTint(progress))
            }
        }
    }
}
