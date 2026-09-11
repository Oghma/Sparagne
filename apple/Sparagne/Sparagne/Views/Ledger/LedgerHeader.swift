import SwiftUI
import SparagneCore

/// The line above the grid: month stepper, PERSONA, direction, search and the
/// row count (`docs/v2/UI.md` §2.1).
struct LedgerHeader: View {
    @Bindable var store: AppStore
    @FocusState.Binding var searchFocused: Bool
    /// The summary views are not a list, so they get the month and the person
    /// but neither the direction, the search nor the row count.
    var compact = false

    var body: some View {
        HStack(spacing: 14) {
            monthStepper

            Text("\(String(localized: "month"))=\(store.month.month)")
                .font(Face.footnote)
                .foregroundStyle(Ink.dim)

            if store.authors.count > 1 {
                SegmentedStrip(
                    options: [nil] + store.authors.map(Optional.some),
                    selection: $store.person,
                    label: { $0?.uppercased() ?? String(localized: "All").uppercased() }
                )
            }

            if !compact {
                SegmentedStrip(
                    options: LedgerDirection.allCases,
                    selection: $store.direction,
                    label: { $0.label.uppercased() }
                )

                search
            }

            Spacer(minLength: 8)

            if !compact {
                Text("\(store.rows.count) \(String(localized: "rows"))")
                    .font(Face.footnote)
                    .foregroundStyle(Ink.dim)
            }
        }
        .padding(.horizontal, Metrics.gutter)
        .frame(height: 40)
        .background(Ink.bg)
    }

    private var monthStepper: some View {
        HStack(spacing: 8) {
            stepButton("chevron.left", months: -1)
            Text(store.month.title())
                .font(Face.mono(13, .semibold))
                .foregroundStyle(Ink.text)
                .tracking(0.5)
                .frame(minWidth: 150, alignment: .leading)
            stepButton("chevron.right", months: 1)
        }
    }

    private func stepButton(_ symbol: String, months: Int) -> some View {
        Button {
            store.month = store.month.adding(months: months)
        } label: {
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(Ink.dim)
                .frame(width: 18, height: 18)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var search: some View {
        HStack(spacing: 6) {
            Text("/")
                .font(Face.row)
                .foregroundStyle(Ink.dim)
            TextField(String(localized: "search description…"), text: $store.searchText)
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .focused($searchFocused)
                // esc clears and drops focus; ⌘F (the menu) focuses it again.
                .onExitCommand {
                    store.searchText = ""
                    searchFocused = false
                }
        }
        .padding(.horizontal, 10)
        .frame(height: 24)
        .frame(maxWidth: 320)
        .background(Ink.panel)
        .overlay(Rectangle().strokeBorder(Ink.line, lineWidth: 1))
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
                        .tracking(0.6)
                        .foregroundStyle(active ? Ink.bg : Ink.dim)
                        .padding(.horizontal, 10)
                        .frame(height: 22)
                        .background(active ? Ink.accent : Ink.panel)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
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
                Text("\(store.rows.count) \(String(localized: "rows"))")
                    .font(Face.footnote)
                    .foregroundStyle(Ink.dim)
            }

            Spacer()

            if let savedAt = store.savedAt {
                Text("\(String(localized: "saved")) \(LedgerDate.clock(savedAt))")
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

    /// `mese=8 flow=uscite persona=tutti`, the mockup's own shorthand. The
    /// segment values are the localized labels, lowercased, not the enums'
    /// raw values, so the line reads in the user's language too.
    private var filters: String {
        let person = store.person ?? String(localized: "all")
        let view = store.tab == .ledger ? "" : "\(String(localized: "view"))=\(store.tab.label.lowercased()) "
        return "\(view)\(String(localized: "month"))=\(store.month.month) "
            + "\(String(localized: "flow"))=\(store.direction.label.lowercased()) "
            + "\(String(localized: "person"))=\(person)"
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
