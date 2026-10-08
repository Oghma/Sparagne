import Foundation

/// The local filter of the Ricorrenze and Setup tables, typed in the top
/// bar's search field (`AppStore.tabFilter`). Pure, so what matches is
/// tested without a view.
enum TableFilter {
    /// Whether any of `fields` contains `query`, ignoring case and accents
    /// ("caffe" finds "Caffè"). A blank query matches everything.
    static func matches(_ query: String, _ fields: [String]) -> Bool {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !needle.isEmpty else { return true }
        return fields.contains { $0.range(of: needle, options: [.caseInsensitive, .diacriticInsensitive]) != nil }
    }
}
