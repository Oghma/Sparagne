import SwiftUI
import SparagneCore

/// The SETUP view (`docs/v2/UI.md` §2.3): the vault's wallets above its
/// envelopes on the left, its categories on the right, all editable tables
/// drawn like the ledger.
struct SetupView: View {
    let store: AppStore

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            VStack(spacing: 10) {
                // As tall as its rows: the envelopes' scroll view gets the rest.
                WalletTable(store: store)
                    .fixedSize(horizontal: false, vertical: true)
                EnvelopeTable(store: store)
            }
            CategoryTable(store: store)
        }
        .padding(10)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Ink.bg)
        .task(id: store.currentVault?.id) { await store.loadCategoryManagement() }
    }
}
