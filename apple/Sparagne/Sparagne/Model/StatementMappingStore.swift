import Foundation
import SparagneCore

/// Remembers the mapping a statement was last imported with, and where it
/// went (`StatementImportTarget`), so the next export from the same bank
/// opens already mapped and aimed at its wallet.
///
/// One mapping per vault and header: the same card exported into two vaults
/// may go to different wallets, and a bank that changes its columns is a new
/// file to map. The mapping is kept as the core's own JSON
/// (`encodeStatementMapping`), all of them in one dictionary under
/// `defaultsKey`. A stored mapping that no longer decodes (the core changed
/// its shape) reads as nothing remembered. The target (`StatementImportTarget`)
/// sits beside it under `targetsKey`, keyed the same way, so a mapping saved
/// before targets were remembered still reads back on its own.
struct StatementMappingStore {
    static let defaultsKey = "statementMappings"
    static let targetsKey = "statementImportOptions"

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
        guard let json = stored(Self.defaultsKey)[Self.key(vaultId: vaultId, headers: headers)] else { return nil }
        return try? decodeStatementMapping(json: json)
    }

    /// Replaces whatever was remembered for `vaultId` and `headers`.
    func remember(_ mapping: StatementMapping, vaultId: Uuid, headers: [String]) {
        var all = stored(Self.defaultsKey)
        all[Self.key(vaultId: vaultId, headers: headers)] = encodeStatementMapping(mapping: mapping)
        defaults.set(all, forKey: Self.defaultsKey)
    }

    /// Where the last import into `vaultId` from a file with `headers` went,
    /// as it was then: the ids may name wallets or envelopes gone since,
    /// which the import sheet leaves out (`StatementImportModel.restore`).
    func target(vaultId: Uuid, headers: [String]) -> StatementImportTarget? {
        guard let json = stored(Self.targetsKey)[Self.key(vaultId: vaultId, headers: headers)] else { return nil }
        return try? JSONDecoder().decode(StatementImportTarget.self, from: Data(json.utf8))
    }

    /// Every wallet an import into `vaultId` was remembered going to,
    /// whatever its header: the wallets statements go to.
    func targetWallets(vaultId: Uuid) -> Set<Uuid> {
        let prefix = "\(vaultId)|"
        return Set(stored(Self.targetsKey).compactMap { key, json -> Uuid? in
            guard key.hasPrefix(prefix) else { return nil }
            return (try? JSONDecoder().decode(StatementImportTarget.self, from: Data(json.utf8)))?.walletId
        })
    }

    /// Replaces the target remembered for `vaultId` and `headers`.
    func remember(_ target: StatementImportTarget, vaultId: Uuid, headers: [String]) {
        guard let data = try? JSONEncoder().encode(target), let json = String(data: data, encoding: .utf8) else {
            return
        }
        var all = stored(Self.targetsKey)
        all[Self.key(vaultId: vaultId, headers: headers)] = json
        defaults.set(all, forKey: Self.targetsKey)
    }

    private func stored(_ key: String) -> [String: String] {
        defaults.dictionary(forKey: key) as? [String: String] ?? [:]
    }
}

/// Where a statement was imported: the wallet, the envelope and the start
/// day. Every field may be missing from what was stored, and a field this
/// version does not know is ignored, so a target written by another version
/// of the app still reads.
///
/// The wallets also searched for duplicates are not part of it on purpose:
/// they serve the move from typing by hand into one wallet to importing the
/// banks, and left ticked they would keep moving real cash spends into the
/// bank. Every import starts with none.
nonisolated struct StatementImportTarget: Codable, Equatable, Sendable {
    var walletId: Uuid?
    /// `nil` is Unallocated.
    var flowId: Uuid?
    /// `yyyy-MM-dd`; `nil` imports the whole file.
    var since: NaiveDate?

    init(walletId: Uuid?, flowId: Uuid?, since: NaiveDate? = nil) {
        self.walletId = walletId
        self.flowId = flowId
        self.since = since
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        walletId = try container.decodeIfPresent(Uuid.self, forKey: .walletId)
        flowId = try container.decodeIfPresent(Uuid.self, forKey: .flowId)
        since = try container.decodeIfPresent(NaiveDate.self, forKey: .since)
    }
}
