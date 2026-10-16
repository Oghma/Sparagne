import Foundation
import Observation
import SparagneCore

/// What the statement import sheet is doing, step by step: read a file,
/// map its columns, preview every row, import (`core/src/statement/mod.rs`).
///
/// The core decides what becomes of each row; the model only holds what the
/// user chose (mapping, target, and per row its kind, note, category,
/// envelope, what becomes of a match, or that it is ticked off) and turns it
/// into the core's `StatementMapping`, `StatementOptions` and
/// `StatementRowOverride`s. Nothing is written before `runImport()`.
@Observable
final class StatementImportModel {
    /// The pages of the sheet, in order.
    enum Step: Equatable {
        case file
        case mapping
        case review
        case report
    }

    /// Where the mapping the editor opened with came from.
    enum MappingSource: Equatable {
        /// The last import of a file with the same header into this vault.
        case remembered
        /// A built-in preset recognised from the header; the name to show.
        case preset(String)
        /// Nothing known: the user maps the columns.
        case blank
    }

    /// The preview's counts, with the rows ticked off in the review moved
    /// from new to skipped, as the import will count them. A flagged row the
    /// user imports anyway comes back from the core as new, and counts as
    /// one.
    struct Counts: Equatable {
        var new = 0
        var alreadyImported = 0
        var skipped = 0
        var invalid = 0
        var rounded = 0
        /// Rows a transaction already in the vault may stand for, left out.
        var possibleDuplicates = 0
        /// The other side of a transfer already in the vault, left out.
        var matchedTransfers = 0
        /// Possible duplicates whose match the import moves into the
        /// statement's wallet.
        var moves = 0
        /// Transfer rows written in place of the entry they matched, which
        /// the import deletes: written, though not among `new`, nor among
        /// `possibleDuplicates`.
        var replaces = 0

        /// Every row the import writes.
        var written: Int { new + replaces }
    }

    /// What a sync round asked for by the import came to.
    enum SyncResult: Equatable, Sendable {
        /// The vault is as the server holds it.
        case synced
        /// The vault is this Mac's alone (no account): nothing to bring in.
        case notSynced
        /// Offline, or the server refused: the plan reads this Mac's copy.
        case failed
    }

    /// Runs one sync round and says how it went (`syncRound(on:)`).
    typealias SyncRound = @MainActor @Sendable () async -> SyncResult

    /// Everything a preview depends on. The sheet re-runs the preview when
    /// it changes (`.task(id:)`).
    struct PreviewInput: Hashable {
        let mapping: StatementMapping
        let options: StatementOptions
        /// The rows' own kinds, envelopes and choices over a match: they
        /// change what a row is, so the core has to see them. A row ticked
        /// off, a note or a category typed do not, and the review counts
        /// them itself, so a click or a keystroke never re-reads the file.
        let rowChoices: [StatementRowOverride]
    }

    // MARK: Dependencies

    @ObservationIgnored let store: AppStore
    @ObservationIgnored private let mappings: StatementMappingStore
    /// The vault the sheet was opened on; a vault switch behind the sheet
    /// does not move the import.
    @ObservationIgnored let vaultId: Uuid?
    /// IANA name for the rows whose date carries no offset.
    @ObservationIgnored let timezone: String
    /// Brings the vault up to date with the server before a file is planned
    /// against it, and again before the import: a Mac that has not pulled
    /// would plan again the rows another device already imported, kept or
    /// moved. `nil` when nothing syncs, as in most tests.
    @ObservationIgnored private let sync: SyncRound?

    // MARK: State

    var step: Step = .file
    private(set) var fileName: String?
    private(set) var text: String?
    private(set) var detection: StatementDetection?
    private(set) var source: MappingSource = .blank

    var mapping = StatementImportModel.blankMapping(delimiter: ",")
    /// The wallet the statement belongs to. Required; preselected when the
    /// vault has a single wallet, or when the last file with this header
    /// went to one still active (`restore`), since a wrong guess imports
    /// into the wrong account.
    var walletId: Uuid? {
        didSet {
            guard walletId != oldValue else { return }
            releaseTransfers(into: walletId)
            if let walletId { alsoCheckWallets.remove(walletId) }
        }
    }
    /// `nil` is Unallocated, the core's default.
    var flowId: Uuid?
    /// Other wallets where a row entered by hand may be the duplicate of a
    /// statement row: one found there is moved into `walletId` by default.
    /// Never holds `walletId`, which is always searched. Empty for every
    /// file and never remembered (`StatementImportTarget`).
    private(set) var alsoCheckWallets: Set<Uuid> = []
    /// Rows dated before this day are skipped (`before_start`); `nil`
    /// imports the whole file.
    var since: NaiveDate?

    private(set) var preview: StatementPreview?
    /// Why the last preview failed: a column not in the file, a wallet gone.
    private(set) var previewProblem: StatementImportProblem?
    private(set) var isPreviewing = false

    /// Categories typed in the review, by line. A line not here shows the
    /// row's category (a description rule's or the bank's match) or the
    /// history's suggestion.
    private(set) var editedCategories: [UInt32: String] = [:]
    /// Notes typed in the review, by line. A line not here shows the row's
    /// note: a description rule's, else the bank's text.
    private(set) var editedNotes: [UInt32: String] = [:]
    /// Kinds picked in the review, by line, over the rules.
    private(set) var editedActions: [UInt32: StatementAction] = [:]
    /// Envelopes picked in the review, by line, as real flow ids
    /// (Unallocated's included). A line not here goes to `flowId`.
    private(set) var editedFlows: [UInt32: Uuid] = [:]
    /// What becomes of a flagged row, by line, over the core's default.
    private(set) var matchChoices: [UInt32: StatementMatchChoice] = [:]
    /// New rows ticked off in the review: they go to the core as `skip`.
    private(set) var excludedLines: Set<UInt32> = []
    /// Note -> the category past notes like it were filed under.
    private(set) var suggestions: [String: String] = [:]

    private(set) var report: StatementReport?
    /// Entries the last import replaced by their transfers. The report
    /// counts each such transfer as executed and its row as a possible
    /// duplicate; this is how many of those rows the plan sent replaced and
    /// the core did not refuse.
    private(set) var replaced = 0
    private(set) var isImporting = false
    /// A file that could not be read, or an import the core refused whole.
    var problem: StatementImportProblem?
    /// The last sync asked for did not get through. It stops nothing: the
    /// sheet only says, quietly, that the plan reads this Mac's copy.
    private(set) var syncFailed = false
    /// A sync round asked for by the import is running: the sheet says so,
    /// so a slow server never reads as a hang.
    private(set) var isSyncing = false

    /// Which preview may write the state: a slower one started before the
    /// last change must not overwrite what the newer one found.
    @ObservationIgnored private var previewGeneration = 0
    /// Notes already asked about, answered or not, so a new preview asks the
    /// core only about the ones it has not seen.
    @ObservationIgnored private var askedNotes: Set<String> = []
    /// What each row is without the user's kind (the description rule's,
    /// the type rule's or the default action), by line: read off the last
    /// preview that did not carry one for the line, so an edit put back to
    /// it is forgotten instead of sent.
    @ObservationIgnored private var baseActions: [UInt32: StatementAction] = [:]
    /// What becomes of each matched row without the user's choice, by line,
    /// read off the last preview that carried no choice for the line: the
    /// core alone knows a match near the file's edges, so this is the
    /// default a choice put back is compared with.
    @ObservationIgnored private var baseChoices: [UInt32: StatementMatchChoice] = [:]
    /// A file just read has not been planned against a synced vault yet.
    @ObservationIgnored private var syncBeforePreview = false
    /// The sync round on its way, which every preview waits for.
    @ObservationIgnored private var syncing: Task<Void, Never>?

    init(
        store: AppStore,
        mappings: StatementMappingStore = StatementMappingStore(),
        timezone: String = TimeZone.current.identifier,
        sync: SyncRound? = nil
    ) {
        self.store = store
        self.mappings = mappings
        self.timezone = timezone
        self.sync = sync
        vaultId = store.currentVault?.id
        walletId = store.wallets.count == 1 ? store.wallets.first?.id : nil
    }

    /// The pages the sheet's step strip may jump to from here: back to any
    /// page already passed, ahead only as far as the footer's own button
    /// would go (a review needs a preview the core accepted). Nothing moves
    /// while the import runs or once it has: the report is the last page.
    func isReachable(_ target: Step) -> Bool {
        guard !isImporting, report == nil else { return target == step }
        switch target {
        case .file: return true
        case .mapping: return detection != nil
        case .review: return preview != nil && previewProblem == nil
        case .report: return false
        }
    }

    // MARK: - Reading the file

    /// Reads the file the user picked. The URL comes from `.fileImporter`, so
    /// it is security scoped under the sandbox.
    func open(_ url: URL) async {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            let data = try Data(contentsOf: url)
            await load(data: data, fileName: url.lastPathComponent)
        } catch {
            problem = StatementImportProblem(error)
        }
    }

    /// Decodes and detects the file off the main actor, then opens the
    /// mapping step on the best mapping known for it, aimed where the last
    /// file with its header went.
    func load(data: Data, fileName: String) async {
        do {
            let read = try await Task.detached { try StatementImportModel.read(data) }.value
            text = read.text
            detection = read.detection
            self.fileName = fileName
            // The wallet first: the mapping is adopted against it.
            alsoCheckWallets = []
            if let vaultId, let target = mappings.target(vaultId: vaultId, headers: read.detection.headers) {
                restore(target)
            }
            let initial = initialMapping(for: read.detection)
            mapping = adopt(initial.mapping, headers: read.detection.headers, delimiter: read.detection.delimiter)
            source = initial.source
            preview = nil
            previewProblem = nil
            editedCategories = [:]
            editedNotes = [:]
            editedActions = [:]
            editedFlows = [:]
            matchChoices = [:]
            excludedLines = []
            baseActions = [:]
            baseChoices = [:]
            replaced = 0
            syncBeforePreview = true
            report = nil
            problem = nil
            step = .mapping
        } catch {
            problem = StatementImportProblem(error)
        }
    }

    /// The text of a statement and what the core recognises in it.
    nonisolated static func read(_ data: Data) throws -> (text: String, detection: StatementDetection) {
        guard let text = decode(data) else { throw StatementFileError.notText }
        let detection = try detectStatement(text: text)
        guard detection.headers.contains(where: { !$0.trimmingCharacters(in: .whitespaces).isEmpty }) else {
            throw StatementFileError.noHeader
        }
        return (text, detection)
    }

    /// Banks still export in the Windows code page. UTF-8 first (with or
    /// without a BOM, which the core drops), UTF-16 when a BOM says so, then
    /// Windows-1252, then ISO Latin 1, which accepts any byte.
    nonisolated static func decode(_ data: Data) -> String? {
        if data.starts(with: [0xFF, 0xFE]) || data.starts(with: [0xFE, 0xFF]) {
            return String(data: data, encoding: .utf16)
        }
        return String(data: data, encoding: .utf8)
            ?? String(data: data, encoding: .windowsCP1252)
            ?? String(data: data, encoding: .isoLatin1)
    }

    /// Puts back where the last file with this header went, as far as it
    /// still exists: a wallet archived or deleted since is not chosen (the
    /// one already chosen stays), and an envelope gone reads as Unallocated.
    func restore(_ target: StatementImportTarget) {
        if let wallet = target.walletId, store.wallets.contains(where: { $0.id == wallet }) { walletId = wallet }
        flowId = target.flowId.flatMap { id in
            store.flows.contains { $0.id == id && !$0.isUnallocated } ? id : nil
        }
        since = target.since
    }

    /// Where the import goes, as `restore` puts it back next time: the
    /// wallets also searched for duplicates are left out.
    var target: StatementImportTarget {
        StatementImportTarget(walletId: walletId, flowId: flowId, since: since)
    }

    /// Remembered for this vault and header, else the preset the header
    /// matches, else nothing.
    private func initialMapping(for detection: StatementDetection) -> (mapping: StatementMapping, source: MappingSource) {
        if let vaultId, let remembered = mappings.mapping(vaultId: vaultId, headers: detection.headers) {
            return (remembered, .remembered)
        }
        if let id = detection.presetId, let preset = statementPresets().first(where: { $0.id == id }) {
            return (preset.mapping, .preset(Self.presetName(preset)))
        }
        return (Self.blankMapping(delimiter: detection.delimiter), .blank)
    }

    /// A mapping made to fit this file: columns spelled as the header spells
    /// them (the pickers match on the exact text), columns the file lacks
    /// cleared, the delimiter the file uses, and transfer wallets that are no
    /// longer active, or that are the target, dropped from the type rules,
    /// the fallback and the description rules.
    private func adopt(_ mapping: StatementMapping, headers: [String], delimiter: String) -> StatementMapping {
        func column(_ name: String?) -> String? {
            guard let wanted = name?.trimmingCharacters(in: .whitespaces).lowercased(), !wanted.isEmpty else {
                return nil
            }
            return headers.first { $0.trimmingCharacters(in: .whitespaces).lowercased() == wanted }
        }
        let active = Set(store.wallets.map(\.id))
        func live(_ action: StatementAction) -> StatementAction {
            guard let wallet = action.counterWalletId, !active.contains(wallet) || wallet == walletId else {
                return action
            }
            return action.withCounterWallet(nil)
        }
        var adopted = mapping
        adopted.delimiter = delimiter
        adopted.dateColumn = column(mapping.dateColumn) ?? ""
        adopted.amountColumn = column(mapping.amountColumn) ?? ""
        adopted.descriptionColumns = mapping.descriptionColumns.compactMap(column)
        adopted.typeColumn = column(mapping.typeColumn)
        adopted.statusColumn = column(mapping.statusColumn)
        adopted.currencyColumn = column(mapping.currencyColumn)
        adopted.categoryColumn = column(mapping.categoryColumn)
        adopted.originalAmountColumn = column(mapping.originalAmountColumn)
        adopted.originalCurrencyColumn = column(mapping.originalCurrencyColumn)
        adopted.typeRules = mapping.typeRules.map { StatementTypeRule(value: $0.value, action: live($0.action)) }
        adopted.defaultAction = live(mapping.defaultAction)
        adopted.descriptionRules = mapping.descriptionRules.map { rule in
            var adoptedRule = rule
            adoptedRule.action = rule.action.map(live)
            return adoptedRule
        }
        return adopted
    }

    /// Nothing mapped yet. A `;` file is guessed to come from a continental
    /// bank: day-first dates and a decimal comma.
    nonisolated static func blankMapping(delimiter: String) -> StatementMapping {
        let continental = delimiter == ";"
        return StatementMapping(
            delimiter: delimiter,
            dateColumn: "",
            dateFormat: continental ? .dayMonthYear : .isoDate,
            amountColumn: "",
            amountSign: .outflowNegative,
            decimalComma: continental,
            descriptionColumns: [],
            typeColumn: nil,
            typeRules: [],
            defaultAction: .bySign,
            statusColumn: nil,
            skipStatuses: [],
            currencyColumn: nil,
            categoryColumn: nil,
            originalAmountColumn: nil,
            originalCurrencyColumn: nil
        )
    }

    /// The name of a built-in preset, in the user's language when the app
    /// knows it.
    nonisolated static func presetName(_ preset: StatementPreset) -> String {
        switch preset.id {
        case "card-transactions": String(localized: "Card transactions")
        default: preset.name
        }
    }

    // MARK: - The mapping

    /// Date and amount are the two columns the core cannot do without.
    var mappingIsComplete: Bool {
        guard !mapping.dateColumn.isEmpty, !mapping.amountColumn.isEmpty else { return false }
        if case .custom(let pattern) = mapping.dateFormat {
            return !pattern.trimmingCharacters(in: .whitespaces).isEmpty
        }
        return true
    }

    /// The header without blank or repeated names, for the column pickers.
    var columns: [String] {
        var seen: Set<String> = []
        return (detection?.headers ?? []).filter { header in
            let name = header.trimmingCharacters(in: .whitespaces)
            return !name.isEmpty && seen.insert(name.lowercased()).inserted
        }
    }

    /// The distinct values of `column` in the sample, first seen first: what
    /// the type rules and the skipped statuses offer to add.
    func sampleValues(of column: String?) -> [String] {
        guard let column, let detection,
              let index = detection.headers.firstIndex(of: column) else { return [] }
        var seen: Set<String> = []
        return detection.sample.compactMap { row in
            guard index < row.count else { return nil }
            let value = row[index].trimmingCharacters(in: .whitespaces)
            return !value.isEmpty && seen.insert(value.lowercased()).inserted ? value : nil
        }
    }

    /// Type values of the sample that no rule covers yet.
    var unruledTypeValues: [String] {
        let ruled = Set(mapping.typeRules.map { $0.value.trimmingCharacters(in: .whitespaces).lowercased() })
        return sampleValues(of: mapping.typeColumn).filter { !ruled.contains($0.lowercased()) }
    }

    func addRule(value: String) {
        mapping.typeRules.append(StatementTypeRule(value: value, action: .expense))
    }

    func removeRule(at index: Int) {
        guard mapping.typeRules.indices.contains(index) else { return }
        mapping.typeRules.remove(at: index)
    }

    /// Skipped statuses compare without case in the core, so a token already
    /// there in another case is not added twice.
    func addSkipStatus(_ status: String) {
        let trimmed = status.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty,
              !mapping.skipStatuses.contains(where: { $0.lowercased() == trimmed.lowercased() }) else { return }
        mapping.skipStatuses.append(trimmed)
    }

    func removeSkipStatus(_ status: String) {
        mapping.skipStatuses.removeAll { $0 == status }
    }

    /// Wallets a transfer can name: every active one but the target, since
    /// the core refuses a transfer from a wallet to itself. The same wallets
    /// are the ones "Also look for duplicates in" offers, the target being
    /// always searched.
    var counterWallets: [WalletView] {
        store.wallets.filter { $0.id != walletId }
    }

    /// A new target that a transfer already names, in a type rule, a
    /// description rule or a row's own kind, would make it a transfer to
    /// itself, which the core refuses: it forgets its wallet and waits for
    /// another.
    private func releaseTransfers(into target: Uuid?) {
        guard let target else { return }
        func released(_ action: StatementAction) -> StatementAction {
            action.counterWalletId == target ? action.withCounterWallet(nil) : action
        }
        let rules = mapping.typeRules.map { StatementTypeRule(value: $0.value, action: released($0.action)) }
        if rules != mapping.typeRules { mapping.typeRules = rules }
        let fallback = released(mapping.defaultAction)
        if fallback != mapping.defaultAction { mapping.defaultAction = fallback }
        let described = mapping.descriptionRules.map { rule in
            var releasedRule = rule
            releasedRule.action = rule.action.map(released)
            return releasedRule
        }
        if described != mapping.descriptionRules { mapping.descriptionRules = described }
        let edited = editedActions.mapValues(released)
        if edited != editedActions { editedActions = edited }
    }

    // MARK: - Description rules

    /// A blank rule at the end, which matches nothing until its text is
    /// typed.
    func addDescriptionRule() {
        mapping.descriptionRules.append(StatementDescriptionRule(contains: ""))
    }

    func removeDescriptionRule(at index: Int) {
        guard mapping.descriptionRules.indices.contains(index) else { return }
        mapping.descriptionRules.remove(at: index)
    }

    /// The rows of the last preview the rule at `index` decided.
    func matchCount(ofRule index: Int) -> Int {
        preview?.rows.filter { $0.descriptionRule.map(Int.init) == index }.count ?? 0
    }

    /// A rule that makes rows like `row` what the review shows it as now:
    /// its whole description as the text to look for, its kind, its
    /// category (an entry's only) and its note (only one that is not the
    /// bank's text, which would stamp this row's text on every other row the
    /// rule finds). The rule goes before the one that decided the row, if
    /// any, else at the end, so it is the one that decides the row; the
    /// row's own kind, note and category are forgotten, so the rule is what
    /// drives it. Its envelope and what becomes of a match stay the row's.
    ///
    /// Returns where the rule went, for the editor that opens on it.
    @discardableResult
    func makeRule(from row: StatementRow) -> Int {
        let kind = action(for: row)
        let category = categoryText(for: row).trimmingCharacters(in: .whitespacesAndNewlines)
        let note = noteText(for: row).trimmingCharacters(in: .whitespacesAndNewlines)
        let rule = StatementDescriptionRule(
            contains: row.payee,
            action: kind,
            category: Self.takesCategory(kind) && !category.isEmpty ? category : nil,
            note: note.isEmpty || note == row.payee ? nil : note
        )
        let index = row.descriptionRule.map(Int.init)
            .flatMap { mapping.descriptionRules.indices.contains($0) ? $0 : nil }
            ?? mapping.descriptionRules.count
        mapping.descriptionRules.insert(rule, at: index)
        editedActions[row.line] = nil
        editedNotes[row.line] = nil
        editedCategories[row.line] = nil
        return index
    }

    // MARK: - Where else to look

    func isAlsoChecked(_ wallet: Uuid) -> Bool {
        alsoCheckWallets.contains(wallet)
    }

    /// Ticks a wallet in or out of "Also look for duplicates in". The target
    /// is not one: it is always searched.
    func setAlsoChecked(_ checked: Bool, wallet: Uuid) {
        guard wallet != walletId else { return }
        if checked { alsoCheckWallets.insert(wallet) } else { alsoCheckWallets.remove(wallet) }
    }

    /// `alsoCheckWallets` as the core takes them: active ones only, in a
    /// stable order, so the same choice is the same preview input.
    private var checkedWallets: [Uuid] {
        let active = Set(store.wallets.map(\.id))
        return alsoCheckWallets.filter { active.contains($0) && $0 != walletId }.sorted()
    }

    // MARK: - Preview

    /// `nil` until there is a file, a complete mapping and a wallet.
    var previewInput: PreviewInput? {
        guard text != nil, mappingIsComplete, let walletId else { return nil }
        return PreviewInput(
            mapping: mapping,
            options: StatementOptions(
                walletId: walletId,
                flowId: flowId,
                timezone: timezone,
                alsoCheckWallets: checkedWallets,
                since: since
            ),
            rowChoices: rowChoices
        )
    }

    /// One override per row given its own kind, an envelope other than the
    /// file's, or a choice over its match, carrying those alone. Built from
    /// the edits, not from the preview, which they feed.
    private var rowChoices: [StatementRowOverride] {
        let fileFlow = fileFlowId
        let lines = Set(editedActions.keys).union(editedFlows.keys).union(matchChoices.keys).sorted()
        return lines.compactMap { line in
            let action = editedActions[line]
            let flow = editedFlows[line].flatMap { $0 == fileFlow ? nil : $0 }
            let onMatch = matchChoices[line]
            guard action != nil || flow != nil || onMatch != nil else { return nil }
            return StatementRowOverride(
                line: line, category: nil, note: nil, skip: false,
                action: action, flowId: flow, onMatch: onMatch
            )
        }
    }

    /// The envelope of the file as a real flow id: `flowId`, or the vault's
    /// Unallocated when it is `nil`.
    var fileFlowId: Uuid? {
        flowId ?? store.flows.first(where: \.isUnallocated)?.id
    }

    /// Asks the core what importing would do now, then the categories the
    /// history suggests for the notes it has not seen yet. The first preview
    /// of a file waits for a sync round, and any preview waits for a round
    /// already on its way, so the file is planned against the vault as the
    /// server holds it.
    func refreshPreview() async {
        previewGeneration += 1
        let generation = previewGeneration
        guard previewInput != nil else {
            preview = nil
            previewProblem = nil
            isPreviewing = false
            return
        }
        isPreviewing = true
        if syncBeforePreview {
            syncBeforePreview = false
            await syncVault()
        } else {
            await syncing?.value
        }
        // Read after the sync: a change made while it ran started a newer
        // preview, which plans instead.
        guard generation == previewGeneration else { return }
        guard let input = previewInput, let text, let vaultId else {
            preview = nil
            previewProblem = nil
            isPreviewing = false
            return
        }
        do {
            let result = try await store.core.previewStatement(
                vaultId: vaultId,
                text: text,
                mapping: input.mapping,
                options: input.options,
                overrides: input.rowChoices
            )
            guard generation == previewGeneration else { return }
            let chosen = Set(input.rowChoices.filter { $0.action != nil }.map(\.line))
            for row in result.rows where !chosen.contains(row.line) {
                baseActions[row.line] = row.action
            }
            let answered = Set(input.rowChoices.filter { $0.onMatch != nil }.map(\.line))
            for row in result.rows where !answered.contains(row.line) {
                baseChoices[row.line] = row.matchChoice
            }
            preview = result
            previewProblem = nil
            await suggestCategories(for: result)
        } catch {
            guard generation == previewGeneration else { return }
            preview = nil
            previewProblem = StatementImportProblem(error)
        }
        if generation == previewGeneration { isPreviewing = false }
    }

    /// Runs one sync round, or joins the one already asked for, and notes
    /// whether it got through. Nothing to do when nothing syncs.
    private func syncVault() async {
        guard let sync else { return }
        if let syncing {
            await syncing.value
            return
        }
        isSyncing = true
        let round = Task { [weak self] in
            let result = await sync()
            self?.syncFailed = result == .failed
            self?.isSyncing = false
        }
        syncing = round
        await round.value
        if syncing == round { syncing = nil }
    }

    /// One call for every note of a new row not asked about before, looking
    /// a year back. The row's note rather than the bank's text: a note a
    /// description rule cleaned matches the history better. Best effort: a
    /// failure leaves the fields empty.
    private func suggestCategories(for preview: StatementPreview) async {
        guard let vaultId else { return }
        let notes = Set(
            preview.rows
                .filter { Self.takesCategory($0) && !$0.note.isEmpty && !askedNotes.contains($0.note) }
                .map(\.note)
        ).sorted()
        guard !notes.isEmpty else { return }
        askedNotes.formUnion(notes)
        let yearAgo = Calendar.current.date(byAdding: .year, value: -1, to: Date()) ?? Date()
        guard let answers = try? await store.core.suggestCategories(
            vaultId: vaultId,
            notes: notes,
            since: CoreDate.utcString(yearAgo)
        ) else { return }
        for (note, answer) in zip(notes, answers) {
            if let answer { suggestions[note] = answer.name }
        }
    }

    /// The preview's counts as the import will see them: the rows ticked
    /// off, new or replacing, moved to skipped.
    var counts: Counts {
        guard let preview else { return Counts() }
        let excludedNew = preview.rows.filter { $0.status == .new && excludedLines.contains($0.line) }
        // The core counts a replacing row among the possible duplicates; it
        // is written all the same, unless ticked off.
        let replacing = preview.rows.filter(Self.replaces)
        let excludedReplacing = replacing.filter { excludedLines.contains($0.line) }
        let excluded = excludedNew + excludedReplacing
        return Counts(
            new: Int(preview.newRows) - excludedNew.count,
            alreadyImported: Int(preview.alreadyImported),
            skipped: Int(preview.skipped) + excluded.count,
            invalid: Int(preview.invalid),
            rounded: Int(preview.rounded) - excluded.filter(\.rounded).count,
            possibleDuplicates: Int(preview.possibleDuplicates) - replacing.count,
            matchedTransfers: Int(preview.matchedTransfers),
            moves: Int(preview.moves),
            replaces: replacing.count - excludedReplacing.count
        )
    }

    // MARK: - The review

    /// A new income, expense or refund: the rows a category means something
    /// for. A transfer has none.
    nonisolated static func takesCategory(_ row: StatementRow) -> Bool {
        guard row.status == .new, let kind = row.kind else { return false }
        return kind == .income || kind == .expense || kind == .refund
    }

    /// An action whose rows are entries, which have a category.
    nonisolated static func takesCategory(_ action: StatementAction) -> Bool {
        switch action {
        case .bySign, .expense, .income, .refund: true
        case .transferIn, .transferOut, .skip: false
        }
    }

    /// What the category field of a row shows: the user's text, else the
    /// row's category (a description rule's, else the bank's match), else
    /// the history's suggestion for its note.
    func categoryText(for row: StatementRow) -> String {
        editedCategories[row.line] ?? row.category ?? suggestions[row.note] ?? ""
    }

    func setCategory(_ text: String, for line: UInt32) {
        editedCategories[line] = text
    }

    /// What the note field of a row shows: the user's text, else the row's
    /// note (a description rule's, else the bank's text).
    func noteText(for row: StatementRow) -> String {
        editedNotes[row.line] ?? row.note
    }

    func setNote(_ text: String, for line: UInt32) {
        editedNotes[line] = text
    }

    func isIncluded(_ line: UInt32) -> Bool {
        !excludedLines.contains(line)
    }

    func setIncluded(_ included: Bool, line: UInt32) {
        if included { excludedLines.remove(line) } else { excludedLines.insert(line) }
    }

    /// Whether the review offers a kind for the row: one the rules decided,
    /// or that a kind of its own could make importable. A row already in
    /// the vault, unreadable, before the start day, of a skipped status or
    /// for nothing has no kind to change.
    nonisolated static func takesAction(_ row: StatementRow) -> Bool {
        switch row.status {
        case .new, .possibleDuplicate, .matchedTransfer: true
        case .alreadyImported, .invalid: false
        case .skipped(let code, _): ["skipped_by_rule", "needs_wallet", "skipped_by_you"].contains(code)
        }
    }

    /// What a row is taken for: the user's kind, else what the rules said.
    func action(for row: StatementRow) -> StatementAction {
        editedActions[row.line] ?? row.action
    }

    /// Gives a row its own kind. The kind the rules give it already, or the
    /// one `BySign` resolved to, forgets the edit instead.
    func setAction(_ action: StatementAction, for row: StatementRow) {
        let base = baseActions[row.line] ?? row.action
        let same = action == base || (base == .bySign && action == (row.outflow ? .expense : .income))
        if same { editedActions[row.line] = nil } else { editedActions[row.line] = action }
    }

    /// The envelope of an entry: the user's, else the file's.
    func flow(for row: StatementRow) -> Uuid? {
        editedFlows[row.line] ?? fileFlowId
    }

    /// Puts a row in an envelope of its own; the file's forgets the edit.
    func setFlow(_ flow: Uuid, for row: StatementRow) {
        if flow == fileFlowId { editedFlows[row.line] = nil } else { editedFlows[row.line] = flow }
    }

    /// A row that matched a transaction already in the vault: flagged, or
    /// imported anyway (it keeps the match, so the review still shows it).
    nonisolated static func isFlagged(_ row: StatementRow) -> Bool {
        switch row.status {
        case .possibleDuplicate, .matchedTransfer: true
        case .new: row.existing != nil
        case .alreadyImported, .skipped, .invalid: false
        }
    }

    /// A transfer row whose other side is already in the vault as an
    /// income, an expense or a refund: the other bank's row imported
    /// without a rule, or the move typed by hand. Its transfer can replace
    /// that entry.
    nonisolated static func isReplace(_ row: StatementRow) -> Bool {
        guard row.kind == .transferWallet, let existing = row.existing else { return false }
        return existing.kind == .income || existing.kind == .expense || existing.kind == .refund
    }

    /// A row the import writes as a transfer in place of the entry it
    /// matched: flagged, yet written.
    nonisolated static func replaces(_ row: StatementRow) -> Bool {
        row.status == .possibleDuplicate && row.matchChoice == .replaceExisting
    }

    /// A row the review's box can leave out: a new one, or a transfer row
    /// whatever becomes of the entry it matched. A transfer row has no
    /// "keep the one there" of its own (the transfer would be missing), so
    /// the box is how it stays out, writing and claiming nothing.
    nonisolated static func takesTick(_ row: StatementRow) -> Bool {
        row.status == .new || isReplace(row)
    }

    /// The match is an entry typed into another wallet than the
    /// statement's, standing for the same spend: it can be moved here
    /// rather than left where it is. An entry standing for a transfer is
    /// replaced, never moved.
    func canMove(_ row: StatementRow) -> Bool {
        guard !Self.isReplace(row), let wallet = row.existing?.walletId else { return false }
        return wallet != walletId
    }

    /// What the core does with a match nobody chose for, as the last
    /// preview without the user's choice said: import the row beside a
    /// match that more likely stands for another export's row (near the
    /// file's first or last days), move an entry found in another wallet
    /// here, replace an entry standing for the transfer when it very likely
    /// is its other side (else import the row beside it), leave the row out
    /// otherwise. Worked out the same way for a row no preview has said it
    /// of.
    func defaultChoice(for row: StatementRow) -> StatementMatchChoice {
        if let base = baseChoices[row.line] { return base }
        if matchChoices[row.line] == nil, let choice = row.matchChoice { return choice }
        if Self.isReplace(row) { return .replaceExisting }
        return canMove(row) ? .moveExisting : .skip
    }

    /// What becomes of a flagged row: the user's choice, else the core's.
    func matchChoice(for row: StatementRow) -> StatementMatchChoice? {
        guard Self.isFlagged(row) else { return nil }
        return matchChoices[row.line] ?? row.matchChoice ?? defaultChoice(for: row)
    }

    /// The choices that mean something for a flagged row, in the menu's
    /// order. A transfer row standing for an entry either replaces it or is
    /// imported beside it: keeping the entry in its place would leave the
    /// transfer missing, so it is left out with its box instead. Moving is
    /// only for an entry typed into another wallet.
    func availableChoices(for row: StatementRow) -> [StatementMatchChoice] {
        if Self.isReplace(row) { return [.replaceExisting, .importAnyway] }
        return canMove(row) ? [.skip, .moveExisting, .importAnyway] : [.skip, .importAnyway]
    }

    /// The default forgets the choice instead of sending it.
    func setMatchChoice(_ choice: StatementMatchChoice, for row: StatementRow) {
        if choice == defaultChoice(for: row) { matchChoices[row.line] = nil } else { matchChoices[row.line] = choice }
    }

    /// A flagged row, not a transfer standing for an entry, imported
    /// because its match more likely stands for another export's row: the
    /// core's default near the file's first or last days, which the review
    /// explains beside the match.
    func importsByDefault(_ row: StatementRow) -> Bool {
        !Self.isReplace(row) && Self.isFlagged(row)
            && defaultChoice(for: row) == .importAnyway && matchChoice(for: row) == .importAnyway
    }

    /// A transfer row imported beside the entry it matched because that
    /// entry is not very likely its other side (days apart, typed by hand):
    /// the review says it may be, and that replacing it is one choice away.
    func mayReplace(_ row: StatementRow) -> Bool {
        Self.isReplace(row) && defaultChoice(for: row) == .importAnyway && matchChoice(for: row) == .importAnyway
    }

    /// One override per row the user changed, each carrying only what
    /// changed: its kind, an envelope other than the file's, what becomes of
    /// its match, that it is ticked off (a new row, or a transfer row
    /// standing for an entry), and on a row written its note and, an entry,
    /// its category. An edit equal to what the row would be anyway sends
    /// nothing. A suggestion counts as a change, since only the app knows
    /// it; the row's own category does not, since the core applies it
    /// anyway. Emptying a field leaves the row's own note or category.
    var overrides: [StatementRowOverride] {
        guard let preview else { return [] }
        let choices = Dictionary(rowChoices.map { ($0.line, $0) }, uniquingKeysWith: { $1 })
        return preview.rows.compactMap { row -> StatementRowOverride? in
            let untouched = StatementRowOverride(line: row.line, category: nil, note: nil, skip: false)
            var override = choices[row.line] ?? untouched
            let tickedOff = Self.takesTick(row) && excludedLines.contains(row.line)
            if tickedOff { override.skip = true }
            let written = !tickedOff && (row.status == .new || Self.replaces(row))
            if written, Self.takesCategory(row) {
                let text = categoryText(for: row).trimmingCharacters(in: .whitespacesAndNewlines)
                if !text.isEmpty, text != row.category { override.category = text }
            }
            if written, let typed = editedNotes[row.line] {
                let note = typed.trimmingCharacters(in: .whitespacesAndNewlines)
                if !note.isEmpty, note != row.note { override.note = note }
            }
            return override == untouched ? nil : override
        }
    }

    // MARK: - Import

    /// Imports the rows with the user's overrides, remembers the mapping and
    /// the target for the next file with this header, and reloads the
    /// window.
    ///
    /// When the vault syncs, a round runs first and the preview is planned
    /// again on what it brought, so the import writes what that fresh plan
    /// says: a row another device imported or kept meanwhile is not written
    /// twice. A round that does not get through stops nothing.
    func runImport() async {
        guard previewInput != nil, !isImporting else { return }
        isImporting = true
        defer { isImporting = false }
        if sync != nil {
            await syncVault()
            await refreshPreview()
            guard preview != nil, previewProblem == nil else { return }
        }
        guard let input = previewInput, let text, let vaultId, let detection else { return }
        let replacing = Set(
            preview?.rows.filter { Self.replaces($0) && !excludedLines.contains($0.line) }.map(\.line) ?? []
        )
        do {
            let result = try await store.core.importStatement(
                vaultId: vaultId,
                text: text,
                mapping: input.mapping,
                options: input.options,
                overrides: overrides
            )
            mappings.remember(input.mapping, vaultId: vaultId, headers: detection.headers)
            mappings.remember(target, vaultId: vaultId, headers: detection.headers)
            replaced = replacing.subtracting(result.rejected.map(\.line)).count
            report = result
            problem = nil
            step = .report
            await store.reload()
        } catch {
            problem = StatementImportProblem(error)
        }
    }
}

// MARK: - Sync

extension StatementImportModel {
    /// The longest the import waits for the engine: a round of its own, or
    /// one already running, then its own.
    static let syncPatience: Duration = .seconds(30)

    /// A sync round on `engine` for the import. Logged out, the vault is
    /// this Mac's alone and nothing waits. `syncNow()` returns at once while
    /// a round is already running, so that round is waited for and followed
    /// by one of the import's own, which then holds what the other pulled;
    /// past `syncPatience`, or offline, or refused, the round reads as
    /// failed and the import goes on with what this Mac has.
    static func syncRound(on engine: SyncEngine) -> SyncRound {
        { [weak engine] in
            guard let engine, engine.isLoggedIn else { return .notSynced }
            let deadline = ContinuousClock.now + syncPatience
            repeat {
                while engine.status == .syncing, ContinuousClock.now < deadline {
                    try? await Task.sleep(for: .milliseconds(100))
                }
                guard ContinuousClock.now < deadline else { return .failed }
                await engine.syncNow()
            } while engine.status == .syncing
            switch engine.status {
            case .idle: return .synced
            case .syncing, .offline, .error: return .failed
            }
        }
    }
}

// MARK: - Supporting values

/// Why a chosen file is not a statement, before the core sees it.
enum StatementFileError: Error, Equatable {
    /// No text encoding reads it.
    case notText
    /// The first row names no column.
    case noHeader
}

/// A problem the import sheet shows under its own buttons: an alert on the
/// window would wait behind the sheet.
struct StatementImportProblem: Equatable, Sendable {
    let headline: String
    /// The core's English message, or the system's; may be empty.
    let detail: String

    init(headline: String, detail: String = "") {
        self.headline = headline
        self.detail = detail
    }

    init(_ error: Error) {
        switch error {
        case let error as DomainError:
            self.init(headline: ErrorMessages.summary(for: error.code), detail: error.message)
        case StatementFileError.notText:
            self.init(headline: String(localized: "This file is not text"))
        case StatementFileError.noHeader:
            self.init(headline: String(localized: "The first row of the file names no column"))
        default:
            self.init(headline: String(localized: "Something went wrong"), detail: error.localizedDescription)
        }
    }
}

/// A `StatementAction` as one flat choice of a picker; the wallet of a
/// transfer is picked beside it.
enum StatementActionChoice: String, CaseIterable, Identifiable {
    case expense
    case income
    case refund
    case transferIn
    case transferOut
    case skip
    case bySign

    var id: String { rawValue }

    /// What a row of the review can be made: every choice but `bySign`,
    /// which the review shows as the kind it resolved to.
    static let rowChoices: [StatementActionChoice] = [.expense, .income, .refund, .transferIn, .transferOut, .skip]

    init(_ action: StatementAction) {
        switch action {
        case .expense: self = .expense
        case .income: self = .income
        case .refund: self = .refund
        case .transferIn: self = .transferIn
        case .transferOut: self = .transferOut
        case .skip: self = .skip
        case .bySign: self = .bySign
        }
    }

    /// The action for this choice. Switching between the two transfers
    /// keeps the wallet already picked.
    func action(keeping current: StatementAction) -> StatementAction {
        let wallet = current.counterWalletId
        return switch self {
        case .expense: .expense
        case .income: .income
        case .refund: .refund
        case .transferIn: .transferIn(fromWalletId: wallet)
        case .transferOut: .transferOut(toWalletId: wallet)
        case .skip: .skip
        case .bySign: .bySign
        }
    }

    var isTransfer: Bool { self == .transferIn || self == .transferOut }
}

extension StatementAction {
    /// The other wallet of a transfer; `nil` for every other action.
    var counterWalletId: Uuid? {
        switch self {
        case .transferIn(let wallet): wallet
        case .transferOut(let wallet): wallet
        default: nil
        }
    }

    /// The same transfer with another wallet; any other action unchanged.
    func withCounterWallet(_ wallet: Uuid?) -> StatementAction {
        switch self {
        case .transferIn: .transferIn(fromWalletId: wallet)
        case .transferOut: .transferOut(toWalletId: wallet)
        default: self
        }
    }
}
