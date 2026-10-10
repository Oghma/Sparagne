import Foundation

/// What the command line can change about one launch.
///
/// `-SparagneDatabase demo.sqlite` opens another database in the app's folder
/// instead of the real one, for trying the app on made-up rows: the
/// development fixture writes one (`core/examples/seed.rs`). A value that
/// starts with `/` or `~` is a path, which the sandbox of a signed build will
/// not let the app open. Nothing in that database is synced, so its rows,
/// signed by people who are not the account, never reach a server.
///
/// `-SparagneTab ledger` opens the window on one sheet tab (`summary`,
/// `ledger`, `recurring`, `allocation`, `setup`), and `-SparagneSheet manage` opens one of
/// the window's sheets or the Settings window, so a screenshot of either
/// needs no keyboard driving.
///
/// Read from the arguments alone, never from the saved preferences, so the
/// option lasts one launch and cannot leave the app on the wrong file.
enum LaunchOptions {
    static let databaseKey = "SparagneDatabase"
    static let tabKey = "SparagneTab"
    static let sheetKey = "SparagneSheet"

    /// The other database, `nil` for the real one.
    static var database: String? {
        database(in: UserDefaults.standard.volatileDomain(forName: UserDefaults.argumentDomain))
    }

    static func database(in arguments: [String: Any]) -> String? {
        guard let value = arguments[databaseKey] as? String else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// The tab to open on, `nil` for the usual one.
    static var tab: LedgerTab? {
        tab(in: UserDefaults.standard.volatileDomain(forName: UserDefaults.argumentDomain))
    }

    static func tab(in arguments: [String: Any]) -> LedgerTab? {
        guard let value = arguments[tabKey] as? String else { return nil }
        return LedgerTab(rawValue: value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased())
    }

    /// The sheet to open once the vault is on screen, `nil` for none:
    /// `vault`, `renameVault`, `deleteVault`, `leaveVault`, `share`, `manage`,
    /// `importStatement`, `rejected` or `settings`,
    /// matched without regard to case. The ones about sharing need an
    /// account, so a demo database opens nothing for them.
    static var sheet: String? {
        sheet(in: UserDefaults.standard.volatileDomain(forName: UserDefaults.argumentDomain))
    }

    static func sheet(in arguments: [String: Any]) -> String? {
        guard let value = arguments[sheetKey] as? String else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return trimmed.isEmpty ? nil : trimmed
    }
}
