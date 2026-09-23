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
