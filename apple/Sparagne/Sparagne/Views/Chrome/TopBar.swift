import SwiftUI

/// The window's own top bar, in place of the system title bar and toolbar.
struct TopBar: View {
    @Bindable var store: AppStore
    let engine: SyncEngine?
    /// Owned by the window, which focuses it on ⌘F: on the tab's own field,
    /// or on the Mastro's after switching to it from the Riepilogo.
    @FocusState.Binding var searchFocused: Bool
    /// Requests one of `MainWindow`'s sheets.
    let present: (MainWindow.SheetKind) -> Void

    /// The traffic lights are hidden in full screen, and so is their room.
    @State private var isFullScreen = false

    /// The month is what the Riepilogo and the Mastro read; the Ricorrenze
    /// and the Setup tabs are not about one.
    private var showsMonth: Bool { store.tab == .summary || store.tab == .ledger }

    var body: some View {
        HStack(spacing: 8) {
            VaultSelector(store: store, engine: engine, present: present)
            if let file = LaunchOptions.database {
                DemoBadge(file: file)
            }
            if showsMonth {
                Rectangle()
                    .fill(Ink.line2)
                    .frame(width: 1, height: 16)
                    .accessibilityHidden(true)
                MonthStepper(store: store)
            }
            Spacer(minLength: 12)
            switch store.tab {
            case .ledger:
                SearchField(text: $store.searchText, prompt: String(localized: "Search the month"), focused: $searchFocused)
            case .recurring:
                SearchField(
                    text: $store.tabFilter,
                    prompt: String(localized: "Search recurring entries"),
                    focused: $searchFocused
                )
            case .setup:
                SearchField(
                    text: $store.tabFilter,
                    prompt: String(localized: "Search Setup"),
                    focused: $searchFocused
                )
            case .summary, .allocation:
                EmptyView()
            }
            // The pill leads to the Ricorrenze tab, where the periods are
            // decided: on that tab they are already on screen.
            if store.tab != .recurring {
                DuePill(store: store)
            }
            addButton
            SyncPill(engine: engine, present: present)
        }
        .padding(.leading, isFullScreen ? 14 : 78)
        .padding(.trailing, 12)
        .frame(maxWidth: .infinity)
        .frame(height: Metrics.topBar)
        .background { background }
        .overlay(alignment: .bottom) { Hairline() }
    }

    /// What the tab on screen adds: a row on the Riepilogo and the Mastro, a
    /// template on the Ricorrenze, a one-off move on the Riparto; the Setup
    /// tab adds in its tables' own empty lines. Nothing for an account that
    /// only reads the vault, which could not save what it typed.
    ///
    /// Add opens the quick-add line (⌘K), as File ▸ Quick Add does; the
    /// others are what the Vault menu and the palette ask for, with no key
    /// cap since their menu items have no shortcut (and ⌘K adds a row, not a
    /// template).
    @ViewBuilder
    private var addButton: some View {
        if store.canWrite {
            switch store.tab {
            case .summary, .ledger:
                TabActionButton(symbol: "plus", title: String(localized: "Add"), keyCap: "\u{2318}K") {
                    NotificationCenter.default.post(name: .focusQuickAdd, object: nil)
                }
            case .recurring:
                TabActionButton(symbol: "plus", title: String(localized: "New recurring entry")) {
                    store.requestNewRecurring()
                }
            case .allocation:
                TabActionButton(symbol: "arrow.left.arrow.right", title: String(localized: "Move between envelopes\u{2026}")) {
                    NotificationCenter.default.post(name: .moveBetweenEnvelopes, object: nil)
                }
            case .setup:
                EmptyView()
            }
        }
    }

    /// The ground, and over it the AppKit view that the bar's empty areas
    /// hit: it drags the window and centres the traffic lights.
    private var background: some View {
        ZStack {
            Ink.bg
            WindowChrome(barHeight: Metrics.topBar, buttonsLeading: 14) { full in
                // Not during the view update that may have called this.
                Task { isFullScreen = full }
            }
        }
    }
}

// MARK: - The month

/// ‹ month › in one bordered group, and Today when the month on screen is another.
struct MonthStepper: View {
    let store: AppStore

    private var isCurrent: Bool { store.month == MonthKey(Date()) }

    var body: some View {
        HStack(spacing: 8) {
            HStack(spacing: 0) {
                step("chevron.left", months: -1)
                Text(store.month.title())
                    .font(Face.ui(12.5, .semibold))
                    .foregroundStyle(Ink.text)
                    .lineLimit(1)
                    .frame(minWidth: 104)
                    // The page's title for VoiceOver: the month everything
                    // below is about.
                    .accessibilityAddTraits(.isHeader)
                step("chevron.right", months: 1)
            }
            .frame(height: Metrics.control)
            .background(Ink.card, in: shape)
            .overlay(shape.strokeBorder(Ink.line2, lineWidth: 1))

            if !isCurrent {
                Button(String(localized: "Today")) {
                    store.month = MonthKey(Date())
                }
                .buttonStyle(.chrome(.ghost))
            }
        }
    }

    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: 7) }

    private func step(_ symbol: String, months: Int) -> some View {
        Button {
            store.month = store.month.adding(months: months)
        } label: {
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(Ink.text2)
                .frame(width: 24, height: 24)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        // The chevron alone reads as "go back" / "forward"; the menu items'
        // names say what they step.
        .accessibilityLabel(months < 0 ? String(localized: "Previous Month") : String(localized: "Next Month"))
    }
}

// MARK: - Search

/// The tab's search, in the bar so it stays put while the filters below change; esc clears it.
struct SearchField: View {
    @Binding var text: String
    let prompt: String
    @FocusState.Binding var focused: Bool

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 11))
                .foregroundStyle(Ink.text3)
                .accessibilityHidden(true)
            TextField(
                text: $text,
                prompt: Text(prompt).foregroundStyle(Ink.text3)
            ) {
                Text(prompt)
            }
            .textFieldStyle(.plain)
            .font(Face.row)
            .foregroundStyle(Ink.text)
            .focused($focused)
            // esc clears and drops focus; ⌘F (the menu) focuses it again.
            .onExitCommand {
                text = ""
                focused = false
            }
            // The menu item says the shortcut to VoiceOver already.
            KeyCap(text: "\u{2318}F")
                .accessibilityHidden(true)
        }
        .padding(.leading, 9)
        .padding(.trailing, 4)
        .frame(minWidth: 170, idealWidth: 280, maxWidth: 280)
        .frame(height: Metrics.control)
        .background(Ink.card, in: shape)
        .overlay(shape.strokeBorder(focused ? Ink.accent.opacity(0.6) : Ink.line2, lineWidth: 1))
    }

    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: 7) }
}

// MARK: - Due and add

/// "3 da confermare", for an account that may confirm them: it opens the Ricorrenze tab.
struct DuePill: View {
    let store: AppStore

    var body: some View {
        let count = store.actionableDueCount
        if count > 0 {
            Button {
                store.tab = .recurring
            } label: {
                HStack(spacing: 6) {
                    StatusDot(color: Ink.accent)
                        .accessibilityHidden(true)
                    Text(CountText.toConfirm(count))
                }
            }
            .buttonStyle(.pill(.accent))
        }
    }
}

/// What the tab on screen adds, at the trailing end of the bar.
private struct TabActionButton: View {
    let symbol: String
    let title: String
    var keyCap: String?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: symbol)
                    .font(.system(size: 10, weight: .semibold))
                    .accessibilityHidden(true)
                Text(title)
                if let keyCap {
                    // The menu item says the shortcut to VoiceOver already.
                    KeyCap(text: keyCap)
                        .accessibilityHidden(true)
                }
            }
        }
        .buttonStyle(.chrome())
    }
}
