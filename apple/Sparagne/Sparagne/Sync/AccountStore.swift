import Foundation
import Observation
import Security
import SparagneCore

/// Where the session token lives. The app keeps it in the Keychain; the tests
/// keep it in memory.
protocol TokenStore: Sendable {
    func token(for account: String) -> String?
    /// Throws when the token could not be stored: the session still works,
    /// but the next launch will not find it.
    func save(_ token: String, for account: String) throws
    func delete(for account: String)
}

/// A Keychain call that did not succeed, with Security's own description.
struct KeychainError: Error, Equatable, Sendable, CustomStringConvertible {
    let status: OSStatus

    var description: String {
        let text = SecCopyErrorMessageString(status, nil) as String? ?? "OSStatus \(status)"
        return "\(text) (\(status))"
    }
}

/// Generic-password items under the service `it.oghma.sparagne`, one per
/// username, in the legacy file-based keychain: the data-protection keychain
/// needs a team identifier, which an ad-hoc signed build does not have.
///
/// Reads and deletes are best effort. A save reports its failure, because a
/// token the Keychain refused only lives until the app quits and Settings
/// says so (`AccountStore.tokenSaveFailure`).
struct KeychainTokenStore: TokenStore {
    static let service = "it.oghma.sparagne"
    /// What Keychain Access shows for the item.
    static let label = "Sparagne sync session"

    private func query(for account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account,
            kSecUseDataProtectionKeychain as String: false,
        ]
    }

    func token(for account: String) -> String? {
        var query = query(for: account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
            let data = item as? Data
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// Updates the item in place, adds it when there is none, and replaces
    /// it when the update is refused: an item written by another build is
    /// bound to that build's signature, and an ad-hoc signature changes with
    /// every build.
    func save(_ token: String, for account: String) throws {
        let query = query(for: account)
        let attributes: [String: Any] = [
            kSecValueData as String: Data(token.utf8),
            kSecAttrLabel as String: Self.label,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlock,
        ]
        var status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if status != errSecSuccess {
            if status != errSecItemNotFound { _ = SecItemDelete(query as CFDictionary) }
            status = SecItemAdd(query.merging(attributes) { _, new in new } as CFDictionary, nil)
        }
        guard status == errSecSuccess else { throw KeychainError(status: status) }
    }

    func delete(for account: String) {
        _ = SecItemDelete(query(for: account) as CFDictionary)
    }
}

/// A token store that forgets everything when the process ends.
final class MemoryTokenStore: TokenStore, @unchecked Sendable {
    private let lock = NSLock()
    private var tokens: [String: String] = [:]

    init() {}

    func token(for account: String) -> String? {
        lock.withLock { tokens[account] }
    }

    func save(_ token: String, for account: String) {
        lock.withLock { tokens[account] = token }
    }

    func delete(for account: String) {
        _ = lock.withLock { tokens.removeValue(forKey: account) }
    }
}

/// The server this app talks to and who it talks as.
///
/// The URL, the username and the token's expiry are preferences; the token is
/// a secret and lives only in the Keychain, never in a file or in the
/// defaults. The username is also the `author` of every command
/// (`docs/v2/SYNC.md` §1), which is why `SyncEngine` relabels the outbox as
/// soon as a login succeeds.
///
/// It also remembers what the server said about the account's vaults — the
/// roles of the last `GET /vaults` and the vaults it lost access to — so a
/// viewer's vault is read-only from the first frame after a relaunch, even
/// offline.
@Observable
@MainActor
final class AccountStore {
    static let serverURLKey = "syncServerURL"
    static let usernameKey = "syncUsername"
    static let expiresAtKey = "syncTokenExpiresAt"
    static let vaultRolesKey = "syncVaultRoles"
    static let lostVaultsKey = "syncLostVaults"
    static let defaultServerURL = "http://127.0.0.1:3000"

    static let localAuthorKey = "localAuthor"

    /// What signs commands when nobody is logged in and no name was chosen:
    /// the macOS account.
    static var systemAuthor: String { NSUserName() }

    /// The name chosen in Settings for the rows written while logged out. A
    /// household ledger names people, not logins, so the macOS account name
    /// is only the fallback (`docs/v2/UI.md` §3, PERSONA).
    var localAuthor: String {
        didSet { defaults.set(localAuthor, forKey: Self.localAuthorKey) }
    }

    @ObservationIgnored private let defaults: UserDefaults
    @ObservationIgnored private let tokens: TokenStore

    /// What the settings field holds; not necessarily a valid URL yet.
    var serverURLText: String {
        didSet { defaults.set(serverURLText, forKey: Self.serverURLKey) }
    }

    /// The logged-in account, `nil` while logged out.
    private(set) var username: String?
    private(set) var token: String?
    /// When the server stops accepting `token` (`TokenResponse.expiresAt`).
    private(set) var expiresAt: Date?
    /// The last name that logged in, kept after a logout so the login form
    /// comes back filled in.
    private(set) var lastUsername: String
    /// The session ended on its own — the token expired, or the server
    /// stopped accepting it — rather than by logging out.
    private(set) var sessionExpired = false
    /// Why the Keychain refused the token of this session, which then lasts
    /// only until the app quits. `nil` when it was saved.
    private(set) var tokenSaveFailure: String?

    /// The role the server gave the account on each vault it listed at the
    /// last `GET /vaults`.
    private(set) var vaultRoles: [Uuid: MemberRole]
    /// Vaults the account had, with server history, that the server no
    /// longer lists and no longer lets it pull: no longer shared with it.
    private(set) var lostVaultIds: Set<Uuid>

    init(
        defaults: UserDefaults = .standard,
        tokens: TokenStore = KeychainTokenStore(),
        now: Date = Date()
    ) {
        self.defaults = defaults
        self.tokens = tokens
        serverURLText = defaults.string(forKey: Self.serverURLKey) ?? Self.defaultServerURL
        localAuthor = defaults.string(forKey: Self.localAuthorKey) ?? ""
        let stored = defaults.string(forKey: Self.usernameKey)
        lastUsername = stored ?? ""
        let expiry = (defaults.object(forKey: Self.expiresAtKey) as? Double).map(Date.init(timeIntervalSince1970:))
        vaultRoles = Self.decodeRoles(defaults.dictionary(forKey: Self.vaultRolesKey))
        lostVaultIds = Set(defaults.stringArray(forKey: Self.lostVaultsKey) ?? [])

        if let stored, let expiry, expiry <= now {
            // A token past its expiry would only earn a 401: the session is
            // over, the name stays for the login form.
            tokens.delete(for: stored)
            defaults.removeObject(forKey: Self.expiresAtKey)
            sessionExpired = true
            username = nil
            token = nil
            expiresAt = nil
        } else {
            let held = stored.flatMap { tokens.token(for: $0) }
            username = held == nil ? nil : stored
            token = held
            expiresAt = held == nil ? nil : expiry
        }
    }

    var isLoggedIn: Bool { token != nil && username != nil }

    /// The base URL, when the field holds one with a scheme and a host.
    var baseURL: URL? {
        let trimmed = serverURLText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let url = URL(string: trimmed), url.scheme != nil, url.host() != nil
        else { return nil }
        return url
    }

    /// The author to stamp on new commands: the account when there is one,
    /// else the name chosen in Settings, else the macOS account.
    var author: String {
        username ?? Self.author(chosen: localAuthor)
    }

    /// Who signs the rows of a launch that never logs in (`LaunchOptions`):
    /// `author` while logged out. Read from the defaults alone, so the
    /// Keychain is not asked for a token nobody will use.
    static func loggedOutAuthor(defaults: UserDefaults = .standard) -> String {
        author(chosen: defaults.string(forKey: localAuthorKey) ?? "")
    }

    private static func author(chosen: String) -> String {
        let trimmed = chosen.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? systemAuthor : trimmed
    }

    func signIn(username: String, token: String, expiresAt: Date? = nil) {
        if username != lastUsername {
            // Another account: what the server said about the previous one's
            // vaults says nothing about this one's.
            adoptRoles([:])
        }
        self.username = username
        self.token = token
        self.expiresAt = expiresAt
        lastUsername = username
        sessionExpired = false
        defaults.set(username, forKey: Self.usernameKey)
        defaults.set(expiresAt?.timeIntervalSince1970, forKey: Self.expiresAtKey)
        do {
            try tokens.save(token, for: username)
            tokenSaveFailure = nil
        } catch {
            tokenSaveFailure = String(describing: error)
        }
    }

    /// Logs out, keeping the name for the next login. `expired` says the
    /// server ended the session, not the user.
    func signOut(expired: Bool = false) {
        if let username { tokens.delete(for: username) }
        token = nil
        username = nil
        expiresAt = nil
        tokenSaveFailure = nil
        sessionExpired = expired
        defaults.removeObject(forKey: Self.expiresAtKey)
    }

    // MARK: - What the server said about the vaults

    /// The roles of a `GET /vaults`. A vault listed again is no longer lost.
    func adoptRoles(_ roles: [Uuid: MemberRole]) {
        vaultRoles = roles
        lostVaultIds.subtract(roles.keys)
        persistVaults()
    }

    func markLost(_ vaultId: Uuid) {
        vaultRoles[vaultId] = nil
        lostVaultIds.insert(vaultId)
        persistVaults()
    }

    /// The vault left this device (left, deleted, removed): nothing more to
    /// remember about it.
    func forget(vault vaultId: Uuid) {
        vaultRoles[vaultId] = nil
        lostVaultIds.remove(vaultId)
        persistVaults()
    }

    private func persistVaults() {
        defaults.set(vaultRoles.mapValues(\.rawValue), forKey: Self.vaultRolesKey)
        defaults.set(lostVaultIds.sorted(), forKey: Self.lostVaultsKey)
    }

    private static func decodeRoles(_ stored: [String: Any]?) -> [Uuid: MemberRole] {
        (stored ?? [:]).compactMapValues { ($0 as? String).flatMap(MemberRole.init(rawValue:)) }
    }
}
