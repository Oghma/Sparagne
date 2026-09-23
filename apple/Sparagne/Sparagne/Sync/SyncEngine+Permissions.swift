import Foundation
import SparagneCore

/// What the account may do to a vault, from the roles the server lists.
/// A stub for now; `mayDeleteVault` still lives in `SyncEngine.swift`.
extension SyncEngine {
    /// Rename is a write: allowed unless the account only reads the vault.
    func mayRenameVault(_ vaultId: Uuid) -> Bool {
        true
    }

    /// A member (not the owner) of a vault the server lists may leave it.
    func mayLeaveVault(_ vaultId: Uuid) -> Bool {
        false
    }
}
