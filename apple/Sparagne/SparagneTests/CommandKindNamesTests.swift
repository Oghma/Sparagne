import Foundation
import Testing

@testable import Sparagne

/// The refused-changes list names each command in words, not by its tag.
struct CommandKindNamesTests {
    /// Every tag `Command::kind_name()` returns (`core/src/command.rs`),
    /// copied here so a new command without a name fails this test.
    static let kinds = [
        "create_vault", "rename_vault", "delete_vault",
        "create_wallet", "rename_wallet", "archive_wallet", "restore_wallet",
        "create_flow", "update_flow", "archive_flow", "restore_flow",
        "create_category", "rename_category", "archive_category", "restore_category",
        "add_alias", "remove_alias", "merge_category",
        "income", "expense", "refund",
        "transfer_wallet", "transfer_flow", "update_transaction", "void_transaction",
        "create_recurring", "update_recurring", "archive_recurring", "restore_recurring",
        "execute_recurring", "skip_recurring",
        "create_allocation_plan", "update_allocation_plan", "execute_allocation",
        "skip_allocation", "reopen_allocation",
    ]

    @Test("Every one of the 36 command kinds has its own name")
    func everyKindIsNamed() {
        #expect(Self.kinds.count == 36)
        let names = Self.kinds.map(CommandKindNames.name(for:))
        for (kind, name) in zip(Self.kinds, names) {
            #expect(name != kind, "\(kind) has no name")
            #expect(!name.isEmpty)
        }
        #expect(Set(names).count == names.count)
    }

    @Test("A kind the table does not know is shown as it is")
    func unknownKindFallsBack() {
        #expect(CommandKindNames.name(for: "split_transaction") == "split_transaction")
    }
}
