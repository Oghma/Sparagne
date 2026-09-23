import SwiftUI
import SparagneCore

/// Changing the account's password (`POST /auth/password`), opened from
/// Settings. A stub for now.
struct ChangePasswordSheet: View {
    let engine: SyncEngine
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack {
            Button(String(localized: "Cancel")) { dismiss() }
        }
        .padding()
    }
}
