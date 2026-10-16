import SwiftUI
import SparagneCore

/// The review page: every row of the file with what the core would do with
/// it, and what the user can change about it before anything is written.
/// A new row can be ticked off (it goes to the core as `skip`), given
/// another note, category (with the list the ledger's CATEGORY cell offers)
/// or envelope; any row the rules decided can be made another kind,
/// another wallet's transfer among them; and a row that matched a
/// transaction already in the vault says which, with what becomes of it.
/// A category typed here may create one, the bank's never does
/// (`core/src/statement/mod.rs`).
///
/// A kind, an envelope or a choice over a match re-runs the preview, since
/// the core decides what the row then is; a tick, a note or a category is
/// counted by the model alone (`StatementImportModel.PreviewInput`).
///
/// Ruled like the window's tables (`SetupHeaderRow`, `SetupRow`,
/// `GridCell`), and pulled out by a cell's padding so the first column
/// lines up with the sheet's title.
struct StatementReview: View {
    @Bindable var model: StatementImportModel
    /// The list under the CATEGORY field being typed into, one for the
    /// page: the sheet holds it, since its footer's keys yield to the list
    /// while it is open.
    let completion: CategoryCompletionModel

    var body: some View {
        VStack(spacing: 0) {
            header
            ScrollView {
                LazyVStack(spacing: 0) {
                    let names = NameBook(snapshot: model.store.snapshot)
                    ForEach(model.preview?.rows ?? [], id: \.line) { row in
                        StatementReviewRow(model: model, row: row, names: names, completion: completion)
                    }
                }
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        // Hung as the grid hangs it, three points in from the cell's edge.
        .hangingCategoryList(completion, inset: 3 - GridColumn.padding)
        .padding(.horizontal, -Metrics.cellPad)
    }

    private var header: some View {
        SetupHeaderRow {
            GridCell(width: ReviewColumn.check) { Color.clear.frame(height: 1) }
            GridCell(width: ReviewColumn.line, alignment: .trailing) {
                heading("#", spoken: String(localized: "Number"))
            }
            GridCell(width: ReviewColumn.date) { heading(String(localized: "Date")) }
            GridCell { heading(String(localized: "Description")) }
            GridCell(width: ReviewColumn.amount, alignment: .trailing) { heading(String(localized: "Amount")) }
            GridCell(width: ReviewColumn.kind) { heading(String(localized: "Kind")) }
            GridCell(width: ReviewColumn.status) { heading(String(localized: "Status")) }
            GridCell(width: ReviewColumn.category) { heading(String(localized: "Category")) }
            GridCell(width: ReviewColumn.envelope) { heading(String(localized: "Envelope")) }
        }
    }

    /// A column heading, announced as one, and `#` as a word.
    private func heading(_ text: String, spoken: String? = nil) -> some View {
        Text(text)
            .accessibilityLabel(spoken ?? text)
            .accessibilityAddTraits(.isHeader)
    }
}

/// Column widths of the review, as a cell's content gets them (`GridCell`
/// pads each side); the description takes what is left, about 225 points
/// of the sheet. Date fits a whole `2026-09-16`, kind its menu's longest
/// word ("trasferimento verso") and status the tag's ("Needs the other
/// wallet" in its 10 pt); a longer tag is cut, its whole text on hover.
private enum ReviewColumn {
    static let check: CGFloat = 14
    static let line: CGFloat = 28
    static let date: CGFloat = 76
    static let amount: CGFloat = 76
    static let kind: CGFloat = 118
    static let status: CGFloat = 140
    static let category: CGFloat = 136
    static let envelope: CGFloat = 102

    /// The columns before the description, padding included: where the
    /// second line of a flagged row starts.
    static let lead = check + line + date + 6 * GridColumn.padding
    /// The second line's menu spans the status and category columns, room
    /// for "Tieni quella già presente".
    static let choice = status + category + 2 * GridColumn.padding
    /// The envelope column, padding included, which the second line leaves
    /// empty.
    static let trail = envelope + 2 * GridColumn.padding
}

/// One previewed row. Rows that will not be imported are drawn in the dim
/// ink, their outcome tag excepted. A row that matched a transaction
/// already in the vault gets a second, dim line under it: that
/// transaction, and a menu of what becomes of the row.
private struct StatementReviewRow: View {
    @Bindable var model: StatementImportModel
    let row: StatementRow
    let names: NameBook
    let completion: CategoryCompletionModel

    @FocusState private var categoryFocused: Bool
    /// The rule "Make a Rule from This Row…" made, while its popover is open.
    @State private var ruleIndex: Int?

    private var isNew: Bool { row.status == .new }
    private var included: Bool { model.isIncluded(row.line) }
    /// Written if the user goes ahead: a new row left ticked, or a transfer
    /// replacing the entry it matched.
    private var live: Bool { (isNew && included) || replacing }
    private var replacing: Bool { StatementImportModel.replaces(row) }
    /// What the row is taken for, the user's kind before the preview has
    /// caught up with it.
    private var action: StatementAction { model.action(for: row) }
    /// The transaction already in the vault the row matched, if it is to be
    /// shown: a flagged row's, or one imported anyway.
    private var match: TransactionView? { StatementImportModel.isFlagged(row) ? row.existing : nil }

    var body: some View {
        VStack(spacing: 0) {
            SetupRow(separator: match == nil) { cells }
            if let match { matchLine(match) }
        }
        .font(Face.row)
        .contextMenu {
            Button(String(localized: "Make a Rule from This Row…")) {
                ruleIndex = model.makeRule(from: row)
            }
            .disabled(row.payee.isEmpty)
        }
        .popover(
            isPresented: Binding(get: { ruleIndex != nil }, set: { if !$0 { ruleIndex = nil } }),
            arrowEdge: .bottom
        ) {
            if let ruleIndex {
                DescriptionRulePopover(model: model, index: ruleIndex) { self.ruleIndex = nil }
            }
        }
    }

    @ViewBuilder
    private var cells: some View {
        GridCell(width: ReviewColumn.check) {
            if isNew {
                IncludeBox(isOn: included) { model.setIncluded($0, line: row.line) }
            } else {
                // An empty cell still holds its width.
                Color.clear.frame(height: 1)
            }
        }
        GridCell(width: ReviewColumn.line, alignment: .trailing) {
            Text(verbatim: String(row.line)).foregroundStyle(Ink.text3)
        }
        GridCell(width: ReviewColumn.date) { Text(verbatim: date).foregroundStyle(ink) }
        GridCell { description }
        GridCell(width: ReviewColumn.amount, alignment: .trailing) {
            Text(verbatim: LedgerMoney.bare(signedAmount)).foregroundStyle(live ? amountTint : Ink.text3)
        }
        GridCell(width: ReviewColumn.kind) { kind }
        GridCell(width: ReviewColumn.status) {
            OutcomeTag(text: statusText, tone: tone)
                .help(StatementText.detail(row.status) ?? statusText)
        }
        GridCell(width: ReviewColumn.category) { category }
        GridCell(width: ReviewColumn.envelope) { envelope }
    }

    private var ink: Color { live ? Ink.text : Ink.text3 }

    private var date: String {
        guard let stamp = row.occurredAt, let date = CoreDate.date(stamp) else { return TransactionRow.placeholder }
        return CoreDate.day(date)
    }

    /// Expenses and transfers out minus, everything else plus, as the
    /// ledger's totals sign them.
    private var signedAmount: Int64 {
        if row.kind == .expense { return -row.amount }
        if row.kind == .transferWallet, case .transferOut = row.action { return -row.amount }
        return row.amount
    }

    private var amountTint: Color {
        switch row.kind {
        case .expense: Ink.negative
        case .income, .refund: Ink.positive
        default: Ink.text
        }
    }

    /// A new row the user ticked off reads as the core will count it, and a
    /// row replacing the entry it matched as one that will be written.
    private var statusText: String {
        if replacing { return String(localized: "To replace") }
        return isNew && !included ? StatementText.skipped("skipped_by_you") : StatementText.status(row.status)
    }

    private var tone: OutcomeTag.Tone {
        if replacing { return .new }
        return switch row.status {
        case .new: included ? .new : .quiet
        case .alreadyImported, .possibleDuplicate, .matchedTransfer, .skipped: .quiet
        case .invalid: .invalid
        }
    }

    // MARK: Description

    /// The note the row will get, editable on a row that will be imported:
    /// a description rule's or the bank's text until the user types another.
    /// The bank's own text is on hover. Emptied, the field shows in the dim
    /// ink the note the row keeps.
    @ViewBuilder
    private var description: some View {
        HStack(spacing: 6) {
            if live {
                let typed = Binding(
                    get: { model.noteText(for: row) },
                    set: { model.setNote($0, for: row.line) }
                )
                ZStack(alignment: .leading) {
                    if model.noteText(for: row).isEmpty {
                        Text(verbatim: row.note.isEmpty ? TransactionRow.placeholder : row.note)
                            .foregroundStyle(Ink.text3)
                            .allowsHitTesting(false)
                    }
                    TextField(String(localized: "Note"), text: typed, prompt: Text(verbatim: ""))
                        .textFieldStyle(.plain)
                        .foregroundStyle(ink)
                }
            } else {
                let note = model.noteText(for: row)
                Text(verbatim: note.isEmpty ? TransactionRow.placeholder : note)
                    .foregroundStyle(ink)
            }
            if let original = row.original {
                Text(verbatim: original).foregroundStyle(Ink.text3)
            }
        }
        .help(row.payee)
    }

    // MARK: Kind

    /// A menu on any row the rules decided: expense, income, refund, a
    /// transfer either way, or skip. `BySign` reads as the kind it resolved
    /// to. A row already in the vault or unreadable has nothing to choose.
    @ViewBuilder
    private var kind: some View {
        if StatementImportModel.takesAction(row) {
            CellPicker(
                label: String(localized: "Kind"),
                selection: Binding(
                    get: { choice },
                    set: { model.setAction($0.action(keeping: action), for: row) }
                ),
                options: StatementActionChoice.rowChoices,
                title: StatementText.action,
                value: StatementText.rowKind(action, resolved: row.kind),
                tint: ink
            )
        } else {
            Text(verbatim: StatementText.kind(row.kind)).foregroundStyle(ink)
        }
    }

    /// The menu's ticked item: a `BySign` as the kind it resolved to.
    private var choice: StatementActionChoice {
        if action == .bySign { return row.outflow ? .expense : .income }
        return StatementActionChoice(action)
    }

    // MARK: Category

    /// The other wallet of a transfer, as a menu; on a new entry the
    /// category, completing as the ledger's CATEGORY cell does
    /// (`completing`): typing opens the list, ↑↓ move in it, ↩ and ⇥ pick,
    /// esc closes it; the row's category, dimmed, on a row that will not be
    /// imported. A transfer already there names, dimmed, the other wallet of
    /// the transfer it matched: there is nothing to choose while it stands
    /// for the row.
    ///
    /// The placeholder of an empty field is drawn here in the dim ink, not
    /// left to the field: the field's own prompt takes the row's text ink
    /// and would read as a category.
    @ViewBuilder
    private var category: some View {
        if row.status == .matchedTransfer, let existing = row.existing {
            let other = existing.fromId == model.walletId ? existing.toId : existing.fromId
            Text(verbatim: "\u{21C4} \(walletName(other))").foregroundStyle(Ink.text3)
        } else if StatementImportModel.takesAction(row), StatementActionChoice(action).isTransfer {
            CellPicker(
                label: String(localized: "Wallet"),
                selection: Binding(
                    get: { action.counterWalletId },
                    set: { model.setAction(action.withCounterWallet($0), for: row) }
                ),
                options: [nil] + model.counterWallets.map { Optional($0.id) },
                title: walletName,
                value: action.counterWalletId.map { "\u{21C4} \(walletName($0))" } ?? walletName(nil),
                tint: action.counterWalletId == nil ? Ink.text2 : ink
            )
        } else if StatementImportModel.takesCategory(row) {
            let text = model.categoryText(for: row)
            let typed = Binding(
                get: { model.categoryText(for: row) },
                set: { model.setCategory($0, for: row.line) }
            )
            ZStack(alignment: .leading) {
                if text.isEmpty {
                    Text(String(localized: "category…"))
                        .foregroundStyle(Ink.text3)
                        .allowsHitTesting(false)
                }
                TextField(String(localized: "Category"), text: typed, prompt: Text(verbatim: ""))
                    .textFieldStyle(.plain)
                    .foregroundStyle(ink)
                    .focused($categoryFocused)
                    .completing(text: typed, owner: row.line, isFocused: categoryFocused, store: model.store, completion: completion)
                    .categoryListAnchor(owner: row.line, isFocused: categoryFocused, store: model.store, completion: completion)
            }
            .disabled(!included)
        } else if row.kind == .transferWallet, let wallet = names.wallet(row.counterWalletId) {
            Text(verbatim: "\u{21C4} \(wallet)").foregroundStyle(Ink.text3)
        } else {
            Text(verbatim: row.category ?? row.matchedCategory ?? "").foregroundStyle(Ink.text3)
        }
    }

    private func walletName(_ id: Uuid?) -> String {
        guard let id else { return String(localized: "Choose…") }
        return names.wallet(id) ?? TransactionRow.placeholder
    }

    // MARK: Envelope

    /// A new entry's envelope, as a menu of the vault's active ones, the
    /// file's unless the user picks another; dimmed on an entry that will
    /// not be imported; nothing on a transfer, which moves no envelope.
    @ViewBuilder
    private var envelope: some View {
        if StatementImportModel.takesCategory(row) {
            CellPicker(
                label: String(localized: "Envelope"),
                selection: Binding(
                    get: { model.flow(for: row) },
                    set: { if let flow = $0 { model.setFlow(flow, for: row) } }
                ),
                options: model.store.flows.map { Optional($0.id) },
                title: flowName,
                tint: ink
            )
            .disabled(!included)
        } else if row.kind == .income || row.kind == .expense || row.kind == .refund {
            Text(verbatim: flowName(model.flow(for: row))).foregroundStyle(Ink.text3)
        } else {
            Color.clear.frame(height: 1)
        }
    }

    private func flowName(_ id: Uuid?) -> String {
        names.flow(id) ?? NameBook.unallocatedLabel
    }

    // MARK: The match

    /// `↳ 2026-09-14 · Bar Centrale · Bar · in Cash` under the description,
    /// and under the status the menu of what becomes of the row: replace
    /// the entry with the transfer the row is (only an entry standing for
    /// it, which the line then names by its kind: `income in Revolut`),
    /// keep the transaction already there for it, move that one here (only
    /// an entry typed into another wallet), or import the row anyway. A row
    /// imported by default, its match more likely another export's row,
    /// says so after the match.
    private func matchLine(_ existing: TransactionView) -> some View {
        let text = StatementText.existing(existing, names: names, replacing: StatementImportModel.isReplace(row))
        let chosen = model.matchChoice(for: row) ?? model.defaultChoice(for: row)
        let hint = model.importsByDefault(row) ? StatementText.importedByDefault : nil
        return HStack(spacing: 0) {
            Color.clear.frame(width: ReviewColumn.lead, height: 1)
            GridCell {
                HStack(spacing: 5) {
                    Image(systemName: "arrow.turn.down.right")
                        .font(.system(size: 9))
                        .accessibilityHidden(true)
                    Text(verbatim: text)
                    if let hint {
                        // Its whole length before the match's text gets any.
                        Text(verbatim: "\u{00B7} \(hint)")
                            .italic()
                            .layoutPriority(1)
                    }
                }
                .foregroundStyle(Ink.text3)
                .help(hint.map { "\(text)\n\($0)" } ?? text)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(String(localized: "Already in the vault: \(text)"))
                .accessibilityHint(hint ?? "")
            }
            GridCell(width: ReviewColumn.choice) {
                CellPicker(
                    label: String(localized: "Matching transaction"),
                    selection: Binding(
                        get: { chosen },
                        set: { model.setMatchChoice($0, for: row) }
                    ),
                    options: model.availableChoices(for: row),
                    title: StatementText.matchChoice,
                    tint: chosen == .importAnyway ? Ink.accent : Ink.text2
                )
                .help(StatementText.matchChoiceDetail(chosen))
                .accessibilityHint(StatementText.matchChoiceDetail(chosen))
            }
            Color.clear.frame(width: ReviewColumn.trail, height: 1)
        }
        .font(Face.ui(11))
        .frame(height: 20)
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.rowLine).frame(height: 1) }
    }
}

/// A cell that opens a menu of `options`, the chosen one ticked: its value
/// in the row's ink with a small chevron after it, so it reads as the
/// review's other cells and still says it opens. `value` is what the cell
/// shows when it says something else than the item's title.
private struct CellPicker<Option: Hashable>: View {
    let label: String
    @Binding var selection: Option
    let options: [Option]
    let title: (Option) -> String
    var value: String?
    var tint: Color = Ink.text

    var body: some View {
        let shown = value ?? title(selection)
        Menu {
            Picker(label, selection: $selection) {
                ForEach(options, id: \.self) { option in
                    Text(title(option)).tag(option)
                }
            }
            .pickerStyle(.inline)
            .labelsHidden()
        } label: {
            HStack(spacing: 3) {
                Text(verbatim: shown)
                    .foregroundStyle(tint)
                    .lineLimit(1)
                Image(systemName: "chevron.down")
                    .font(.system(size: 7, weight: .semibold))
                    .foregroundStyle(Ink.text3)
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        // As `FormMenu`: the plain style keeps the label as drawn.
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .accessibilityLabel(label)
        .accessibilityValue(shown)
    }
}

/// "Make a Rule from This Row…": the rule the model just made from the row
/// (`StatementImportModel.makeRule`), in the same editor as the mapping
/// page's rules, with its own CATEGORY list drawn under the field. Every
/// change re-runs the preview, so the row and the others the rule finds
/// change under the popover; Remove Rule takes it back.
private struct DescriptionRulePopover: View {
    @Bindable var model: StatementImportModel
    let index: Int
    let close: () -> Void

    @State private var completion = CategoryCompletionModel()

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionLabel(text: String(localized: "Rule for rows like this one"))
            DescriptionRuleEditor(model: model, index: index, layout: .form, completion: completion)
            HStack(spacing: 8) {
                FormDestructiveButton(title: String(localized: "Remove Rule"), small: true) {
                    model.removeDescriptionRule(at: index)
                    close()
                }
                Spacer(minLength: 8)
                Button(String(localized: "Done"), action: close)
                    .buttonStyle(.chrome(.bordered))
                    .keyboardShortcut(completion.isOpen ? nil : .defaultAction)
            }
        }
        .padding(14)
        .frame(width: 400)
        .background(Ink.card)
        .environment(\.formMetrics, .inspector)
        .preferredColorScheme(.dark)
    }
}

/// What the core will do with a row, as a small rounded tag: soft accent on
/// a row that will be imported, `raised` and dim on one that will not
/// (already in the vault, matched to a transaction there, skipped by a rule,
/// its status or the user), soft negative on one the core cannot read. The
/// ledger's and setup's tags, so the three tables speak the same way; the
/// dim one is cut rather than spilling out of its column.
private struct OutcomeTag: View {
    enum Tone { case new, quiet, invalid }

    let text: String
    let tone: Tone

    var body: some View {
        switch tone {
        case .new: RowTag(text: text, tint: Ink.accent)
        case .quiet:
            Text(text)
                .font(Face.ui(10, .semibold))
                .foregroundStyle(Ink.text3)
                .lineLimit(1)
                .padding(.horizontal, 5)
                .frame(height: 16)
                .background(Ink.raised, in: RoundedRectangle(cornerRadius: 4))
        case .invalid: RowTag(text: text, tint: Ink.negative)
        }
    }
}

/// The exclude box of a new row, drawn by hand like the form's switch: the
/// accent with a dark tick while the row goes in, an outline once it is
/// ticked off. VoiceOver gets the system checkbox it stands for, named
/// "Import" as before.
private struct IncludeBox: View {
    let isOn: Bool
    /// On the main actor, as everything here, and said in the type so the
    /// `Binding` VoiceOver's checkbox writes through can take it.
    let set: @MainActor (Bool) -> Void

    var body: some View {
        Button {
            set(!isOn)
        } label: {
            ZStack {
                RoundedRectangle(cornerRadius: 3)
                    .fill(isOn ? Ink.accent : Ink.card)
                RoundedRectangle(cornerRadius: 3)
                    .strokeBorder(isOn ? Ink.accent : Ink.line2, lineWidth: 1)
                if isOn {
                    Image(systemName: "checkmark")
                        .font(.system(size: 8, weight: .bold))
                        .foregroundStyle(Color(hex: 0x140A00))
                }
            }
            .frame(width: 13, height: 13)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityRepresentation {
            Toggle(String(localized: "Import"), isOn: Binding(get: { isOn }, set: set))
        }
    }
}

/// What the import did: the counts, then every row the core refused, with
/// its line and the reason in the user's language. The rows matched to a
/// transaction already in the vault, the transactions moved and the entries
/// replaced by their transfers get a figure and a sentence only when there
/// are any.
///
/// The core counts a transfer that replaced an entry as executed and its
/// row as a possible duplicate; `replaced`, how many the model sent so and
/// the core did not refuse, takes them out of both and shows them on their
/// own.
struct StatementReportView: View {
    let report: StatementReport
    var replaced = 0

    private var imported: UInt32 { report.executed - min(report.executed, UInt32(replaced)) }
    private var duplicates: UInt32 { report.possibleDuplicates - min(report.possibleDuplicates, UInt32(replaced)) }

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            FormGroup(String(localized: "Import finished")) {
                Panel {
                    HStack(alignment: .top, spacing: 0) {
                        figure(String(localized: "Imported"), imported, Ink.positive)
                        figure(String(localized: "Already imported"), report.deduplicated, Ink.text)
                        figure(String(localized: "Skipped"), report.skipped, Ink.text)
                        if duplicates > 0 {
                            figure(String(localized: "Possible duplicates"), duplicates, Ink.text)
                        }
                        if report.matchedTransfers > 0 {
                            figure(String(localized: "Transfers already there"), report.matchedTransfers, Ink.text)
                        }
                        if report.moved > 0 {
                            figure(String(localized: "Moved"), report.moved, Ink.text)
                        }
                        if replaced > 0 {
                            figure(String(localized: "Replaced"), UInt32(replaced), Ink.positive)
                        }
                        figure(String(localized: "Rounded"), report.rounded, Ink.text)
                        figure(String(localized: "Refused"), UInt32(report.rejected.count), Ink.negative)
                    }
                }
                if duplicates > 0 {
                    FormNote(
                        String(localized: "\(Int(duplicates)) rows looked like a transaction already in the vault and were not imported."),
                        indented: false
                    )
                }
                if replaced > 0 {
                    FormNote(
                        String(localized: "\(replaced) transactions were deleted and replaced by the transfers they stood for."),
                        indented: false
                    )
                }
                if report.moved > 0 {
                    FormNote(
                        String(localized: "\(Int(report.moved)) transactions entered by hand were moved into the statement’s wallet."),
                        indented: false
                    )
                }
                if report.matchedTransfers > 0 {
                    FormNote(
                        String(localized: "\(Int(report.matchedTransfers)) transfers were already there from the other wallet’s side and were not imported again."),
                        indented: false
                    )
                }
                if report.rounded > 0 {
                    FormNote(String(localized: "Rounded rows had more decimals than the currency keeps."), indented: false)
                }
            }
            if !report.rejected.isEmpty {
                FormGroup(String(localized: "Refused")) {
                    VStack(spacing: 0) {
                        ForEach(Array(report.rejected.enumerated()), id: \.offset) { _, rejection in
                            SetupRow {
                                GridCell(width: Self.lineColumn) {
                                    Text(String(localized: "Line \(Int(rejection.line))"))
                                        .foregroundStyle(Ink.text3)
                                }
                                GridCell {
                                    HStack(spacing: 10) {
                                        Text(ErrorMessages.summary(for: rejection.code))
                                            .foregroundStyle(Ink.negative)
                                            .layoutPriority(1)
                                        Text(verbatim: rejection.message)
                                            .foregroundStyle(Ink.text3)
                                            .help(rejection.message)
                                    }
                                }
                            }
                        }
                    }
                    .font(Face.row)
                    .padding(.horizontal, -Metrics.cellPad)
                }
            }
        }
        .padding(.bottom, 4)
    }

    /// "Riga 1234" with room to spare.
    private static let lineColumn: CGFloat = 70

    /// A figure of the report, as the summary's cards show one: the label
    /// over the number, the number dimmed at zero. With all eight figures
    /// out, a long label ("Trasferimenti già presenti") shrinks a little
    /// rather than wrap and push its number below the others.
    private func figure(_ label: String, _ value: UInt32, _ tint: Color) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(Face.label)
                .foregroundStyle(Ink.text3)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Text(verbatim: String(value))
                .font(Face.display)
                .foregroundStyle(value == 0 ? Ink.text3 : tint)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}
