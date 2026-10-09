import SwiftUI

/// The sheet tabs along the bottom of the window, as in a spreadsheet:
/// Riepilogo · Mastro · Ricorrenze · Riparto · Setup, plain words
/// with the active one underlined in the accent, and on the right the status
/// line of the sheet on screen. ⌘1 to ⌘5 select the same tabs from the View
/// menu.
struct SheetTabBar<Status: View>: View {
    let store: AppStore
    /// The active tab's own figures (`StatusLine`).
    @ViewBuilder let status: () -> Status

    var body: some View {
        HStack(spacing: 14) {
            ForEach(LedgerTab.allCases) { tab in
                let count = count(for: tab)
                SheetTab(tab: tab, isActive: tab == store.tab, count: count, spokenCount: spoken(tab, count)) {
                    store.tab = tab
                }
            }
            Spacer(minLength: 14)
            status()
        }
        .padding(.horizontal, 14)
        .frame(height: Metrics.tabBar)
        .frame(maxWidth: .infinity)
        .background(Ink.bg)
        .overlay(alignment: .top) { Hairline() }
    }

    /// What a tab carries beside its name: on Ricorrenze the periods to
    /// confirm, the same count as the top bar's pill; on Riparto one while
    /// a period waits to be shared out.
    private func count(for tab: LedgerTab) -> Int {
        switch tab {
        case .recurring: store.actionableDueCount
        case .allocation: store.actionableAllocationCount
        case .summary, .ledger, .setup: 0
        }
    }

    /// The count as VoiceOver reads it after the tab's name.
    private func spoken(_ tab: LedgerTab, _ count: Int) -> String {
        tab == .allocation ? AllocationText.toShareOut(count) : CountText.toConfirm(count)
    }
}

/// One tab: its name, the accent count when there is one, and a 2-point
/// accent underline when it is the sheet on screen.
private struct SheetTab: View {
    let tab: LedgerTab
    let isActive: Bool
    let count: Int
    /// The count in words, for VoiceOver.
    let spokenCount: String
    let select: () -> Void

    var body: some View {
        Button(action: select) {
            HStack(spacing: 5) {
                Text(tab.label)
                    .foregroundStyle(isActive ? Ink.text : Ink.text3)
                if count > 0 {
                    Text(count, format: .number)
                        .fontWeight(.semibold)
                        .foregroundStyle(Ink.accent)
                }
            }
            .font(Face.ui(12, .medium))
            // The bar's whole height, so the underline sits on its bottom
            // edge; the top hairline is drawn over the first point.
            .frame(height: Metrics.tabBar)
            .overlay(alignment: .bottom) {
                if isActive {
                    Rectangle().fill(Ink.accent).frame(height: 2)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(isActive ? [.isButton, .isSelected] : .isButton)
        .accessibilityAction(.default, select)
    }

    private var accessibilityLabel: String {
        count > 0 ? "\(tab.label), \(spokenCount)" : tab.label
    }
}
