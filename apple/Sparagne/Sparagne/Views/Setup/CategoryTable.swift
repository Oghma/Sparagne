import SwiftUI
import SparagneCore

/// The categories of the vault as an editable table (`docs/v2/UI.md` §2.3):
/// the last line adds one, names and aliases edit in place.
struct CategoryTable: View {
    @Bindable var store: AppStore

    var body: some View {
        // Placeholder until the category work lands.
        Panel { SectionLabel(text: String(localized: "Categories")) }
    }
}
