import Foundation
import SparagneCore

/// Remembers the mapping a statement was last imported with, so the next
/// export from the same bank opens already mapped.
///
/// One mapping per vault and header: the same card exported into two vaults
/// may go to different wallets, and a bank that changes its columns is a new
/// file to map. The mapping is kept as the core's own JSON
/// (`encodeStatementMapping`), all of them in one dictionary under
/// `defaultsKey`. A stored mapping that no longer decodes (the core changed
/// its shape) reads as nothing remembered.
struct StatementMappingStore {
    static let defaultsKey = "statementMappings"

    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    /// `<vault id>|<header>`, the header being each column trimmed and
    /// lowercased, joined by commas: the comparison the core itself uses to
    /// find a mapped column, so a header that differs only in case or padding
    /// is the same file.
    static func key(vaultId: Uuid, headers: [String]) -> String {
        let header = headers
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }
            .joined(separator: ",")
        return "\(vaultId)|\(header)"
    }

    /// The mapping last imported into `vaultId` from a file with `headers`.
    func mapping(vaultId: Uuid, headers: [String]) -> StatementMapping? {
        guard let json = stored[Self.key(vaultId: vaultId, headers: headers)] else { return nil }
        return try? decodeStatementMapping(json: json)
    }

    /// Replaces whatever was remembered for `vaultId` and `headers`.
    func remember(_ mapping: StatementMapping, vaultId: Uuid, headers: [String]) {
        var all = stored
        all[Self.key(vaultId: vaultId, headers: headers)] = encodeStatementMapping(mapping: mapping)
        defaults.set(all, forKey: Self.defaultsKey)
    }

    private var stored: [String: String] {
        defaults.dictionary(forKey: Self.defaultsKey) as? [String: String] ?? [:]
    }
}
