import SwiftUI

/// The Setup tab's status line (`docs/v2/UI.md` §2.3): how many wallets,
/// envelopes and categories the vault has in use, archived ones left out.
struct SetupStatusLine: View {
    let store: AppStore

    var body: some View {
        StatusLine(items: [
            StatusItem(label: String(localized: "Wallets"), value: store.wallets.count.formatted(.number)),
            // Unallocated is the core's own envelope, not one the user made.
            StatusItem(
                label: String(localized: "Envelopes"),
                value: store.flows.filter { !$0.isUnallocated }.count.formatted(.number)
            ),
            StatusItem(label: String(localized: "Categories"), value: store.categories.count.formatted(.number)),
        ])
    }
}
