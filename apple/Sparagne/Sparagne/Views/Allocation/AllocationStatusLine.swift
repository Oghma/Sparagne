import SwiftUI

/// The Riparto tab's status line.
///
/// A stub for now: only when the last change was saved.
struct AllocationStatusLine: View {
    let store: AppStore

    var body: some View {
        StatusLine(items: items)
    }

    private var items: [StatusItem] {
        guard let savedAt = store.savedAt else { return [] }
        return [StatusItem(label: String(localized: "saved at \(LedgerDate.clock(savedAt))"))]
    }
}
