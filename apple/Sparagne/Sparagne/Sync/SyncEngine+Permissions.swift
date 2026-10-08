import Foundation
import SparagneCore

/// What the account may do to a vault, from the roles the server listed
/// (remembered across launches in `AccountStore.vaultRoles`). The menus, the
/// palette and the management sheet offer an action only when it would go
/// through: the core and the server refuse the rest anyway.
extension SyncEngine {
    /// Rename is a write: allowed when the role can write, or when the
    /// server does not list the vault (local-only, not pushed yet). Never on
    /// a vault no longer shared with the account.
    func mayRenameVault(_ vaultId: Uuid) -> Bool {
        !readOnlyVaultIds.contains(vaultId)
    }

    /// Only the owner deletes a vault (`docs/v2/SYNC.md` §3). Logged in with
    /// the vault listed, the role says it; otherwise the core decides from
    /// the vault's owner and the author it signs with, and so does this.
    func mayDeleteVault(_ vaultId: Uuid) -> Bool {
        guard !hasLostAccess(to: vaultId) else { return false }
        if isLoggedIn, let role = role(forVault: vaultId) { return role == .owner }
        guard let vault = store.vaults.first(where: { $0.id == vaultId }) else { return false }
        return vault.owner == account.author
    }

    /// A member who is not the owner may leave a vault the server lists; the
    /// owner cannot leave their own.
    func mayLeaveVault(_ vaultId: Uuid) -> Bool {
        guard isLoggedIn, let role = role(forVault: vaultId) else { return false }
        return role != .owner
    }
}

/// What the vault on screen allows, for the three places that offer the
/// vault's life cycle: the Vault menu, the top bar's vault selector and the
/// command palette (`docs/v2/UI.md` §2.4). One set of rules, so the three
/// never disagree about an entry.
///
/// Without an engine (a demo database) nothing is on a server: the core
/// alone decides, so rename and delete are offered and leave and share are
/// not.
struct VaultPermissions: Equatable {
    var mayRename = false
    var mayDelete = false
    var mayLeave = false
    /// Owner only, and only with an account the server can share through.
    var mayShare = false

    init(vault: VaultView?, engine: SyncEngine?) {
        guard let vault else { return }
        mayRename = engine?.mayRenameVault(vault.id) ?? true
        mayDelete = engine?.mayDeleteVault(vault.id) ?? true
        mayLeave = engine?.mayLeaveVault(vault.id) ?? false
        mayShare = engine.map { $0.isLoggedIn && $0.isOwner(ofVault: vault.id) } ?? false
    }
}
