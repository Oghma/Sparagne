import SwiftUI
import SparagneCore

/// The SETUP view (`docs/v2/UI.md` §2.3): the vault's envelopes and its
/// categories as two editable tables side by side, drawn like the ledger.
struct SetupView: View {
    let store: AppStore

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            EnvelopeTable(store: store)
            CategoryTable(store: store)
        }
        .padding(10)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Ink.bg)
        .onAppear { store.loadCategoryManagement() }
        .onChange(of: store.currentVault?.id) { _, _ in store.loadCategoryManagement() }
    }
}
