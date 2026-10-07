import SwiftUI
import SparagneCore

/// The line above the grid: PERSONA and direction (`docs/v2/UI.md` §2.1).
/// The month and the search are in the top bar (`TopBar`), the row count in
/// the tab bar's status line (`LedgerStatusLine`).
struct LedgerHeader: View {
    @Bindable var store: AppStore

    var body: some View {
        HStack(spacing: 14) {
            if store.authors.count > 1 {
                SegmentedStrip(
                    options: [nil] + store.authors.map(Optional.some),
                    selection: $store.person,
                    label: { $0 ?? String(localized: "All") }
                )
            }

            SegmentedStrip(
                options: LedgerDirection.allCases,
                selection: $store.direction,
                label: { $0.label }
            )

            Spacer(minLength: 8)
        }
        .padding(.horizontal, Metrics.gutter)
        .frame(height: 40)
        .background(Ink.bg)
    }
}

/// The flat segmented control of the mockups: the active segment is filled
/// with the accent, the others are bare. `Picker(.segmented)` cannot be made
/// to look like this, so it is drawn by hand.
struct SegmentedStrip<Option: Hashable>: View {
    let options: [Option]
    @Binding var selection: Option
    let label: (Option) -> String

    var body: some View {
        HStack(spacing: 1) {
            ForEach(options, id: \.self) { option in
                let active = option == selection
                Button {
                    selection = option
                } label: {
                    Text(label(option))
                        .font(Face.label)
                        .foregroundStyle(active ? Ink.bg : Ink.dim)
                        .padding(.horizontal, 10)
                        .frame(height: 22)
                        .background(active ? Ink.accent : Ink.panel)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                // One button per segment for VoiceOver, saying which one is
                // on: the fill that says it on screen is not read.
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(label(option))
                .accessibilityAddTraits(active ? [.isButton, .isSelected] : .isButton)
                .accessibilityAction { selection = option }
            }
        }
        .overlay(Rectangle().strokeBorder(Ink.line, lineWidth: 1))
    }
}
