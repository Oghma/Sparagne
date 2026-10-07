import SwiftUI

/// The Ricorrenze tab's status line (`docs/v2/UI.md` §2.5): how many
/// templates run, and how many are archived. The templates are loaded by the
/// tab (`RecurringTab`), so the counts are of what it shows.
struct RecurringStatusLine: View {
    let store: AppStore

    var body: some View {
        let archived = store.recurringTemplates.filter(\.archived).count
        let active = store.recurringTemplates.count - archived
        StatusLine(items: [
            StatusItem(label: String(localized: "Active"), value: active.formatted(.number)),
            // Not "Archived", whose translation agrees with wallets and
            // envelopes; these are recurring entries.
            StatusItem(label: String(localized: "In archive"), value: archived.formatted(.number)),
        ])
    }
}
