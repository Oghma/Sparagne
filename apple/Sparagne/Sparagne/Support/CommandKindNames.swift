import Foundation

/// What a command did, in words, for the list of refused changes.
///
/// The core names a command by its snake_case tag (`Command::kind_name()` in
/// `core/src/command.rs`), which is what `RejectedCommand.kind` carries. A
/// tag this table does not know yet — a newer core — is shown as it is
/// rather than hidden.
enum CommandKindNames {
    static func name(for kind: String) -> String {
        switch kind {
        case "create_vault": String(localized: "Create vault")
        case "rename_vault": String(localized: "Rename vault")
        case "delete_vault": String(localized: "Delete vault")
        case "create_wallet": String(localized: "Create wallet")
        case "rename_wallet": String(localized: "Rename wallet")
        case "archive_wallet": String(localized: "Archive wallet")
        case "restore_wallet": String(localized: "Restore wallet")
        case "create_flow": String(localized: "Create envelope")
        case "update_flow": String(localized: "Edit envelope")
        case "archive_flow": String(localized: "Archive envelope")
        case "restore_flow": String(localized: "Restore envelope")
        case "create_category": String(localized: "Create category")
        case "rename_category": String(localized: "Rename category")
        case "archive_category": String(localized: "Archive category")
        case "restore_category": String(localized: "Restore category")
        case "add_alias": String(localized: "Add alias")
        case "remove_alias": String(localized: "Remove alias")
        case "merge_category": String(localized: "Merge categories")
        case "income": String(localized: "Add income")
        case "expense": String(localized: "Add expense")
        case "refund": String(localized: "Add refund")
        case "transfer_wallet": String(localized: "Transfer between wallets")
        case "transfer_flow": String(localized: "Transfer between envelopes")
        case "update_transaction": String(localized: "Edit transaction")
        case "void_transaction": String(localized: "Void transaction")
        case "create_recurring": String(localized: "Create recurring")
        case "update_recurring": String(localized: "Edit recurring")
        case "archive_recurring": String(localized: "Archive recurring")
        case "restore_recurring": String(localized: "Restore recurring")
        case "execute_recurring": String(localized: "Run recurring")
        case "skip_recurring": String(localized: "Skip recurring")
        default: kind
        }
    }
}
