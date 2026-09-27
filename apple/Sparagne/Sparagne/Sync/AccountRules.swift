import Foundation

/// The server's rules for an account (`docs/v2/SYNC.md` §3, register), checked
/// in the form before anything is sent, so a refusal reads as a hint under
/// the field instead of the server's English after a round trip.
enum AccountRules {
    static let usernameLength = 3...32
    static let minimumPasswordLength = 8

    /// What the server does to every username it receives: trimmed and
    /// lowercased, so `" Alice "` and `alice` are one account.
    static func normalize(username: String) -> String {
        username.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    /// Whether a new account may have this (already normalized) name: 3 to
    /// 32 of `[a-z0-9_.-]`, counted in Unicode scalars as the server counts
    /// chars.
    static func isValidUsername(_ username: String) -> Bool {
        usernameLength.contains(username.unicodeScalars.count)
            && username.unicodeScalars.allSatisfy { scalar in
                ("a"..."z").contains(scalar) || ("0"..."9").contains(scalar) || "_.-".unicodeScalars.contains(scalar)
            }
    }

    /// Whether a new account, or a new password, may be this.
    static func isValidPassword(_ password: String) -> Bool {
        password.unicodeScalars.count >= minimumPasswordLength
    }
}
