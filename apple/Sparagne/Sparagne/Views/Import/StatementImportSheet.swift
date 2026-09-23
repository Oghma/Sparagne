import SwiftUI
import SparagneCore

/// Importing a bank or card statement (`core::statement`): file, mapping,
/// preview, import. A stub for now.
struct StatementImportSheet: View {
    let store: AppStore
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack {
            Button(String(localized: "Done")) { dismiss() }
        }
        .padding()
    }
}
