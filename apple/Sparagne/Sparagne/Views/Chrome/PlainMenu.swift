import SwiftUI

extension View {
    /// A `Menu` that is only its label as drawn: the default menu style
    /// would swap the label for a system pop-up button, and the indicator
    /// would add a chevron the label may draw itself.
    func plainMenu() -> some View {
        menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
    }
}
