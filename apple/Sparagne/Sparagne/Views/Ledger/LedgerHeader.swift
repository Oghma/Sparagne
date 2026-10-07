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

/// The bottom line of the window: the filters in force, the total of what is
/// on screen, and where the database is.
struct LedgerStatusBar: View {
    let store: AppStore

    var body: some View {
        HStack(spacing: 16) {
            Circle()
                .fill(store.pendingUndo == nil ? Ink.positive : Ink.accent)
                .frame(width: 6, height: 6)

            Text(filters)
                .font(Face.footnote)
                .foregroundStyle(Ink.dim)

            if store.tab == .ledger {
                HStack(spacing: 6) {
                    Text("\u{03A3}").font(Face.footnote).foregroundStyle(Ink.dim)
                    Text(LedgerMoney.amount(visibleTotal))
                        .font(Face.footnote)
                        .foregroundStyle(Ink.text)
                }
                Text(CountText.rows(store.rows.count))
                    .font(Face.footnote)
                    .foregroundStyle(Ink.dim)
            }

            Spacer()

            if let savedAt = store.savedAt {
                Text(String(localized: "saved \(LedgerDate.clock(savedAt))"))
                    .font(Face.footnote)
                    .foregroundStyle(Ink.dim)
            }
            Text(store.currentVault?.name ?? "—")
                .font(Face.footnote)
                .foregroundStyle(Ink.dim)
        }
        .padding(.horizontal, Metrics.gutter)
        .frame(height: 28)
        .background(Ink.bg)
    }

    /// `mese=8 flow=uscite persona=tutti`, the mockup's own shorthand, as one
    /// sentence of the catalog so a translation can name and order the keys.
    /// The segment values are the localized labels, lowercased, not the
    /// enums' raw values, so the line reads in the user's language too.
    private var filters: String {
        let month = store.month.month
        let flow = store.direction.label.lowercased()
        let person = store.person ?? String(localized: "all")
        guard store.tab != .ledger else {
            return String(localized: "month=\(month) flow=\(flow) person=\(person)")
        }
        let view = store.tab.label.lowercased()
        return String(localized: "view=\(view) month=\(month) flow=\(flow) person=\(person)")
    }

    /// The sum of the rows on screen, which is what the user is looking at:
    /// not the month's total, which the panel already shows. A refund sits in
    /// the USCITE list and has to come off it, or the sum reads higher than
    /// what was actually spent.
    private var visibleTotal: Int64 {
        store.rows.reduce(0) { total, row in
            switch row.kind {
            case .refund: total - row.absoluteAmount
            case .income, .expense: total + row.absoluteAmount
            case .transferWallet, .transferFlow: total
            }
        }
    }
}
