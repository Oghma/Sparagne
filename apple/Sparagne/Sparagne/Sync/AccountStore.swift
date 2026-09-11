import Foundation
import Observation
import Security

/// Where the session token lives. The app keeps it in the Keychain; the tests
/// keep it in memory.
protocol TokenStore: Sendable {
    func token(for account: String) -> String?
    func save(_ token: String, for account: String)
    func delete(for account: String)
}

/// Generic-password items under the service `it.oghma.sparagne`, one per
/// username.
///
/// Every operation is best effort: a Keychain that refuses to store the token
/// (an unsigned build, a locked keychain) only costs the session on the next
/// launch, so failures are swallowed rather than surfaced.
struct KeychainTokenStore: TokenStore {
    static let service = "it.oghma.sparagne"

    func token(for account: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
            let data = item as? Data
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    func save(_ token: String, for account: String) {
        delete(for: account)
        let item: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account,
            kSecValueData as String: Data(token.utf8),
        ]
        _ = SecItemAdd(item as CFDictionary, nil)
    }

    func delete(for account: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account,
        ]
        _ = SecItemDelete(query as CFDictionary)
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
/// The URL and the username are preferences; the token is a secret and lives
/// in the Keychain. The username is also the `author` of every command
/// (`docs/v2/SYNC.md` §1), which is why `SyncEngine` relabels the outbox as
/// soon as a login succeeds.
@Observable
@MainActor
final class AccountStore {
    static let serverURLKey = "syncServerURL"
    static let usernameKey = "syncUsername"
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
    /// The last name that logged in, kept after a logout so the login form
    /// comes back filled in.
    private(set) var lastUsername: String

    init(defaults: UserDefaults = .standard, tokens: TokenStore = KeychainTokenStore()) {
        self.defaults = defaults
        self.tokens = tokens
        serverURLText = defaults.string(forKey: Self.serverURLKey) ?? Self.defaultServerURL
        localAuthor = defaults.string(forKey: Self.localAuthorKey) ?? ""
        let stored = defaults.string(forKey: Self.usernameKey)
        lastUsername = stored ?? ""
        let held = stored.flatMap { tokens.token(for: $0) }
        username = held == nil ? nil : stored
        token = held
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
        if let username { return username }
        let chosen = localAuthor.trimmingCharacters(in: .whitespacesAndNewlines)
        return chosen.isEmpty ? Self.systemAuthor : chosen
    }

    func signIn(username: String, token: String) {
        self.username = username
        self.token = token
        lastUsername = username
        defaults.set(username, forKey: Self.usernameKey)
        tokens.save(token, for: username)
    }

    func signOut() {
        if let username { tokens.delete(for: username) }
        token = nil
        username = nil
    }
}
