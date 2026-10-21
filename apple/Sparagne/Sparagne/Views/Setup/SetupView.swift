import SwiftUI
import SparagneCore

/// The Setup tab: the vault, its wallets and envelopes on the left, its categories on the right.
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
                columns: Array(repeating: GridItem(.flexible(), spacing: 10, alignment: .top), count: 2),
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
