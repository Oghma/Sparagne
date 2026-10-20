import Foundation
import SparagneCore

/// The mapping and target a statement was last imported with, per vault and header.
struct StatementMappingStore {
    // The mapping is the core's own JSON (`encodeStatementMapping`): one that no longer
    // decodes reads as nothing remembered. Targets sit apart under `targetsKey`.

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
    /// whatever its header, the wallets its accounts went to included: the
    /// wallets statements go to.
    func targetWallets(vaultId: Uuid) -> Set<Uuid> {
        let prefix = "\(vaultId)|"
        var wallets: Set<Uuid> = []
        for (key, json) in stored(Self.targetsKey) where key.hasPrefix(prefix) {
            guard let target = try? JSONDecoder().decode(StatementImportTarget.self, from: Data(json.utf8)) else {
                continue
            }
            if let wallet = target.walletId { wallets.insert(wallet) }
            wallets.formUnion(target.accounts.compactMap(\.walletId))
        }
        return wallets
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

/// Where a statement was imported: wallet, envelope, start day, each account's wallet; any may be missing.
nonisolated struct StatementImportTarget: Codable, Equatable, Sendable {
    // The wallets also searched for duplicates are left out on purpose: left ticked, they
    // would keep moving cash spends into the bank.

    var walletId: Uuid?
    /// `nil` is Unallocated.
    var flowId: Uuid?
    /// `yyyy-MM-dd`; `nil` imports the whole file.
    var since: NaiveDate?
    /// The accounts sent elsewhere than `walletId`, by name as the file
    /// writes it; a `nil` wallet left the account out.
    var accounts: [Account] = []

    /// Where one account of the file went.
    struct Account: Codable, Equatable, Sendable {
        var account: String
        /// `nil`: left out.
        var walletId: Uuid?
    }

    init(walletId: Uuid?, flowId: Uuid?, since: NaiveDate? = nil, accounts: [Account] = []) {
        self.walletId = walletId
        self.flowId = flowId
        self.since = since
        self.accounts = accounts
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        walletId = try container.decodeIfPresent(Uuid.self, forKey: .walletId)
        flowId = try container.decodeIfPresent(Uuid.self, forKey: .flowId)
        since = try container.decodeIfPresent(NaiveDate.self, forKey: .since)
        accounts = try container.decodeIfPresent([Account].self, forKey: .accounts) ?? []
    }
}
