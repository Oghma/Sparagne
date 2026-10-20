import Foundation

/// What the command line changes about one launch, read from the arguments alone so it lasts that launch.
enum LaunchOptions {
    static let databaseKey = "SparagneDatabase"
    static let tabKey = "SparagneTab"
    static let sheetKey = "SparagneSheet"

    /// The launch's own arguments, never the saved preferences.
    private static var arguments: [String: Any] {
        UserDefaults.standard.volatileDomain(forName: UserDefaults.argumentDomain)
    }

    /// `-SparagneDatabase demo.sqlite`: another database in the app's folder,
    /// never synced, its rows signed by people who are not the account; a value
    /// starting with `/` or `~` is a path, which a signed build's sandbox
    /// refuses. `nil` for the real one.
    static var database: String? { database(in: arguments) }

    static func database(in arguments: [String: Any]) -> String? {
        guard let value = arguments[databaseKey] as? String else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// `-SparagneTab ledger`: the tab to open on (`summary`, `ledger`,
    /// `recurring`, `allocation`, `setup`), `nil` for the usual one.
    static var tab: LedgerTab? { tab(in: arguments) }

    static func tab(in arguments: [String: Any]) -> LedgerTab? {
        guard let value = arguments[tabKey] as? String else { return nil }
        return LedgerTab(rawValue: value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased())
    }

    /// The sheet to open once the vault is on screen, `nil` for none:
    /// `vault`, `renameVault`, `deleteVault`, `leaveVault`, `share`, `manage`,
    /// `importStatement`, `move`, `rejected` or `settings`,
    /// matched without regard to case. The ones about sharing need an
    /// account, so a demo database opens nothing for them.
    static var sheet: String? { sheet(in: arguments) }

    static func sheet(in arguments: [String: Any]) -> String? {
        guard let value = arguments[sheetKey] as? String else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return trimmed.isEmpty ? nil : trimmed
    }
}
