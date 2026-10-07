import SwiftUI
import SparagneCore

/// The line above the grid (`docs/v2/UI.md` §2.1): which rows the sheet
/// shows. Direction, PERSONA, then three chips for what is off by default:
/// transfers, voided rows and the WALLET column. The month and the search are
/// in the top bar (`TopBar`), the figures in the tab bar's status line
/// (`LedgerStatusLine`).
///
/// The chips write the store; the window keeps them in step with the View
/// menu's toggles through the shared preferences (`LedgerWindow`).
struct FilterBar: View {
    @Bindable var store: AppStore

    var body: some View {
        HStack(spacing: 8) {
            SegmentedStrip(
                options: LedgerDirection.allCases,
                selection: $store.direction,
                label: { $0.label }
            )
            .accessibilityElement(children: .contain)
            .accessibilityLabel(String(localized: "Direction"))

            // One author has nobody to filter out.
            if store.authors.count > 1 {
                SegmentedStrip(
                    options: [nil] + store.authors.map(Optional.some),
                    selection: $store.person,
                    label: { $0 ?? String(localized: "Everyone") }
                )
                .accessibilityElement(children: .contain)
                .accessibilityLabel(String(localized: "Person"))
            }

            Rectangle()
                .fill(Ink.line2)
                .frame(width: 1, height: 16)
                .accessibilityHidden(true)

            FilterChip(title: String(localized: "Transfers"), isOn: $store.showTransfers)
            FilterChip(title: String(localized: "Deleted"), isOn: $store.showVoided)
            FilterChip(title: String(localized: "Wallet column"), isOn: $store.showWalletColumn)

            Spacer(minLength: 0)
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 12)
        .overlay(alignment: .bottom) { Hairline() }
    }
}

/// The flat segmented control of the canvas (`.seg`): a 1-point outline on
/// the chrome's ground, the active segment lifted onto `hi` in full-strength
/// text. Not the accent: the accent marks what can be acted on, and every
/// segment can. `Picker(.segmented)` cannot be made to look like this, so it
/// is drawn by hand.
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
                        .font(Face.ui(11.5, .medium))
                        .foregroundStyle(active ? Ink.text : Ink.text2)
                        .lineLimit(1)
                        .padding(.horizontal, 8)
                        .frame(height: 18)
                        .background {
                            if active {
                                RoundedRectangle(cornerRadius: 4)
                                    .fill(Ink.hi)
                                    .shadow(color: .black.opacity(0.45), radius: 1, y: 1)
                            }
                        }
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
        .padding(1)
        .background(Ink.bg, in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Ink.line2, lineWidth: 1))
    }
}

/// A filter that is off by default (`.chip`): a dashed outline while off, a
/// solid one on the raised ground while on, so a sheet showing more than
/// usual says so at a glance.
struct FilterChip: View {
    let title: String
    @Binding var isOn: Bool

    var body: some View {
        Button {
            isOn.toggle()
        } label: {
            Text(title)
                .font(Face.ui(11.5))
                .foregroundStyle(isOn ? Ink.text : Ink.text3)
                .lineLimit(1)
                .padding(.horizontal, 9)
                .frame(height: 22)
                .background(Capsule().fill(isOn ? Ink.raised : Color.clear))
                .overlay(
                    Capsule().strokeBorder(
                        Ink.line2,
                        style: StrokeStyle(lineWidth: 1, dash: isOn ? [] : [3, 2])
                    )
                )
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
        .accessibilityAddTraits(isOn ? [.isToggle, .isSelected] : .isToggle)
    }
}
