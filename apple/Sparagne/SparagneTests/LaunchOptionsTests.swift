import Foundation
import Testing

@testable import Sparagne

/// `-SparagneDatabase`: which file a launch opens.
struct LaunchOptionsTests {
    @Test("The option names the other database; missing or blank, the real one opens")
    func database() {
        #expect(LaunchOptions.database(in: ["SparagneDatabase": "demo.sqlite"]) == "demo.sqlite")
        #expect(LaunchOptions.database(in: ["SparagneDatabase": "  "]) == nil)
        #expect(LaunchOptions.database(in: ["SparagneDatabase": 1]) == nil)
        #expect(LaunchOptions.database(in: [:]) == nil)
    }

    @Test("A name lands next to the real database, a path is taken as it is")
    func url() throws {
        let real = try CoreActor.databaseURL()
        let demo = try CoreActor.databaseURL(named: "demo.sqlite")
        #expect(real.lastPathComponent == CoreActor.databaseName)
        #expect(demo.deletingLastPathComponent() == real.deletingLastPathComponent())
        #expect(demo.lastPathComponent == "demo.sqlite")
        #expect(try CoreActor.databaseURL(named: "/tmp/demo.sqlite").path(percentEncoded: false) == "/tmp/demo.sqlite")
        let home = FileManager.default.homeDirectoryForCurrentUser.path(percentEncoded: false)
        let tilde = try CoreActor.databaseURL(named: "~/demo.sqlite").path(percentEncoded: false)
        #expect(tilde == (home as NSString).appendingPathComponent("demo.sqlite"))
    }
}

/// Who signs the rows of a launch on another database.
@MainActor
struct LoggedOutAuthorTests {
    @Test("The name chosen in Settings, else the macOS account, whatever the account")
    func author() throws {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.launch.\(UUID().uuidString)"))
        #expect(AccountStore.loggedOutAuthor(defaults: defaults) == NSUserName())
        defaults.set("  ", forKey: AccountStore.localAuthorKey)
        #expect(AccountStore.loggedOutAuthor(defaults: defaults) == NSUserName())
        defaults.set(" elisa ", forKey: AccountStore.localAuthorKey)
        defaults.set("matteo", forKey: AccountStore.usernameKey)
        #expect(AccountStore.loggedOutAuthor(defaults: defaults) == "elisa")
    }
}
