import Foundation
import Observation
import SparagneCore

/// What the statement import sheet is doing, step by step: read a file,
/// map its columns, preview every row, import (`core/src/statement/mod.rs`).
///
/// The core decides what becomes of each row; the model only holds what the
/// user chose (mapping, target, the category of a new row, the rows ticked
/// off) and turns it into the core's `StatementMapping`, `StatementOptions`
/// and `StatementRowOverride`s. Nothing is written before `runImport()`.
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
    /// from new to skipped, as the import will count them.
    struct Counts: Equatable {
        var new = 0
        var alreadyImported = 0
        var skipped = 0
        var invalid = 0
        var rounded = 0
    }

    /// Everything a preview depends on. The sheet re-runs the preview when
    /// it changes (`.task(id:)`).
    struct PreviewInput: Hashable {
        let mapping: StatementMapping
        let options: StatementOptions
    }

    // MARK: Dependencies

    @ObservationIgnored let store: AppStore
    @ObservationIgnored private let mappings: StatementMappingStore
    /// The vault the sheet was opened on; a vault switch behind the sheet
    /// does not move the import.
    @ObservationIgnored let vaultId: Uuid?
    /// IANA name for the rows whose date carries no offset.
    @ObservationIgnored let timezone: String

    // MARK: State

    var step: Step = .file
    private(set) var fileName: String?
    private(set) var text: String?
    private(set) var detection: StatementDetection?
    private(set) var source: MappingSource = .blank

    var mapping = StatementImportModel.blankMapping(delimiter: ",")
    /// The wallet the statement belongs to. Required; preselected only when
    /// the vault has a single wallet, since a wrong guess imports into the
    /// wrong account.
    var walletId: Uuid? {
        didSet { if walletId != oldValue { releaseTransfers(into: walletId) } }
    }
    /// `nil` is Unallocated, the core's default.
    var flowId: Uuid?

    private(set) var preview: StatementPreview?
    /// Why the last preview failed: a column not in the file, a wallet gone.
    private(set) var previewProblem: StatementImportProblem?
    private(set) var isPreviewing = false

    /// Categories typed in the review, by line. A line not here shows the
    /// bank's matched category or the history's suggestion.
    private(set) var editedCategories: [UInt32: String] = [:]
    /// New rows ticked off in the review: they go to the core as `skip`.
    private(set) var excludedLines: Set<UInt32> = []
    /// Payee -> the category past notes like it were filed under.
    private(set) var suggestions: [String: String] = [:]

    private(set) var report: StatementReport?
    private(set) var isImporting = false
    /// A file that could not be read, or an import the core refused whole.
    var problem: StatementImportProblem?

    /// Which preview may write the state: a slower one started before the
    /// last change must not overwrite what the newer one found.
    @ObservationIgnored private var previewGeneration = 0
    /// Payees already asked about, answered or not, so a new preview asks the
    /// core only about the ones it has not seen.
    @ObservationIgnored private var askedPayees: Set<String> = []

    init(
        store: AppStore,
        mappings: StatementMappingStore = StatementMappingStore(),
        timezone: String = TimeZone.current.identifier
    ) {
        self.store = store
        self.mappings = mappings
        self.timezone = timezone
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
    /// mapping step on the best mapping known for it.
    func load(data: Data, fileName: String) async {
        do {
            let read = try await Task.detached { try StatementImportModel.read(data) }.value
            text = read.text
            detection = read.detection
            self.fileName = fileName
            let initial = initialMapping(for: read.detection)
            mapping = adopt(initial.mapping, headers: read.detection.headers, delimiter: read.detection.delimiter)
            source = initial.source
            preview = nil
            previewProblem = nil
            editedCategories = [:]
            excludedLines = []
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
    /// longer active dropped.
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

    /// Wallets a transfer rule can name: every active one but the target,
    /// since the core refuses a transfer from a wallet to itself.
    var counterWallets: [WalletView] {
        store.wallets.filter { $0.id != walletId }
    }

    /// A new target that a transfer rule already names would make the rule
    /// a transfer to itself, which the core refuses: the rule forgets its
    /// wallet and waits for another.
    private func releaseTransfers(into target: Uuid?) {
        guard let target else { return }
        func released(_ action: StatementAction) -> StatementAction {
            action.counterWalletId == target ? action.withCounterWallet(nil) : action
        }
        let rules = mapping.typeRules.map { StatementTypeRule(value: $0.value, action: released($0.action)) }
        if rules != mapping.typeRules { mapping.typeRules = rules }
        let fallback = released(mapping.defaultAction)
        if fallback != mapping.defaultAction { mapping.defaultAction = fallback }
    }

    // MARK: - Preview

    /// `nil` until there is a file, a complete mapping and a wallet.
    var previewInput: PreviewInput? {
        guard text != nil, mappingIsComplete, let walletId else { return nil }
        return PreviewInput(
            mapping: mapping,
            options: StatementOptions(walletId: walletId, flowId: flowId, timezone: timezone)
        )
    }

    /// Asks the core what importing would do now, then the categories the
    /// history suggests for the payees it has not seen yet.
    func refreshPreview() async {
        previewGeneration += 1
        let generation = previewGeneration
        guard let input = previewInput, let text, let vaultId else {
            preview = nil
            previewProblem = nil
            isPreviewing = false
            return
        }
        isPreviewing = true
        do {
            let result = try await store.core.previewStatement(
                vaultId: vaultId,
                text: text,
                mapping: input.mapping,
                options: input.options
            )
            guard generation == previewGeneration else { return }
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

    /// One call for every payee of a new row not asked about before, looking
    /// a year back. Best effort: a failure leaves the fields empty.
    private func suggestCategories(for preview: StatementPreview) async {
        guard let vaultId else { return }
        let payees = Set(
            preview.rows
                .filter { Self.takesCategory($0) && !$0.payee.isEmpty && !askedPayees.contains($0.payee) }
                .map(\.payee)
        ).sorted()
        guard !payees.isEmpty else { return }
        askedPayees.formUnion(payees)
        let yearAgo = Calendar.current.date(byAdding: .year, value: -1, to: Date()) ?? Date()
        guard let answers = try? await store.core.suggestCategories(
            vaultId: vaultId,
            notes: payees,
            since: CoreDate.utcString(yearAgo)
        ) else { return }
        for (payee, answer) in zip(payees, answers) {
            if let answer { suggestions[payee] = answer.name }
        }
    }

    /// The preview's counts as the import will see them.
    var counts: Counts {
        guard let preview else { return Counts() }
        let excluded = preview.rows.filter { $0.status == .new && excludedLines.contains($0.line) }
        return Counts(
            new: Int(preview.newRows) - excluded.count,
            alreadyImported: Int(preview.alreadyImported),
            skipped: Int(preview.skipped) + excluded.count,
            invalid: Int(preview.invalid),
            rounded: Int(preview.rounded) - excluded.filter(\.rounded).count
        )
    }

    // MARK: - The review

    /// A new income, expense or refund: the rows a category means something
    /// for. A transfer has none.
    nonisolated static func takesCategory(_ row: StatementRow) -> Bool {
        guard row.status == .new, let kind = row.kind else { return false }
        return kind == .income || kind == .expense || kind == .refund
    }

    /// What the category field of a row shows: the user's text, else the
    /// category the bank's matched, else the history's suggestion.
    func categoryText(for row: StatementRow) -> String {
        editedCategories[row.line] ?? row.matchedCategory ?? suggestions[row.payee] ?? ""
    }

    func setCategory(_ text: String, for line: UInt32) {
        editedCategories[line] = text
    }

    func isIncluded(_ line: UInt32) -> Bool {
        !excludedLines.contains(line)
    }

    func setIncluded(_ included: Bool, line: UInt32) {
        if included { excludedLines.remove(line) } else { excludedLines.insert(line) }
    }

    /// One override per new row the user changed: ticked off, or given a
    /// category the core would not pick by itself. A suggestion counts as a
    /// change, since only the app knows it; the bank's matched category
    /// does not, since the core applies it anyway. Emptying the field of a
    /// matched row leaves the matched category.
    var overrides: [StatementRowOverride] {
        guard let preview else { return [] }
        return preview.rows.compactMap { row -> StatementRowOverride? in
            guard row.status == .new else { return nil }
            let skip = excludedLines.contains(row.line)
            var category: String?
            if Self.takesCategory(row) {
                let text = categoryText(for: row).trimmingCharacters(in: .whitespacesAndNewlines)
                if !text.isEmpty, text != row.matchedCategory { category = text }
            }
            guard skip || category != nil else { return nil }
            return StatementRowOverride(line: row.line, category: skip ? nil : category, note: nil, skip: skip)
        }
    }

    // MARK: - Import

    /// Imports the rows as previewed with the user's overrides, remembers the
    /// mapping for the next file with this header, and reloads the window.
    func runImport() async {
        guard let input = previewInput, let text, let vaultId, let detection, !isImporting else { return }
        isImporting = true
        defer { isImporting = false }
        do {
            let result = try await store.core.importStatement(
                vaultId: vaultId,
                text: text,
                mapping: input.mapping,
                options: input.options,
                overrides: overrides
            )
            mappings.remember(input.mapping, vaultId: vaultId, headers: detection.headers)
            report = result
            problem = nil
            step = .report
            await store.reload()
        } catch {
            problem = StatementImportProblem(error)
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
