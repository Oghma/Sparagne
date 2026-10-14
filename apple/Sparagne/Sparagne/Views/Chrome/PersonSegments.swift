import SwiftUI

/// The PERSONA segmented control, shared by the Mastro's filter bar and the
/// RIEPILOGO's heading: one filter for the whole window (`store.person`),
/// so a person picked on either tab is the one the other reads.
///
/// Nothing with one person: there is nobody to filter out. The persons of
/// the rows, not the vault's members: a member with no row would filter the
/// window down to nothing.
struct PersonSegments: View {
    @Bindable var store: AppStore

    var body: some View {
        if store.peopleInRows.count > 1 {
            SegmentedStrip(
                options: [nil] + store.peopleInRows.map(Optional.some),
                selection: $store.person,
                label: { $0 ?? String(localized: "Everyone") }
            )
            .accessibilityElement(children: .contain)
            .accessibilityLabel(String(localized: "Person"))
        }
    }
}
