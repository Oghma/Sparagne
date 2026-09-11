import SwiftUI
import SparagneCore

/// The window (`docs/v2/UI.md` §2): a month header, one of the three views,
/// and a status bar. No sidebar and no inspector: the grid is editable in
/// place, and everything that manages entities lives in the menus.
struct LedgerWindow: View {
    let store: AppStore
    let engine: SyncEngine?
    @Binding var sheet: MainWindow.SheetKind?

    @FocusState private var searchFocused: Bool
    @State private var showsQuickAdd = false
    /// Driven by the two toggles in the Ledger menu.
    @AppStorage("showVoided") private var showVoided = false
    @AppStorage("showTransfers") private var showTransfers = false

    var body: some View {
        VStack(spacing: 0) {
            RecurringBanner(store: store) { sheet = .recurring }
            LedgerHeader(store: store, searchFocused: $searchFocused, compact: store.tab != .ledger)
            Hairline()
            content
            Hairline()
            LedgerStatusBar(store: store)
        }
        .background(Ink.bg)
        .overlay(alignment: .top) {
            if showsQuickAdd {
                QuickAddOverlay(store: store, isPresented: $showsQuickAdd)
                    .padding(.top, 60)
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .focusQuickAdd)) { _ in
            showsQuickAdd = true
        }
        .onReceive(NotificationCenter.default.publisher(for: .focusSearch)) { _ in
            store.tab = .ledger
            searchFocused = true
        }
        .onReceive(NotificationCenter.default.publisher(for: .stepMonth)) { note in
            let months = note.object as? Int ?? 1
            store.month = store.month.adding(months: months)
        }
        .onAppear {
            store.showVoided = showVoided
            store.showTransfers = showTransfers
        }
        .onChange(of: showVoided) { _, new in store.showVoided = new }
        .onChange(of: showTransfers) { _, new in store.showTransfers = new }
        .task(id: store.searchText) {
            // Debounce: reload only once the field has been quiet for 300 ms.
            guard (try? await Task.sleep(for: .milliseconds(300))) != nil else { return }
            store.reload()
        }
    }

    @ViewBuilder
    private var content: some View {
        if let summary = store.summary {
            switch store.tab {
            case .ledger:
                HStack(spacing: 0) {
                    LedgerGrid(store: store)
                    Rectangle().fill(Ink.line).frame(width: 1)
                    SummaryPanel(summary: summary, store: store)
                }
            case .summary:
                ScrollView { SummaryView(summary: summary, store: store) }
            case .year:
                ScrollView { YearView(summary: summary, store: store) }
            }
        } else {
            VStack {
                Spacer()
                Text(String(localized: "No vault yet"))
                    .font(Face.row)
                    .foregroundStyle(Ink.dim)
                Spacer()
            }
            .frame(maxWidth: .infinity)
        }
    }
}

// MARK: - Recurring

/// The banner over the grid when a recurring period is waiting for a
/// decision (`DISTILLATO_V1.md` §2.3). One line, in the ledger's own ink.
struct RecurringBanner: View {
    let store: AppStore
    let onOpen: () -> Void

    private var count: Int {
        store.pendingRecurringItems.reduce(0) { $0 + $1.due.count }
    }

    var body: some View {
        if count > 0 {
            HStack(spacing: 10) {
                Text("\u{25CF}")
                    .font(Face.footnote)
                    .foregroundStyle(Ink.accent)
                Text("\(count) \(String(localized: "recurring entries are due"))")
                    .font(Face.row)
                    .foregroundStyle(Ink.text)
                Spacer()
                Button(String(localized: "Review")) { onOpen() }
                    .buttonStyle(.plain)
                    .font(Face.label)
                    .foregroundStyle(Ink.accent)
            }
            .padding(.horizontal, Metrics.gutter)
            .frame(height: 30)
            .background(Ink.panel)
            .overlay(alignment: .bottom) { Hairline() }
        }
    }
}

// MARK: - Quick add

/// ⌘K: the one-line grammar of `DISTILLATO_V1.md` §3.1, over the grid.
///
/// The grid covers the common case; this covers the fast case, where the
/// whole row is one line of text and the fingers never leave the keyboard.
struct QuickAddOverlay: View {
    @Bindable var store: AppStore
    @Binding var isPresented: Bool
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            TextField(
                String(localized: "-12.50 pizza #food @cash >groceries"),
                text: $store.quickAddText
            )
            .textFieldStyle(.plain)
            .font(Face.mono(14))
            .foregroundStyle(Ink.text)
            .focused($focused)
            .onSubmit {
                store.submit(quickAdd: store.quickAddText)
                isPresented = false
            }

            Text(preview.text)
                .font(Face.footnote)
                .foregroundStyle(preview.isError ? Ink.accent : Ink.dim)
                .lineLimit(1)
        }
        .padding(14)
        .frame(width: 520, alignment: .leading)
        .background(Ink.panel)
        .overlay(Rectangle().strokeBorder(Ink.accent, lineWidth: 1))
        .shadow(color: .black.opacity(0.5), radius: 20, y: 8)
        .onAppear { focused = true }
        .onExitCommand {
            store.quickAddText = ""
            isPresented = false
        }
    }

    private var preview: (text: String, isError: Bool) {
        let trimmed = store.quickAddText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return (" ", false) }
        switch store.preview(quickAdd: trimmed) {
        case .success(let parsed):
            return (QuickAddSummary.describe(parsed, currency: store.currency), false)
        case .failure(let error):
            return (error.message, true)
        }
    }
}
