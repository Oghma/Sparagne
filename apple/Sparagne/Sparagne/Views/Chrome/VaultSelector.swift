import SwiftUI
import SparagneCore

/// The vault on screen, at the start of the top bar (`docs/v2/UI.md` §2.4):
/// its initial in a small mark, its name, and a menu with the other vaults
/// and the vault's life cycle.
///
/// The life-cycle entries reuse the Vault menu's notifications, so
/// `MainWindow` seeds every sheet in one place, and its rules
/// (`VaultPermissions`), so an entry is greyed out here exactly when it is
/// there.
struct VaultSelector: View {
    let store: AppStore
    let engine: SyncEngine?
    let present: (MainWindow.SheetKind) -> Void

    private var name: String {
        store.currentVault?.name ?? String(localized: "Sparagne")
    }

    private var others: [VaultView] {
        store.vaults.filter { $0.id != store.currentVault?.id }
    }

    var body: some View {
        let permissions = VaultPermissions(vault: store.currentVault, engine: engine)
        Menu {
            ForEach(others, id: \.id) { vault in
                Button(VaultNaming.label(for: vault, among: store.vaults)) {
                    Task { await store.select(vault) }
                }
            }
            if !others.isEmpty {
                Divider()
            }
            Button(String(localized: "New Vault\u{2026}")) {
                NotificationCenter.default.post(name: .newVault, object: nil)
            }
            Button(String(localized: "Rename\u{2026}")) {
                NotificationCenter.default.post(name: .renameVault, object: nil)
            }
            .disabled(!permissions.mayRename)
            Button(String(localized: "Share\u{2026}")) {
                if let vault = store.currentVault { present(.share(vault)) }
            }
            .disabled(!permissions.mayShare)
            Button(String(localized: "Delete\u{2026}")) {
                NotificationCenter.default.post(name: .deleteVault, object: nil)
            }
            .disabled(!permissions.mayDelete)
            Button(String(localized: "Leave Vault\u{2026}")) {
                NotificationCenter.default.post(name: .leaveVault, object: nil)
            }
            .disabled(!permissions.mayLeave)
        } label: {
            label
        }
        // A plain button style keeps the label as drawn: the default menu
        // button would replace it with a system pop-up button.
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .accessibilityLabel(String(localized: "Vault"))
        .accessibilityValue(name)
    }

    private var label: some View {
        HStack(spacing: 7) {
            Text(initial)
                .font(Face.ui(10.5, .bold))
                .foregroundStyle(Ink.accent)
                .frame(width: 18, height: 18)
                .background(Ink.accent.opacity(0.14), in: RoundedRectangle(cornerRadius: 5))
            Text(name)
                .font(Face.ui(13, .semibold))
                .foregroundStyle(Ink.text)
                .lineLimit(1)
            Image(systemName: "chevron.down")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(Ink.text3)
        }
        .padding(.leading, 4)
        .padding(.trailing, 8)
        .frame(height: Metrics.control)
        .contentShape(Rectangle())
    }

    /// The first letter of the name, as a mark: the vault is told apart at a
    /// glance before the name is read.
    private var initial: String {
        name.first.map { String($0).uppercased() } ?? "S"
    }
}

/// "DEMO" after the vault when the window is on another database
/// (`LaunchOptions.database`), so rows made up for a try are never mistaken
/// for the ledger. The tooltip names the file.
struct DemoBadge: View {
    let file: String

    var body: some View {
        Text(verbatim: "DEMO")
            .font(Face.ui(10, .semibold))
            .tracking(0.4)
            .foregroundStyle(Ink.text2)
            .padding(.horizontal, 5)
            .frame(height: 16)
            .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(Ink.line2, lineWidth: 1))
            .help(String(localized: "Demo database \(file): it never syncs"))
            .accessibilityLabel(String(localized: "Demo database \(file): it never syncs"))
    }
}
