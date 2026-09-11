import SwiftUI
import SparagneCore
import UniformTypeIdentifiers

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

    // MARK: CSV export (⌘E, `Support/LedgerCSV.swift`)
    @State private var showsCSVExporter = false
    @State private var exportDocument: CSVDocument?
    @State private var exportFileName = "export.csv"

    /// The debounce's own memory of what it last reloaded for, so the task
    /// re-running on the initial value (and on every fresh mount, e.g. a
    /// vault switch) does not fire a second, redundant `reload()`.
    @State private var lastSearched = ""

    var body: some View {
        VStack(spacing: 0) {
            RecurringBanner(store: store) { sheet = .recurring }
            // The setup tables are not about a month, so they get no header.
            if store.tab != .setup {
                LedgerHeader(store: store, searchFocused: $searchFocused, compact: store.tab != .ledger)
                Hairline()
            }
            content
            Hairline()
            LedgerStatusBar(store: store)
        }
        .background(Ink.bg)
        .overlay(alignment: .top) {
            if showsQuickAdd {
                // A full-window, transparent backdrop under the box: a click
                // outside dismisses it, the same as esc.
                ZStack(alignment: .top) {
                    Color.clear
                        .contentShape(Rectangle())
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .onTapGesture {
                            store.quickAddText = ""
                            showsQuickAdd = false
                        }
                    QuickAddOverlay(store: store, isPresented: $showsQuickAdd)
                        .padding(.top, 60)
                }
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .focusQuickAdd)) { _ in
            showsQuickAdd = true
        }
        .onReceive(NotificationCenter.default.publisher(for: .focusSearch)) { _ in
            // The search field is not in the hierarchy outside the ledger
            // tab (the header stays compact there), so the tab has to switch
            // and lay out before the field can take focus.
            store.tab = .ledger
            Task { @MainActor in
                await Task.yield()
                searchFocused = true
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .openSetup)) { _ in
            store.tab = .setup
        }
        .onReceive(NotificationCenter.default.publisher(for: .stepMonth)) { note in
            let months = note.object as? Int ?? 1
            store.month = store.month.adding(months: months)
        }
        .onReceive(NotificationCenter.default.publisher(for: .exportCSV)) { _ in
            exportDocument = CSVDocument(text: LedgerCSV.render(store.rows))
            exportFileName = LedgerCSV.fileName(
                vault: store.currentVault?.name ?? "",
                month: store.month,
                direction: store.direction
            )
            showsCSVExporter = true
        }
        .onAppear {
            store.showVoided = showVoided
            store.showTransfers = showTransfers
        }
        .onChange(of: showVoided) { _, new in store.showVoided = new }
        .onChange(of: showTransfers) { _, new in store.showTransfers = new }
        .task(id: store.searchText) {
            // Debounce: reload only once the field has been quiet for 300 ms,
            // and only if it actually changed since the last reload (the task
            // also runs for the value the view already had, e.g. on a fresh
            // mount after a vault switch).
            guard store.searchText != lastSearched else { return }
            guard (try? await Task.sleep(for: .milliseconds(300))) != nil else { return }
            lastSearched = store.searchText
            store.reload()
        }
        .fileExporter(
            isPresented: $showsCSVExporter,
            document: exportDocument,
            contentType: .commaSeparatedText,
            defaultFilename: exportFileName
        ) { result in
            if case .failure(let error) = result { store.report(error) }
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
                if let year = store.year {
                    ScrollView { SummaryView(year: year, store: store) }
                }
            case .setup:
                SetupView(store: store)
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

// MARK: - CSV export

/// The in-memory file `.fileExporter` writes: the ledger's own CSV text
/// (`Support/LedgerCSV.swift`, `docs/v2/UI.md` §6, ⌘E). Export only, so
/// reading back a foreign file is not a case this window has to handle.
struct CSVDocument: FileDocument {
    static var readableContentTypes: [UTType] { [.commaSeparatedText] }
    static var writableContentTypes: [UTType] { [.commaSeparatedText] }

    var text: String

    init(text: String) { self.text = text }

    init(configuration: ReadConfiguration) throws {
        guard let data = configuration.file.regularFileContents else {
            throw CocoaError(.fileReadCorruptFile)
        }
        text = String(decoding: data, as: UTF8.self)
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: Data(text.utf8))
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
                // A failed submit leaves the text in place and raises
                // `presentedError`; stay open so the user can fix the line
                // instead of closing over an empty grid.
                if store.presentedError == nil {
                    isPresented = false
                }
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
        .onChange(of: store.savedAt) { _, _ in
            // `resolveAmbiguous` resubmits from the error alert's candidate
            // buttons, outside this field's own `onSubmit`, and clears the
            // text on success. A save that lands while the line still holds
            // text is someone else's (a pending void flushing) and must not
            // take the line away.
            if store.quickAddText.isEmpty { isPresented = false }
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
