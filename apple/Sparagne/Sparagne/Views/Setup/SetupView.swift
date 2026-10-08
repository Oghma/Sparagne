import SwiftUI
import SparagneCore

/// The SETUP view: the vault's card, its wallets and
/// its envelopes in the left column, its categories in the right one, as
/// cards on the sheet. Two columns when the window has room for both at 520
/// pt, one under the other when it does not.
struct SetupView: View {
    let store: AppStore
    /// `nil` before the database is open and in a demo database: the vault's
    /// members and its sharing need an account.
    let engine: SyncEngine?
    /// Opens one of the window's sheets, e.g. sharing the vault.
    let present: (MainWindow.SheetKind) -> Void

    var body: some View {
        ScrollView {
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 520), spacing: 10, alignment: .top)],
                alignment: .leading,
                spacing: 10
            ) {
                VStack(spacing: 10) {
                    VaultCard(store: store, engine: engine, present: present)
                    WalletTable(store: store)
                    EnvelopeTable(store: store)
                }
                CategoryTable(store: store)
            }
            .padding(12)
        }
        .scrollBounceBehavior(.basedOnSize)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Ink.sheet)
        .task(id: store.currentVault?.id) {
            await store.loadCategoryManagement()
        }
        // The one trigger of the usage column: it follows the list, so the
        // first load, a vault switch (the list is cleared and then filled), a
        // merge and a new category each read it once. The empty list a switch
        // leaves in between has nothing to count, and a rename keeps the ids.
        .task(id: store.windowCategories.map(\.id)) {
            guard !store.windowCategories.isEmpty else { return }
            await store.loadCategoryUsage()
        }
    }
}
