import Foundation
import SparagneCore

/// The category hint's rules for a parsed quick-add line: when the ⌘K panel
/// asks the core for a category, and how ⇥ writes the one it suggests. The
/// parsed line itself is drawn as chips (`QuickAddTokens`).
///
/// Parsing itself belongs to the core (`parseQuickAdd`).
enum QuickAddSummary {
    /// The note worth asking the core a category for: an entry's, when the
    /// line has no `#category` of its own. `nil` for a transfer, which has no
    /// category, and for a line that already says one.
    static func noteWithoutCategory(_ parsed: QuickAdd) -> String? {
        guard case .entry(_, _, let note, let category, _, _, _, _) = parsed,
              category?.isEmpty ?? true,
              let note = note?.trimmingCharacters(in: .whitespacesAndNewlines),
              !note.isEmpty
        else { return nil }
        return note
    }

    /// The line with `#category` added, which is what ⇥ does to accept the
    /// hint: the line says it from then on, so the preview shows it and
    /// nothing is applied unseen. `nil` for a name `#` cannot carry, one with
    /// a space: the grammar would read the rest of it as note.
    static func accepting(_ category: String, into line: String) -> String? {
        guard !category.isEmpty, !category.contains(where: \.isWhitespace) else { return nil }
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        return "\(trimmed) #\(category)"
    }
}
