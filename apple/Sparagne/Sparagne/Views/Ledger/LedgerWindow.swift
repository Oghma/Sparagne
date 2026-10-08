import SwiftUI
import SparagneCore
import UniformTypeIdentifiers

/// The window, top to bottom: the top bar, the sheet on
/// screen, and the sheet tabs with the sheet's status line. No sidebar and
/// no inspector: the grid is editable in place, and everything that manages
/// entities lives in the menus.
struct LedgerWindow: View {
    let store: AppStore
    let engine: SyncEngine?
    @Binding var sheet: MainWindow.SheetKind?

    @FocusState private var searchFocused: Bool
    @State private var showsQuickAdd = false
    /// The window's, shared with its text fields: the ledger's steps go on the
    /// same stack as the typing, so Edit ▸ Undo takes back whichever came last.
    @Environment(\.undoManager) private var undoManager
    /// Driven by the two toggles in the Ledger menu.
    @AppStorage("showVoided") private var showVoided = false
    @AppStorage("showTransfers") private var showTransfers = false
    /// The optional WALLET column of the grid, off until
    /// the View menu turns it on.
    @AppStorage("showWalletColumn") private var showWalletColumn = false

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
            TopBar(store: store, engine: engine, searchFocused: $searchFocused) { sheet = $0 }
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Ink.sheet)
            SheetTabBar(store: store) { statusLine }
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
                    // Near the top, over the sheet's first rows: where the
                    // eye already is when ⌘K is pressed.
                    QuickAddOverlay(store: store, engine: engine, isPresented: $showsQuickAdd)
                        .padding(.top, Metrics.topBar + 28)
                        .padding(.horizontal, Metrics.gutter)
                }
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .focusQuickAdd)) { _ in
            showsQuickAdd = true
        }
        .onReceive(NotificationCenter.default.publisher(for: .focusSearch)) { _ in
            // The Riepilogo has no field, so ⌘F goes to the Mastro's: the
            // tab has to switch and lay out before the field can take focus.
            // The other tabs have their own, already on screen.
            if store.tab == .summary { store.tab = .ledger }
            Task {
                await Task.yield()
                searchFocused = true
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .exportCSV)) { _ in
            Task {
                // Every row of the month, not only the pages scrolled to so
                // far: a month past one page would otherwise be cut short.
                await store.loadAll()
                // The export carries the columns the grid is showing, so a
                // file opened next to the window has the same shape.
                exportDocument = CSVDocument(text: LedgerCSV.render(store.rows, wallet: store.showWalletColumn))
                exportFileName = LedgerCSV.fileName(
                    vault: store.currentVault?.name ?? "",
                    month: store.month,
                    direction: store.direction
                )
                showsCSVExporter = true
            }
        }
        .onAppear {
            store.showVoided = showVoided
            store.showTransfers = showTransfers
            store.showWalletColumn = showWalletColumn
            store.attach(undoManager: undoManager)
        }
        .onChange(of: undoManager) { _, new in store.attach(undoManager: new) }
        // Both ways for all three: the menu writes the preference, the
        // palette writes the store, and the shared key keeps them one value.
        .onChange(of: showVoided) { _, new in store.showVoided = new }
        .onChange(of: store.showVoided) { _, new in showVoided = new }
        .onChange(of: showTransfers) { _, new in store.showTransfers = new }
        .onChange(of: store.showTransfers) { _, new in showTransfers = new }
        .onChange(of: showWalletColumn) { _, new in store.showWalletColumn = new }
        .onChange(of: store.showWalletColumn) { _, new in showWalletColumn = new }
        .task(id: store.searchText) {
            // Debounce: reload only once the field has been quiet for 300 ms,
            // and only if it actually changed since the last reload (the task
            // also runs for the value the view already had, e.g. on a fresh
            // mount after a vault switch).
            guard store.searchText != lastSearched else { return }
            guard (try? await Task.sleep(for: .milliseconds(300))) != nil else { return }
            lastSearched = store.searchText
            await store.reload()
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
                // The filters belong to the rows, so they sit over the grid
                // only; the panel runs the full height beside both (`.body`).
                HStack(spacing: 0) {
                    VStack(spacing: 0) {
                        FilterBar(store: store)
                        LedgerGrid(store: store)
                    }
                    SummaryPanel(summary: summary, store: store)
                }
            case .summary:
                if let year = store.year {
                    ScrollView { SummaryView(year: year, store: store) }
                }
            case .recurring:
                RecurringTab(store: store)
            case .setup:
                SetupView(store: store, engine: engine) { sheet = $0 }
            }
        } else {
            VStack {
                Spacer()
                Text(String(localized: "No vault yet"))
                    .font(Face.row)
                    .foregroundStyle(Ink.text3)
                Spacer()
            }
            .frame(maxWidth: .infinity)
        }
    }

    /// The figures of the sheet on screen, at the right of the tab bar.
    @ViewBuilder
    private var statusLine: some View {
        switch store.tab {
        case .summary: SummaryStatusLine(store: store)
        case .ledger: LedgerStatusLine(store: store)
        case .recurring: RecurringStatusLine(store: store)
        case .setup: SetupStatusLine(store: store)
        }
    }
}

// MARK: - CSV export

/// The in-memory file `.fileExporter` writes: the ledger's own CSV text
/// (`Support/LedgerCSV.swift`, ⌘E). Export only, so
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
