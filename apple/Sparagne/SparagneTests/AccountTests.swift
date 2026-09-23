import Foundation
import Security
import SparagneCore
import Testing

@testable import Sparagne

/// A token store that refuses every save, the way the Keychain can refuse an
/// ad-hoc signed build.
private final class RefusingTokenStore: TokenStore, @unchecked Sendable {
    func token(for account: String) -> String? { nil }

    func save(_ token: String, for account: String) throws {
        throw KeychainError(status: errSecInteractionNotAllowed)
    }

    func delete(for account: String) {}
}

/// A transport that answers every request with the same response.
private struct CannedTransport: SyncTransport {
    let response: SyncResponse

    func send(_ request: SyncRequest) async throws -> SyncResponse { response }
}

/// The account side of the app: where the session lives, when it ends, and
/// what the forms check before the server does.
@MainActor
struct AccountTests {
    private static func defaults() throws -> UserDefaults {
        try #require(UserDefaults(suiteName: "sparagne.account.\(UUID().uuidString)"))
    }

    // MARK: - Session

    @Test("A session whose token has expired counts as logged out at launch, and keeps the name")
    func expiredAtLaunch() throws {
        let defaults = try Self.defaults()
        let tokens = MemoryTokenStore()
        let now = Date()
        let first = AccountStore(defaults: defaults, tokens: tokens, now: now)
        first.signIn(username: "alice", token: "t-1", expiresAt: now.addingTimeInterval(3_600))

        let later = AccountStore(defaults: defaults, tokens: tokens, now: now.addingTimeInterval(1_800))
        #expect(later.isLoggedIn)
        let expiry = try #require(later.expiresAt)
        #expect(abs(expiry.timeIntervalSince(now) - 3_600) < 0.001)
        #expect(!later.sessionExpired)

        let expired = AccountStore(defaults: defaults, tokens: tokens, now: now.addingTimeInterval(7_200))
        #expect(!expired.isLoggedIn)
        #expect(expired.username == nil)
        #expect(expired.lastUsername == "alice")
        #expect(expired.sessionExpired)
        // The dead token is not kept either.
        #expect(tokens.token(for: "alice") == nil)
    }

    @Test("Logging out is not an expiry, and forgets when the token would have ended")
    func logOutIsNotExpiry() throws {
        let defaults = try Self.defaults()
        let account = AccountStore(defaults: defaults, tokens: MemoryTokenStore())
        account.signIn(username: "alice", token: "t-1", expiresAt: Date().addingTimeInterval(3_600))
        account.signOut()

        #expect(!account.isLoggedIn)
        #expect(!account.sessionExpired)
        #expect(account.expiresAt == nil)
        #expect(defaults.object(forKey: AccountStore.expiresAtKey) == nil)
    }

    @Test("A token the Keychain refuses keeps the session and is reported")
    func keychainRefusal() throws {
        let account = AccountStore(defaults: try Self.defaults(), tokens: RefusingTokenStore())

        account.signIn(username: "alice", token: "t-1", expiresAt: nil)

        #expect(account.isLoggedIn)
        #expect(account.token == "t-1")
        let failure = try #require(account.tokenSaveFailure)
        #expect(failure.contains("\(errSecInteractionNotAllowed)"))

        account.signOut()
        #expect(account.tokenSaveFailure == nil)
    }

    @Test("The token never lands in the preferences")
    func tokenStaysOutOfDefaults() throws {
        let defaults = try Self.defaults()
        let account = AccountStore(defaults: defaults, tokens: MemoryTokenStore())
        account.signIn(username: "alice", token: "secret-token-value", expiresAt: nil)

        for value in defaults.dictionaryRepresentation().values {
            #expect(!"\(value)".contains("secret-token-value"))
        }
    }

    @Test("Roles and lost vaults survive a relaunch; another account starts from none")
    func rolesArePersisted() throws {
        let defaults = try Self.defaults()
        let account = AccountStore(defaults: defaults, tokens: MemoryTokenStore())
        account.signIn(username: "alice", token: "t-1", expiresAt: nil)
        account.adoptRoles(["v1": .viewer, "v2": .owner])
        account.markLost("v3")

        let relaunched = AccountStore(defaults: defaults, tokens: MemoryTokenStore())
        #expect(relaunched.vaultRoles == ["v1": .viewer, "v2": .owner])
        #expect(relaunched.lostVaultIds == ["v3"])

        // Listed again, a lost vault is no longer lost.
        relaunched.adoptRoles(["v3": .editor])
        #expect(relaunched.lostVaultIds.isEmpty)

        relaunched.signIn(username: "bob", token: "t-2", expiresAt: nil)
        #expect(relaunched.vaultRoles.isEmpty)
    }

    // MARK: - Rate limits

    @Test("A 429 carries Retry-After into the error and reads in minutes, rounded up")
    func rateLimitMessage() async throws {
        let body = Data(#"{"error":{"code":"too_many_requests","message":"too many attempts, retry in 61 seconds"}}"#.utf8)
        let api = ServerAPI(
            transport: CannedTransport(response: SyncResponse(status: 429, body: body, headers: ["retry-after": "61"]))
        )

        do {
            _ = try await api.login(username: "alice", password: "supersecret")
            Issue.record("the login should have been refused")
        } catch let error as ServerError {
            #expect(error.status == 429)
            #expect(error.retryAfter == 61)
            #expect(error.isRateLimited)
            #expect(error.summary == String(localized: "Too many attempts. Try again in \(2) minutes."))
            #expect(error.detail == nil)
        }

        #expect(ErrorMessages.tooManyAttempts(retryAfter: 60) == String(localized: "Too many attempts. Try again in \(1) minutes."))
        #expect(ErrorMessages.tooManyAttempts(retryAfter: 1) == String(localized: "Too many attempts. Try again in \(1) minutes."))
        #expect(ErrorMessages.tooManyAttempts(retryAfter: nil) == ErrorMessages.summary(for: "too_many_requests"))
    }

    @Test("Header names are case-insensitive")
    func headerLookup() {
        let response = SyncResponse(status: 429, headers: ["Retry-After": "30"])
        #expect(response.header("retry-after") == "30")
        #expect(response.header("RETRY-AFTER") == "30")
        #expect(response.header("Content-Type") == nil)
    }

    @Test("A 401 on a session reads as an expired session, on a login as wrong credentials")
    func unauthorizedWording() {
        #expect(ErrorMessages.summary(for: "unauthorized") == ErrorMessages.sessionExpired)
        #expect(ErrorMessages.wrongCredentials != ErrorMessages.sessionExpired)
    }

    // MARK: - The register form

    @Test("Usernames are trimmed and lowercased, then held to the server's rules")
    func usernameRules() {
        #expect(AccountRules.normalize(username: "  Alice.B_1 \n") == "alice.b_1")
        for good in ["abc", "alice", "a.b-c_9", String(repeating: "x", count: 32)] {
            #expect(AccountRules.isValidUsername(good), "\(good)")
        }
        for bad in ["", "ab", String(repeating: "x", count: 33), "Alice", "al ice", "alice!", "àlice", "al@ce"] {
            #expect(!AccountRules.isValidUsername(bad), "\(bad)")
        }
    }

    @Test("Passwords need at least 8 characters")
    func passwordRules() {
        #expect(!AccountRules.isValidPassword(""))
        #expect(!AccountRules.isValidPassword("1234567"))
        #expect(AccountRules.isValidPassword("12345678"))
        #expect(AccountRules.isValidPassword("supersecret"))
    }
}
