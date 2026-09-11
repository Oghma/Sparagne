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
    /// Says whether this vault is mine on the server, which is what the
    /// "Share…" entry needs (`docs/v2/SYNC.md` §3).
    let engine: SyncEngine?
    /// Requests one of `MainWindow`'s sheets; `MainWindow` owns the
    /// presentation state so every sheet, new or management, goes through
    /// one `.sheet(item:)`.
    let present: (MainWindow.SheetKind) -> Void

    @Environment(\.dismiss) private var dismiss

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
                        Button(vault.name) { store.select(vault) }
                    }
                    Divider()
                    Button(String(localized: "New Vault…")) { present(.vault) }
                    if let vault = store.currentVault, let engine, engine.isLoggedIn,
                        engine.isOwner(ofVault: vault.id) {
                        Button(String(localized: "Share…")) { present(.share(vault)) }
                    }
                } label: {
                    Label(
                        store.currentVault?.name ?? String(localized: "No vault"),
                        systemImage: "chevron.up.chevron.down"
                    )
                }
                .menuStyle(.borderlessButton)
            }

            Section(String(localized: "Wallets")) {
                ForEach(store.wallets, id: \.id) { wallet in
                    WalletManagementRow(wallet: wallet)
                        .contextMenu {
                            Button(String(localized: "Rename…")) { present(.renameWallet(wallet)) }
                            Button(String(localized: "Archive"), role: .destructive) {
                                store.archiveWallet(wallet.id)
                            }
                        }
                }
                if !store.archivedWallets.isEmpty {
                    DisclosureGroup(String(localized: "Archived")) {
                        ForEach(store.archivedWallets, id: \.id) { wallet in
                            ArchivedRow(name: wallet.name) { store.restoreWallet(wallet.id) }
                        }
                    }
                }
                Button(String(localized: "New Wallet…")) { present(.wallet) }
                    .buttonStyle(.link)
                    .disabled(store.currentVault == nil)
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
                        if !flow.isUnallocated {
                            Button(String(localized: "Rename…")) { present(.renameEnvelope(flow)) }
                            Button(String(localized: "Edit…")) { present(.editEnvelope(flow)) }
                            Button(String(localized: "Archive"), role: .destructive) {
                                store.archiveEnvelope(flow.id)
                            }
                        }
                    }
                }
                if !store.archivedFlows.isEmpty {
                    DisclosureGroup(String(localized: "Archived")) {
                        ForEach(store.archivedFlows, id: \.id) { flow in
                            ArchivedRow(name: flow.name) { store.restoreEnvelope(flow.id) }
                        }
                    }
                }
                Button(String(localized: "New Envelope…")) { present(.envelope) }
                    .buttonStyle(.link)
                    .disabled(store.currentVault == nil)
            }

            Section(String(localized: "Recurring")) {
                Button(String(localized: "Manage Recurring…")) { present(.recurring) }
                    .buttonStyle(.link)
                    .disabled(store.currentVault == nil)
            }
        }
        .listStyle(.inset)
    }
}

/// One archived wallet or envelope, with its Restore action.
private struct ArchivedRow: View {
    let name: String
    let restore: () -> Void

    var body: some View {
        HStack {
            Text(name).foregroundStyle(.secondary)
            Spacer()
            Button(String(localized: "Restore"), action: restore)
                .buttonStyle(.link)
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
