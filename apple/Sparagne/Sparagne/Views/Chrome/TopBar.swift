import SwiftUI

/// The window's own top bar, in place of the system title bar and toolbar
/// (`docs/v2/UI.md` §2): the vault, the month, then on the trailing side the
/// search, what is due, the add button and the sync state. The scene hides
/// the title bar (`.windowStyle(.hiddenTitleBar)`); the traffic lights sit at
/// the bar's leading end, centred by `WindowChrome`, which also makes the
/// bar's empty areas drag the window.
struct TopBar: View {
    @Bindable var store: AppStore
    let engine: SyncEngine?
    /// Owned by the window, which focuses it on ⌘F after switching to the
    /// Mastro tab.
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
            if store.tab == .ledger {
                MonthSearchField(store: store, focused: $searchFocused)
            }
            DuePill(store: store)
            AddButton()
            SyncPill(engine: engine, present: present)
        }
        .padding(.leading, isFullScreen ? 14 : 78)
        .padding(.trailing, 12)
        .frame(maxWidth: .infinity)
        .frame(height: Metrics.topBar)
        .background { background }
        .overlay(alignment: .bottom) { Hairline() }
    }

    /// The ground, and over it the AppKit view that the bar's empty areas
    /// hit: it drags the window and centres the traffic lights.
    private var background: some View {
        ZStack {
            Ink.bg
            WindowChrome(barHeight: Metrics.topBar, buttonsLeading: 14) { full in
                // Not during the view update that may have called this.
                Task { @MainActor in isFullScreen = full }
            }
        }
    }
}

// MARK: - The month

/// ‹ month › in one bordered group, and Today when the month on screen is
/// another (`docs/v2/UI.md` §2.1). ⌥← and ⌥→ step it too, from the Ledger
/// menu.
struct MonthStepper: View {
    @Bindable var store: AppStore

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

/// The Mastro's search, in the bar so it stays put while the filters below
/// change (`docs/v2/UI.md` §2.1). Bound to `store.searchText`; the window
/// debounces the reload. esc clears it and gives the focus back.
struct MonthSearchField: View {
    @Bindable var store: AppStore
    @FocusState.Binding var focused: Bool

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 11))
                .foregroundStyle(Ink.text3)
                .accessibilityHidden(true)
            TextField(
                text: $store.searchText,
                prompt: Text(String(localized: "Search the month")).foregroundStyle(Ink.text3)
            ) {
                Text(String(localized: "Search the month"))
            }
            .textFieldStyle(.plain)
            .font(Face.ui(12))
            .foregroundStyle(Ink.text)
            .focused($focused)
            // esc clears and drops focus; ⌘F (the menu) focuses it again.
            .onExitCommand {
                store.searchText = ""
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

/// "3 da confermare": the recurring periods waiting for a decision
/// (`docs/v2/UI.md` §2.5), the same count the Ricorrenze tab shows. Nothing
/// when none is due, or when the account only reads the vault and could not
/// confirm one anyway. Opens the periods through `.reviewDueRecurring`, so
/// the window decides in one place where they are decided.
struct DuePill: View {
    let store: AppStore

    var body: some View {
        let count = store.actionableDueCount
        if count > 0 {
            Button {
                NotificationCenter.default.post(name: .reviewDueRecurring, object: nil)
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

/// Opens the quick-add line (⌘K), the same as File ▸ Quick Add.
struct AddButton: View {
    var body: some View {
        Button {
            NotificationCenter.default.post(name: .focusQuickAdd, object: nil)
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "plus")
                    .font(.system(size: 10, weight: .semibold))
                    .accessibilityHidden(true)
                Text(String(localized: "Add"))
                KeyCap(text: "\u{2318}K")
                    .accessibilityHidden(true)
            }
        }
        .buttonStyle(.chrome())
    }
}
