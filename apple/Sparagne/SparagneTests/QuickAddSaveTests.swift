import Foundation
import Testing

@testable import Sparagne

/// When the ⌘K panel closes on a save it did not start itself
/// (`QuickAddSave.savedLine`): only when that save took its line.
struct QuickAddSaveTests {
    private static let before = Date(timeIntervalSinceReferenceDate: 0)
    private static let after = Date(timeIntervalSinceReferenceDate: 60)

    @Test("A save that empties the line is the line saved, as when the alert's candidate resubmits it")
    func lineSaved() {
        let typed = QuickAddSave(savedAt: Self.before, line: "12 pizza #pi")
        #expect(QuickAddSave(savedAt: Self.after, line: "").savedLine(after: typed))
        #expect(QuickAddSave(savedAt: Self.after, line: "").savedLine(after: QuickAddSave(savedAt: nil, line: "12 pizza")))
    }

    @Test("A pending void flushing saves too, and closes neither an empty panel nor one with a line in it")
    func someoneElsesSave() {
        let empty = QuickAddSave(savedAt: Self.before, line: "")
        #expect(!QuickAddSave(savedAt: Self.after, line: "").savedLine(after: empty))
        #expect(!QuickAddSave(savedAt: Self.after, line: "").savedLine(after: QuickAddSave(savedAt: nil, line: "")))
        let typed = QuickAddSave(savedAt: Self.before, line: "12 pizza")
        #expect(!QuickAddSave(savedAt: Self.after, line: "12 pizza").savedLine(after: typed))
    }

    @Test("Clearing the line by hand saves nothing, and keeps the panel open")
    func clearedByHand() {
        let typed = QuickAddSave(savedAt: Self.before, line: "12 pizza")
        #expect(!QuickAddSave(savedAt: Self.before, line: "").savedLine(after: typed))
    }
}
