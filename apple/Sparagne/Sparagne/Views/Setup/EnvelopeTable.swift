import SwiftUI
import SparagneCore

/// The envelopes of the vault as an editable table (`docs/v2/UI.md` §2.3):
/// the last line adds one, every cell edits in place.
struct EnvelopeTable: View {
    @Bindable var store: AppStore

    var body: some View {
        // Placeholder until the envelope work lands.
        Panel { SectionLabel(text: String(localized: "Envelopes")) }
    }
}
