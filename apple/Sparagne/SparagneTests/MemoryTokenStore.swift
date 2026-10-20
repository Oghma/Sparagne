import Foundation

@testable import Sparagne

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
