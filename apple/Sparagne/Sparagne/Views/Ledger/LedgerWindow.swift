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
    /// The window's, shared with its text fields: the ledger's steps go on the
    /// same stack as the typing, so Edit ▸ Undo takes back whichever came last.
    @Environment(\.undoManager) private var undoManager
    /// Driven by the two toggles in the Ledger menu.
    @AppStorage("showVoided") private var showVoided = false
    @AppStorage("showTransfers") private var showTransfers = false
    /// The optional WALLET column of the grid (`docs/v2/UI.md` §3), off until
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
            RecurringBanner(store: store) { sheet = .dueRecurring }
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
                    QuickAddOverlay(store: store, engine: engine, isPresented: $showsQuickAdd)
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
            Task {
                // Every row of the month, not only the pages scrolled to so
                // far: a month past one page would otherwise be cut short.
                await store.loadAll()
                // The export carries the columns the grid is showing, so a
                // file opened next to the window has the same shape
                // (`UI.md` §6).
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
/// decision (`DISTILLATO_V1.md` §2.3). One line, in the ledger's own ink;
/// Review opens the due sheet (`DueRecurringSheet`), where the periods are
/// executed or skipped.
struct RecurringBanner: View {
    let store: AppStore
    let onOpen: () -> Void

    private var count: Int {
        store.pendingRecurringItems.reduce(0) { $0 + $1.due.count }
    }

    var body: some View {
        // A viewer can neither execute nor skip, so the banner would be a
        // to-do list with nothing to do on it.
        if count > 0, !store.isReadOnly {
            HStack(spacing: 10) {
                Text("\u{25CF}")
                    .font(Face.footnote)
                    .foregroundStyle(Ink.accent)
                Text(CountText.recurringDue(count))
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

/// ⌘K: the one-line grammar of `DISTILLATO_V1.md` §3.1, over the grid, and
/// the command palette of `docs/v2/UI.md` §6 when the line starts with `>`.
///
/// The grid covers the common case; this covers the fast case, where the
/// whole row is one line of text and the fingers never leave the keyboard.
/// One field, two grammars: a transaction, or a command.
struct QuickAddOverlay: View {
    @Bindable var store: AppStore
    let engine: SyncEngine?
    @Binding var isPresented: Bool

    @FocusState private var focused: Bool
    @State private var palette = CommandPaletteModel()
    /// What the vault's history files the line's note under, when the line
    /// names no category: shown beside the preview, added only by ⇥.
    @State private var hint: NoteSuggestion?

    /// `>` in first position turns the field into the palette.
    private var isCommand: Bool { CommandPaletteModel.isCommand(store.quickAddText) }

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
            // The arrows belong to the list while the field is a palette; the
            // caret gets them back as soon as the `>` is gone.
            .onKeyPress(.upArrow) {
                guard isCommand else { return .ignored }
                palette.move(by: -1)
                return .handled
            }
            .onKeyPress(.downArrow) {
                guard isCommand else { return .ignored }
                palette.move(by: 1)
                return .handled
            }
            // ⇥ takes the hint into the line, where the preview shows it;
            // without one the key does what it always did.
            .onKeyPress(.tab) {
                guard !isCommand, let category = categoryHint,
                      let accepted = QuickAddSummary.accepting(category, into: store.quickAddText)
                else { return .ignored }
                store.quickAddText = accepted
                return .handled
            }
            .onSubmit {
                if isCommand {
                    // Nothing highlighted means nothing matched: stay open so
                    // the query can be fixed.
                    Task { if await palette.run() { close() } }
                    return
                }
                Task {
                    await store.submit(quickAdd: store.quickAddText)
                    // A failed submit leaves the text in place and raises
                    // `presentedError`; stay open so the user can fix the line
                    // instead of closing over an empty grid.
                    if store.presentedError == nil {
                        isPresented = false
                    }
                }
            }

            if isCommand {
                CommandPaletteList(model: palette, onRun: close)
            } else {
                HStack(spacing: 12) {
                    Text(preview.text)
                        .font(Face.footnote)
                        .foregroundStyle(preview.isError ? Ink.accent : Ink.dim)
                        .lineLimit(1)
                    if let category = categoryHint {
                        Spacer(minLength: 0)
                        Text("\u{21E5} #\(category)")
                            .font(Face.footnote)
                            .foregroundStyle(Ink.text)
                            .lineLimit(1)
                            .accessibilityLabel(String(localized: "Suggested category \(category), tab to add it"))
                    }
                }
            }
        }
        .padding(14)
        .frame(width: 520, alignment: .leading)
        .background(Ink.panel)
        .overlay(Rectangle().strokeBorder(Ink.accent, lineWidth: 1))
        .shadow(color: .black.opacity(0.5), radius: 20, y: 8)
        .onAppear {
            focused = true
            refreshActions()
        }
        // The entries say what the toggles will do and list the other vaults,
        // so they are built fresh every time the `>` is typed.
        .onChange(of: isCommand) { _, now in
            if now { refreshActions() }
        }
        .onChange(of: store.quickAddText) { _, new in
            palette.query = CommandPaletteModel.query(in: new)
        }
        .task(id: store.quickAddText) { await suggestCategory() }
        .onExitCommand(perform: close)
        .onChange(of: store.savedAt) { _, _ in
            // `resolveAmbiguous` resubmits from the error alert's candidate
            // buttons, outside this field's own `onSubmit`, and clears the
            // text on success. A save that lands while the line still holds
            // text is someone else's (a pending void flushing) and must not
            // take the line away.
            if store.quickAddText.isEmpty { isPresented = false }
        }
    }

    private func close() {
        store.quickAddText = ""
        isPresented = false
    }

    private func refreshActions() {
        palette.actions = CommandPaletteModel.ledgerActions(store: store, engine: engine)
        palette.query = CommandPaletteModel.query(in: store.quickAddText)
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

    /// The note of the line as typed, when the line parses and names no
    /// category.
    private var noteWithoutCategory: String? {
        let trimmed = store.quickAddText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !isCommand, case .success(let parsed) = store.preview(quickAdd: trimmed) else {
            return nil
        }
        return QuickAddSummary.noteWithoutCategory(parsed)
    }

    /// The hint, while it is about the note on the line and `#` can carry it.
    private var categoryHint: String? {
        guard let hint, hint.note == noteWithoutCategory,
              QuickAddSummary.accepting(hint.category, into: store.quickAddText) != nil
        else { return nil }
        return hint.category
    }

    /// The line has been still for a moment: ask the core about its note. An
    /// answer for a note the line no longer has is dropped.
    private func suggestCategory() async {
        guard let note = noteWithoutCategory, hint?.note != note else { return }
        guard (try? await Task.sleep(for: .milliseconds(250))) != nil else { return }
        guard let category = await store.suggestedCategory(forNote: note), noteWithoutCategory == note else { return }
        hint = NoteSuggestion(note: note, category: category)
    }
}
