import SwiftUI
import SparagneCore

/// The vault on screen and its life cycle, in one sheet (⌘⇧M): switch to
/// another vault, start a new one, rename, share, leave or delete this one,
/// and remove the copies of vaults no longer shared.
///
/// Wallets, envelopes and categories live on the Setup tab, the recurring
/// templates on the Ricorrenze tab (`docs/v2/UI.md` §2.3, §2.5); the sheet
/// links to both instead of keeping a second, smaller copy of either.
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

    var body: some View {
        FormSheet(String(localized: "Vault"), width: 460) {
            VStack(alignment: .leading, spacing: 10) {
                vaultPicker
                actions
                if store.isReadOnly, let vault = store.currentVault, !(engine?.hasLostAccess(to: vault.id) ?? false) {
                    FormNote(String(localized: "You can read this vault but not change it."), indented: false)
                }
            }

            if let engine, !engine.lostVaults.isEmpty {
                lostSection(engine)
            }

            links
        } footer: {
            FormPrimaryButton(title: String(localized: "Done")) { dismiss() }
        }
    }

    // MARK: - The vault on screen

    private var vaultPicker: some View {
        FormMenu(
            label: String(localized: "Vault"),
            value: store.currentVault.map { VaultNaming.label(for: $0, among: store.vaults) }
                ?? String(localized: "No vault")
        ) {
            ForEach(store.vaults, id: \.id) { vault in
                // A toggle, for the menu's checkmark on the vault on screen.
                Toggle(
                    VaultNaming.label(for: vault, among: store.vaults),
                    isOn: Binding(
                        get: { vault.id == store.currentVault?.id },
                        set: { _ in Task { await store.select(vault) } }
                    )
                )
            }
        }
    }

    /// The life cycle, each action only when it would go through: the core
    /// and the server refuse the rest anyway. The rules are the Vault menu's
    /// and the top bar's (`VaultPermissions`), so the three never disagree.
    private var actions: some View {
        let permissions = VaultPermissions(vault: store.currentVault, engine: engine)
        return HStack(spacing: 8) {
            Button(String(localized: "New Vault…")) { present(.vault) }
            if let vault = store.currentVault {
                if permissions.mayRename {
                    Button(String(localized: "Rename…")) { present(.renameVault(vault)) }
                }
                if permissions.mayShare {
                    Button(String(localized: "Share…")) { present(.share(vault)) }
                }
                if permissions.mayLeave {
                    Button(String(localized: "Leave…")) { present(.leaveVault(vault)) }
                }
                Spacer(minLength: 8)
                if permissions.mayDelete {
                    FormDestructiveButton(title: String(localized: "Delete…")) { present(.deleteVault(vault)) }
                }
            }
        }
        .buttonStyle(.chrome(.bordered))
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: - Vaults no longer shared

    /// Vaults the server no longer shares with the account. Their copy stays
    /// here, read-only, until the user removes it: never dropped behind
    /// their back.
    private func lostSection(_ engine: SyncEngine) -> some View {
        FormGroup(String(localized: "No longer shared with you")) {
            Panel(padding: 0) {
                VStack(spacing: 0) {
                    ForEach(Array(engine.lostVaults.enumerated()), id: \.element.id) { index, vault in
                        if index > 0 { Hairline() }
                        HStack(spacing: 8) {
                            Text(VaultNaming.label(for: vault, among: store.vaults))
                                .font(Face.ui(13))
                                .foregroundStyle(Ink.text)
                            Spacer(minLength: 8)
                            FormDestructiveButton(title: String(localized: "Remove from This Mac"), small: true) {
                                remove(vault, from: engine)
                            }
                        }
                        .padding(.horizontal, Metrics.cardPad)
                        .frame(minHeight: 34)
                    }
                }
            }
            FormNote(
                String(localized: "The owner stopped sharing these vaults with you, or they were removed from the server. What was synced before stays here, read-only, until you remove it."),
                indented: false
            )
            if let removalFailure {
                FormNote(removalFailure, tone: .negative, indented: false)
            }
        }
    }

    private func remove(_ vault: VaultView, from engine: SyncEngine) {
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

    // MARK: - Elsewhere in the window

    /// What this sheet used to list, where it lives now: each row closes the
    /// sheet on its tab.
    private var links: some View {
        Panel(padding: 0) {
            VStack(spacing: 0) {
                TabLink(title: String(localized: "Wallets, envelopes and categories"), tab: .setup, go: go)
                Hairline()
                TabLink(title: LedgerTab.recurring.label, tab: .recurring, go: go)
            }
            .clipShape(RoundedRectangle(cornerRadius: Metrics.cardRadius))
        }
        .disabled(store.currentVault == nil)
    }

    private func go(_ tab: LedgerTab) {
        store.tab = tab
        dismiss()
    }
}

/// A row that takes the window to one of its tabs: the name, the tab's ⌘
/// shortcut as a reminder, and a chevron.
private struct TabLink: View {
    let title: String
    let tab: LedgerTab
    let go: (LedgerTab) -> Void

    @State private var hovered = false
    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        Button {
            go(tab)
        } label: {
            HStack(spacing: 8) {
                Text(title)
                    .font(Face.ui(13))
                    .foregroundStyle(Ink.text)
                Spacer(minLength: 8)
                KeyCap(text: "\u{2318}\(tab.shortcut)")
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(Ink.text3)
            }
            .padding(.horizontal, Metrics.cardPad)
            .frame(height: 34)
            .background(hovered && isEnabled ? Ink.raised : Color.clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(isEnabled ? 1 : 0.4)
        .onHover { hovered = $0 }
        // The name alone: the key cap and the chevron are for the eye.
        .accessibilityLabel(title)
    }
}
