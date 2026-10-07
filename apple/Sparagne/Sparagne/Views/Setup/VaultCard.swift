import SwiftUI
import SparagneCore

/// The vault's own card at the top of SETUP's left column
/// (`docs/v2/UI.md` §2.3): its name, its currency and who it is shared with.
/// The name and the currency are facts here; renaming goes through the
/// Vault menu, and a vault's currency never changes.
struct VaultCard: View {
    let store: AppStore
    /// `nil` before the database is open and in a demo database.
    let engine: SyncEngine?
    /// Opens one of the window's sheets, here the sharing one.
    let present: (MainWindow.SheetKind) -> Void

    /// What the server lists, or `nil` until it answers (or when it cannot):
    /// the card then shows the owner alone.
    @State private var members: [MemberEntry]?

    var body: some View {
        SetupCard(title: String(localized: "Vault")) {
            shareButton
        } content: {
            if let vault = store.currentVault {
                VStack(spacing: 0) {
                    row(String(localized: "Name"), separator: false) {
                        Text(vault.name).foregroundStyle(Ink.text)
                    }
                    row(String(localized: "Currency")) {
                        HStack(spacing: 6) {
                            Text(Self.currencyLabel(vault.currency)).foregroundStyle(Ink.text)
                            Text(String(localized: "can't be changed"))
                                .font(Face.ui(11.5))
                                .foregroundStyle(Ink.text3)
                        }
                    }
                    row(String(localized: "Members")) { membersCell(vault) }
                }
            }
        }
        .task(id: MembersKey(vault: store.currentVault?.id, loggedIn: engine?.isLoggedIn == true)) {
            await loadMembers()
        }
    }

    // MARK: - Pieces

    /// "Share…" at the title's right, only where it would go through:
    /// logged in and the vault's owner (`VaultPermissions`, the same rule the
    /// Vault menu and the palette use).
    @ViewBuilder
    private var shareButton: some View {
        if let vault = store.currentVault, engine?.isLoggedIn == true,
           VaultPermissions(vault: vault, engine: engine).mayShare {
            Button {
                present(.share(vault))
            } label: {
                Label(String(localized: "Share…"), systemImage: "person.badge.plus")
                    .font(Face.ui(11.5, .medium))
                    .labelStyle(.titleAndIcon)
                    .foregroundStyle(Ink.text)
                    .padding(.horizontal, 8)
                    .frame(height: 22)
                    .background(Ink.card, in: RoundedRectangle(cornerRadius: 6))
                    .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Ink.line2, lineWidth: 1))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }

    /// A label of 84 pt and its value, 28 pt tall, a hairline between rows.
    private func row<Value: View>(
        _ label: String,
        separator: Bool = true,
        @ViewBuilder value: () -> Value
    ) -> some View {
        HStack(spacing: 8) {
            Text(label)
                .font(Face.row)
                .foregroundStyle(Ink.text2)
                .frame(width: 84, alignment: .leading)
            value()
                .font(Face.row)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(minHeight: 28)
        .overlay(alignment: .top) {
            if separator { Rectangle().fill(Ink.rowLine).frame(height: 1) }
        }
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private func membersCell(_ vault: VaultView) -> some View {
        if let members, !members.isEmpty {
            HStack(spacing: 12) {
                ForEach(members) { member in
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text(member.username).foregroundStyle(Ink.text)
                        Text(member.role.label).font(Face.ui(11)).foregroundStyle(Ink.text3)
                    }
                }
            }
        } else if engine == nil {
            Text(String(localized: "Only on this Mac")).foregroundStyle(Ink.text2)
        } else {
            Text(vault.owner).foregroundStyle(Ink.text)
        }
    }

    // MARK: - Loading

    private struct MembersKey: Equatable {
        let vault: Uuid?
        let loggedIn: Bool
    }

    /// Only with an account: the list is the server's. A refusal or an
    /// offline Mac leaves the owner alone, which is true either way.
    private func loadMembers() async {
        members = nil
        guard let vault = store.currentVault, let engine, engine.isLoggedIn else { return }
        let loaded = try? await engine.members(ofVault: vault.id)
        guard store.currentVault?.id == vault.id else { return }
        members = loaded
    }

    /// "EUR · €".
    static func currencyLabel(_ currency: Currency) -> String {
        switch currency {
        case .eur: "EUR · €"
        }
    }
}
