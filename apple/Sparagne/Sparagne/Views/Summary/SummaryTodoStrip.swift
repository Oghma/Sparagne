import SwiftUI

/// The strip of things to do under the RIEPILOGO's heading: one chip per
/// item, a click opens the tab that deals with it. Not drawn when empty.
struct SummaryTodoStrip: View {
    let items: [SummaryTodoItem]
    let store: AppStore

    var body: some View {
        // A stub for now.
        EmptyView()
    }
}
