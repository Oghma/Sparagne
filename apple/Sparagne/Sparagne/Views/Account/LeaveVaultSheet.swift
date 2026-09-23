import SwiftUI
import SparagneCore

/// A member leaving a shared vault: the membership goes on the server, the
/// local copy goes with `forgetVault`. A stub for now.
struct LeaveVaultSheet: View {
    let engine: SyncEngine
    let vault: VaultView
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack {
            Button(String(localized: "Cancel")) { dismiss() }
        }
        .padding()
    }
}
