import Foundation

/// What a refused command did, in words; a tag a newer core sends and this lacks shows as it is.
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
        case "create_flow": String(localized: "Create flow")
        case "update_flow": String(localized: "Edit flow")
        case "archive_flow": String(localized: "Archive flow")
        case "restore_flow": String(localized: "Restore flow")
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
        case "transfer_flow": String(localized: "Transfer between flows")
        case "update_transaction": String(localized: "Edit transaction")
        case "void_transaction": String(localized: "Delete Transaction")
        case "create_recurring": String(localized: "Create recurring")
        case "update_recurring": String(localized: "Edit recurring")
        case "archive_recurring": String(localized: "Archive recurring")
        case "restore_recurring": String(localized: "Restore recurring")
        case "execute_recurring": String(localized: "Run recurring")
        case "skip_recurring": String(localized: "Skip recurring")
        case "create_allocation_plan": String(localized: "Create allocation plan")
        case "update_allocation_plan": String(localized: "Edit allocation plan")
        case "execute_allocation": String(localized: "Share out to flows")
        case "skip_allocation": String(localized: "Skip allocation")
        case "reopen_allocation": String(localized: "Undo allocation")
        case "create_loan": String(localized: "Create loan")
        case "update_loan": String(localized: "Edit loan")
        case "set_loan_recurring": String(localized: "Link loan to recurring")
        case "archive_loan": String(localized: "Archive loan")
        case "restore_loan": String(localized: "Restore loan")
        case "add_loan_event": String(localized: "Add loan event")
        case "remove_loan_event": String(localized: "Remove loan event")
        default: kind
        }
    }
}
