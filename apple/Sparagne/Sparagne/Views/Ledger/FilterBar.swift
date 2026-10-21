import SwiftUI
import SparagneCore

/// The line above the grid: which rows the sheet
/// shows. Direction, PERSONA, then three chips for what is off by default:
/// transfers, voided rows and the WALLET column. The month and the search are
/// in the top bar (`TopBar`), the figures in the tab bar's status line
/// (`SheetStatus`).
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

            PersonSegments(store: store)

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
