import SwiftUI
import SparagneCore

/// The recurring periods waiting for a decision: execute or skip each, or
/// all at once. A stub for now.
struct DueRecurringSheet: View {
    let store: AppStore
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack {
            Button(String(localized: "Done")) { dismiss() }
        }
        .padding()
    }
}
