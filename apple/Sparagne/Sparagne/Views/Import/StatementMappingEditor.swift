import SwiftUI
import SparagneCore

/// The mapping page: what the file is, where it goes, a few of its rows, and
/// how its columns become transactions (`StatementMapping`). Every change
/// re-runs the preview, whose counts sit under the page.
///
/// Drawn with the pieces every form of the app shares (`FormKit`), flat on
/// the sheet: two columns of groups, with the sample, the type rules and the
/// description rules across both, since they need the width.
struct StatementMappingEditor: View {
    @Bindable var model: StatementImportModel
    /// The list under a description rule's category field, hung by the
    /// sheet outside the page's scroll view (`hangingCategoryList`).
    let completion: CategoryCompletionModel

    @Environment(\.formMetrics) private var metrics

    /// A column of the sample: wide enough to tell a date from an amount,
    /// narrow enough for most headers to fit without scrolling.
    private static let sampleColumn: CGFloat = 118

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(alignment: .top, spacing: 28) {
                statement.frame(maxWidth: .infinity, alignment: .leading)
                target.frame(maxWidth: .infinity, alignment: .leading)
            }
            if model.mapping.accountColumn != nil { accounts }
            sample
            HStack(alignment: .top, spacing: 28) {
                columns.frame(maxWidth: .infinity, alignment: .leading)
                VStack(alignment: .leading, spacing: 22) {
                    format
                    if model.mapping.statusColumn != nil { statuses }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            types
            descriptionRules
        }
        .padding(.bottom, 4)
    }

    // MARK: - The file and the target

    private var statement: some View {
        FormGroup(String(localized: "Statement")) {
            FormValueRow(
                String(localized: "Rows"),
                value: (model.detection?.rows).map { "\($0)" } ?? TransactionRow.placeholder
            )
            FormValueRow(
                String(localized: "Delimiter"),
                value: StatementText.delimiter(model.detection?.delimiter ?? model.mapping.delimiter)
            )
            FormValueRow(String(localized: "Mapping"), value: sourceText)
        }
    }

    private var sourceText: String {
        switch model.source {
        case .remembered: String(localized: "Remembered from the last import")
        case .preset(let name): String(localized: "Preset: \(name)")
        case .blank: String(localized: "New: choose the columns below")
        }
    }

    private var target: some View {
        let wallets: [Uuid?] = [nil] + model.store.wallets.map { Optional($0.id) }
        // The core reads no envelope as Unallocated.
        let flows: [Uuid?] = model.store.flows.map { $0.isUnallocated ? nil : Optional($0.id) }
        return FormGroup(String(localized: "Target")) {
            FormRow(String(localized: "Wallet")) {
                FormPicker(label: String(localized: "Wallet"), selection: $model.walletId, options: wallets, title: walletName)
            }
            FormRow(String(localized: "Envelope")) {
                FormPicker(label: String(localized: "Envelope"), selection: $model.flowId, options: flows, title: flowName)
            }
            FormRow(String(localized: "Duplicates also in")) { alsoCheck }
            FormNote(String(localized: "Only wallets kept by hand, such as Cash while you move to statements; never one you import a statement into."))
            LookalikeNotes(model: model)
            FormRow(String(localized: "Start day")) { startDay }
            FormValueRow(String(localized: "Time zone"), value: model.timezone)
        }
    }

    /// The other active wallets, any of them ticked: a row entered there by
    /// hand that looks like a statement row is moved into the statement's
    /// wallet rather than imported twice. The statement's own wallet is
    /// always searched, so it is not offered. Never remembered: every file
    /// starts with none ticked.
    private var alsoCheck: some View {
        let others = model.counterWallets
        let checked = others.filter { model.isAlsoChecked($0.id) }.map(\.name)
        return FormMenu(
            label: String(localized: "Also look for duplicates in"),
            value: checked.isEmpty ? String(localized: "No other wallet") : checked.joined(separator: ", ")
        ) {
            ForEach(others, id: \.id) { wallet in
                Toggle(wallet.name, isOn: Binding(
                    get: { model.isAlsoChecked(wallet.id) },
                    set: { model.setAlsoChecked($0, wallet: wallet.id) }
                ))
            }
        }
        .disabled(others.isEmpty)
        .help(String(localized: "A row entered by hand in one of these wallets that looks like a statement row is moved into the statement’s wallet instead of being imported twice. Chosen anew for every file."))
    }

    /// Off, the whole file is read; on, the rows dated before the day are
    /// skipped, so a bank's history from before the vault took it over
    /// stays out.
    private var startDay: some View {
        HStack(spacing: 10) {
            FormSwitch(label: String(localized: "Start day"), isOn: model.since != nil) { on in
                model.since = on ? Self.firstOfThisMonth : nil
            }
            if let since = model.since {
                DayField(
                    label: String(localized: "Start day"),
                    day: Binding(get: { model.since ?? since }, set: { model.since = $0 })
                )
                .frame(width: 150)
            } else {
                Text(String(localized: "Whole file"))
                    .font(Face.ui(metrics.fontSize))
                    .foregroundStyle(Ink.text3)
            }
        }
        .help(String(localized: "Rows dated before this day are not imported."))
    }

    // MARK: - Accounts

    /// Each account of an export holding several, with its rows, and where
    /// they go: the file's wallet, another active wallet, or nowhere.
    private var accounts: some View {
        FormGroup(String(localized: "Statement accounts")) {
            if model.accounts.isEmpty {
                FormNote(String(localized: "No account in this column."), indented: false)
            }
            ForEach(model.accounts, id: \.value) { account in
                HStack(spacing: 8) {
                    Text(verbatim: account.value)
                        .font(Face.ui(metrics.fontSize))
                        .foregroundStyle(Ink.text2)
                        .lineLimit(1)
                        .help(account.value)
                        .frame(width: Self.accountColumn, alignment: .leading)
                    Text(String(localized: "\(Int(account.rows)) rows"))
                        .font(Face.ui(metrics.fontSize - 1))
                        .foregroundStyle(Ink.text3)
                        .frame(width: Self.rowsColumn, alignment: .leading)
                    FormPicker(
                        label: account.value,
                        selection: Binding(
                            get: { AccountPick(model.destination(ofAccount: account.value)) },
                            set: { model.setDestination($0.destination, ofAccount: account.value) }
                        ),
                        options: [.file] + model.store.wallets.map { AccountPick.wallet($0.id) } + [.leftOut],
                        title: accountPickTitle
                    )
                    .frame(width: Self.pickerWidth)
                    Spacer(minLength: 0)
                }
                .frame(minHeight: metrics.controlHeight + 2)
            }
        }
    }

    /// An account's name, its rows and its picker: "Conto Corrente Arancio"
    /// whole, "12 righe", and the wallet names.
    private static let accountColumn: CGFloat = 200
    private static let rowsColumn: CGFloat = 80
    private static let pickerWidth: CGFloat = 300

    /// What an account's picker offers: the file's wallet, any active
    /// wallet, or leaving its rows out.
    private enum AccountPick: Hashable {
        case file
        case wallet(Uuid)
        case leftOut

        init(_ destination: StatementImportModel.AccountDestination?) {
            switch destination {
            case nil: self = .file
            case .wallet(let wallet): self = .wallet(wallet)
            case .leftOut: self = .leftOut
            }
        }

        var destination: StatementImportModel.AccountDestination? {
            switch self {
            case .file: nil
            case .wallet(let wallet): .wallet(wallet)
            case .leftOut: .leftOut
            }
        }
    }

    private func accountPickTitle(_ pick: AccountPick) -> String {
        switch pick {
        case .file:
            let name = model.walletId.map { walletName($0) } ?? TransactionRow.placeholder
            return String(localized: "File’s wallet (\(name))")
        case .wallet(let wallet): return walletName(wallet)
        case .leftOut: return String(localized: "Leave out")
        }
    }

    /// Where a start day switched on begins: the first of today's month.
    private static var firstOfThisMonth: NaiveDate {
        let calendar = Calendar.current
        let start = calendar.date(from: calendar.dateComponents([.year, .month], from: Date())) ?? Date()
        return CoreDate.day(start)
    }

    /// A wallet by id, or the prompt to choose one.
    private func walletName(_ id: Uuid?) -> String {
        guard let id else { return String(localized: "Choose…") }
        return model.store.wallets.first { $0.id == id }?.name ?? TransactionRow.placeholder
    }

    /// An envelope by id; no id is Unallocated.
    private func flowName(_ id: Uuid?) -> String {
        guard let flow = model.store.flows.first(where: { id == nil ? $0.isUnallocated : $0.id == id }) else {
            return id == nil ? NameBook.unallocatedLabel : TransactionRow.placeholder
        }
        return model.store.flowName(flow)
    }

    // MARK: - The sample

    /// The first rows of the file as they are, so the columns can be told
    /// apart by their content. Ruled like the window's tables, and pulled
    /// out by a cell's padding so the first column lines up with the labels.
    private var sample: some View {
        FormGroup(String(localized: "Sample")) {
            ScrollView(.horizontal) {
                VStack(alignment: .leading, spacing: 0) {
                    SetupHeaderRow {
                        ForEach(Array((model.detection?.headers ?? []).enumerated()), id: \.offset) { _, header in
                            sampleCell(header)
                        }
                    }
                    ForEach(Array((model.detection?.sample ?? []).enumerated()), id: \.offset) { _, row in
                        SetupRow {
                            ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
                                sampleCell(cell.trimmingCharacters(in: .whitespaces), whole: cell)
                            }
                        }
                        .font(Face.row)
                        .foregroundStyle(Ink.text)
                    }
                }
            }
            .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
            .padding(.horizontal, -Metrics.cellPad)
        }
    }

    private func sampleCell(_ text: String, whole: String? = nil) -> some View {
        GridCell(width: Self.sampleColumn) { Text(text) }
            .help(whole ?? text)
    }

    // MARK: - Columns and format

    private var columns: some View {
        FormGroup(String(localized: "Columns")) {
            FormRow(Self.required(String(localized: "Date"))) {
                requiredColumn(String(localized: "Date"), $model.mapping.dateColumn)
            }
            FormRow(Self.required(String(localized: "Amount"))) {
                requiredColumn(String(localized: "Amount"), $model.mapping.amountColumn)
            }
            FormRow(String(localized: "Description")) { descriptionColumns }
            FormRow(String(localized: "Type")) {
                optionalColumn(String(localized: "Type"), $model.mapping.typeColumn)
            }
            FormRow(String(localized: "Status")) {
                optionalColumn(String(localized: "Status"), $model.mapping.statusColumn)
            }
            FormRow(String(localized: "Currency")) {
                optionalColumn(String(localized: "Currency"), $model.mapping.currencyColumn)
            }
            FormRow(String(localized: "Bank category")) {
                optionalColumn(String(localized: "Bank category"), $model.mapping.categoryColumn)
                    .help(String(localized: "The bank’s category only picks one of yours with the same name or alias; it never creates one."))
            }
            FormRow(String(localized: "Original amount")) {
                optionalColumn(String(localized: "Original amount"), $model.mapping.originalAmountColumn)
            }
            FormRow(String(localized: "Original currency")) {
                optionalColumn(String(localized: "Original currency"), $model.mapping.originalCurrencyColumn)
            }
            FormRow(String(localized: "Statement account")) {
                optionalColumn(String(localized: "Statement account"), $model.mapping.accountColumn)
                    .help(String(localized: "In an export holding several accounts, the column saying whose each row is."))
            }
            FormRow(String(localized: "Transfer")) {
                optionalColumn(String(localized: "Transfer"), $model.mapping.transferColumn)
                    .help(String(localized: "The column marking the two halves of a move between two of your accounts."))
            }
        }
    }

    /// The two columns the core cannot do without carry a star.
    private static func required(_ label: String) -> String {
        label + " *"
    }

    private func requiredColumn(_ title: String, _ selection: Binding<String>) -> some View {
        FormPicker(label: title, selection: selection, options: [""] + model.columns) { column in
            column.isEmpty ? String(localized: "Choose…") : column
        }
    }

    private func optionalColumn(_ title: String, _ selection: Binding<String?>) -> some View {
        FormPicker(label: title, selection: selection, options: [nil] + model.columns.map { Optional($0) }) { column in
            column ?? String(localized: "No column")
        }
    }

    /// Several columns joined with a space make the note, in the order they
    /// were ticked.
    private var descriptionColumns: some View {
        FormMenu(
            label: String(localized: "Description"),
            value: model.mapping.descriptionColumns.isEmpty
                ? String(localized: "No column")
                : model.mapping.descriptionColumns.joined(separator: " + ")
        ) {
            ForEach(model.columns, id: \.self) { column in
                Toggle(column, isOn: Binding(
                    get: { model.mapping.descriptionColumns.contains(column) },
                    set: { on in
                        if on {
                            model.mapping.descriptionColumns.append(column)
                        } else {
                            model.mapping.descriptionColumns.removeAll { $0 == column }
                        }
                    }
                ))
            }
        }
    }

    private var format: some View {
        FormGroup(String(localized: "Format")) {
            FormRow(String(localized: "Date format")) {
                FormPicker(
                    label: String(localized: "Date format"),
                    selection: Binding(
                        get: { StatementDateChoice(model.mapping.dateFormat) },
                        set: { model.chooseDateFormat($0.format(keeping: model.mapping.dateFormat)) }
                    ),
                    options: StatementDateChoice.allCases,
                    title: { $0.label }
                )
            }
            if StatementDateChoice(model.mapping.dateFormat) == .custom {
                FormRow(String(localized: "Pattern")) {
                    FormTextField(
                        label: String(localized: "Pattern"),
                        text: Binding(
                            get: { StatementDateChoice.pattern(of: model.mapping.dateFormat) ?? "" },
                            set: { model.chooseDateFormat(.custom(pattern: $0)) }
                        )
                    )
                    .help(String(localized: "A strftime pattern, for example:") + " %d.%m.%Y %H:%M")
                }
            }
            FormRow(String(localized: "Amount sign")) {
                FormPicker(
                    label: String(localized: "Amount sign"),
                    selection: $model.mapping.amountSign,
                    options: [AmountSign.outflowNegative, .outflowPositive],
                    title: Self.signLabel
                )
            }
            FormRow(String(localized: "Decimal comma")) {
                FormSwitch(label: String(localized: "Decimal comma"), isOn: model.mapping.decimalComma) {
                    model.chooseDecimalComma($0)
                }
            }
        }
    }

    private static func signLabel(_ sign: AmountSign) -> String {
        switch sign {
        case .outflowNegative: String(localized: "Spending is negative")
        case .outflowPositive: String(localized: "Spending is positive")
        }
    }

    // MARK: - Types and statuses

    /// A rule per value of the type column, then what every other row does.
    /// Without a type column, the fallback is the only rule. A rule reads as
    /// a form row whose label is the value it matches, so the values line up
    /// with the labels of the groups above.
    private var types: some View {
        VStack(alignment: .leading, spacing: metrics.rowSpacing) {
            HStack(spacing: 8) {
                FormGroupLabel(String(localized: "Types"))
                Spacer(minLength: 8)
                if model.mapping.typeColumn != nil { addRuleMenu }
            }
            if model.mapping.typeColumn != nil {
                ForEach(model.mapping.typeRules.indices, id: \.self) { index in
                    ruleRow(index)
                }
            }
            HStack(spacing: 8) {
                Text(
                    model.mapping.typeColumn == nil
                        ? String(localized: "Every row")
                        : String(localized: "Any other row")
                )
                .font(Face.ui(metrics.fontSize))
                .foregroundStyle(Ink.text2)
                .frame(width: metrics.labelWidth, alignment: .leading)
                actionPicker($model.mapping.defaultAction)
                transferWalletPicker($model.mapping.defaultAction)
                Color.clear.frame(width: Self.removeWidth, height: 1)
            }
            .frame(minHeight: metrics.controlHeight + 2)
        }
    }

    /// The remove button's track, kept empty on the fallback row so its
    /// pickers stay under the rules' ones.
    private static let removeWidth: CGFloat = 20

    /// A small ghost button that opens the menu, drawn by hand as
    /// `FormMenu` draws its box.
    private var addRuleMenu: some View {
        Menu {
            ForEach(model.unruledTypeValues, id: \.self) { value in
                Button(value) { model.addRule(value: value) }
            }
            if !model.unruledTypeValues.isEmpty { Divider() }
            Button(String(localized: "Other Value")) { model.addRule(value: "") }
        } label: {
            addRuleLabel
                .font(Face.label)
                .foregroundStyle(Ink.text2)
                .padding(.horizontal, 8)
                .frame(height: 22)
                .contentShape(Rectangle())
        }
        .plainMenu()
        .fixedSize()
    }

    private var addRuleLabel: some View {
        HStack(spacing: 4) {
            Image(systemName: "plus")
                .font(.system(size: 9, weight: .semibold))
            Text(String(localized: "Add Rule"))
        }
    }

    @ViewBuilder
    private func ruleRow(_ index: Int) -> some View {
        if model.mapping.typeRules.indices.contains(index) {
            HStack(spacing: 8) {
                FormTextField(label: String(localized: "value"), text: ruleValue(index), prompt: String(localized: "value"))
                    .frame(width: metrics.labelWidth)
                actionPicker(ruleAction(index))
                transferWalletPicker(ruleAction(index))
                SetupIconButton(symbol: "xmark", help: String(localized: "Remove"), label: String(localized: "Remove")) {
                    model.removeRule(at: index)
                }
                .frame(width: Self.removeWidth)
            }
            .frame(minHeight: metrics.controlHeight + 2)
        }
    }

    /// Bindings into a rule by position that tolerate the rule going away:
    /// a field still focused while its row is removed reads and writes
    /// nothing instead of trapping.
    private func ruleValue(_ index: Int) -> Binding<String> {
        Binding(
            get: { model.mapping.typeRules.indices.contains(index) ? model.mapping.typeRules[index].value : "" },
            set: { if model.mapping.typeRules.indices.contains(index) { model.mapping.typeRules[index].value = $0 } }
        )
    }

    private func ruleAction(_ index: Int) -> Binding<StatementAction> {
        Binding(
            get: { model.mapping.typeRules.indices.contains(index) ? model.mapping.typeRules[index].action : .skip },
            set: { if model.mapping.typeRules.indices.contains(index) { model.mapping.typeRules[index].action = $0 } }
        )
    }

    private func actionPicker(_ action: Binding<StatementAction>) -> some View {
        FormPicker(
            label: String(localized: "Type"),
            selection: Binding(
                get: { StatementActionChoice(action.wrappedValue) },
                set: { action.wrappedValue = $0.action(keeping: action.wrappedValue) }
            ),
            options: StatementActionChoice.allCases,
            title: StatementText.action
        )
    }

    /// The other wallet of a transfer; an empty slot as wide for every other
    /// action, so the rows keep their columns.
    @ViewBuilder
    private func transferWalletPicker(_ action: Binding<StatementAction>) -> some View {
        if StatementActionChoice(action.wrappedValue).isTransfer {
            FormPicker(
                label: String(localized: "Wallet"),
                selection: Binding(
                    get: { action.wrappedValue.counterWalletId },
                    set: { action.wrappedValue = action.wrappedValue.withCounterWallet($0) }
                ),
                options: [nil] + model.counterWallets.map { Optional($0.id) },
                title: walletName
            )
        } else {
            Color.clear.frame(height: 1)
        }
    }

    /// The statuses whose rows are skipped, as removable chips, with the
    /// sample's other statuses one click away.
    private var statuses: some View {
        FormGroup(String(localized: "Skip rows with status")) {
            StatusChips(
                tokens: model.mapping.skipStatuses,
                remove: { model.removeSkipStatus($0) }
            )
            let offered = model.sampleValues(of: model.mapping.statusColumn).filter { value in
                !model.mapping.skipStatuses.contains { $0.lowercased() == value.lowercased() }
            }
            HStack(spacing: 6) {
                StatusField { model.addSkipStatus($0) }
                    .frame(width: 140)
                ForEach(offered, id: \.self) { value in
                    Button {
                        model.addSkipStatus(value)
                    } label: {
                        Text(verbatim: "+ \(value)")
                    }
                    .buttonStyle(.chrome(.ghost, small: true))
                }
            }
        }
    }

    // MARK: - Description rules

    /// Rules on the bank's text, tried in order before the types: the first
    /// whose text the description holds decides the row's kind, category
    /// and note. A line per rule, its fields under the type rules' columns.
    private var descriptionRules: some View {
        VStack(alignment: .leading, spacing: metrics.rowSpacing) {
            HStack(spacing: 8) {
                FormGroupLabel(String(localized: "Description rules"))
                Spacer(minLength: 8)
                Button {
                    model.addDescriptionRule()
                } label: {
                    addRuleLabel
                }
                .buttonStyle(.chrome(.ghost, small: true))
                .fixedSize()
            }
            if model.mapping.descriptionRules.isEmpty {
                FormNote(
                    String(localized: "A rule finds the rows whose description holds its text, case aside, and says what they are, their category and a cleaner note, before the types do. The first rule that matches decides."),
                    indented: false
                )
            } else {
                ForEach(model.mapping.descriptionRules.indices, id: \.self) { index in
                    DescriptionRuleEditor(model: model, index: index, layout: .line, completion: completion)
                }
            }
        }
    }
}

// MARK: - Wallets worth searching

/// The core's advice to search more wallets for duplicates
/// (`StatementImportModel.lookalikes`): a line per wallet saying how many
/// new rows look like entries there, with a button that ticks it under
/// "Duplicates also in". Under that picker on the mapping page, and on the
/// review page; gone once the wallet is ticked.
struct LookalikeNotes: View {
    let model: StatementImportModel
    /// Under a form's control column, or across the page.
    var indented = true

    @Environment(\.formMetrics) private var metrics

    var body: some View {
        let lookalikes = model.lookalikes
        if !lookalikes.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(lookalikes, id: \.wallet.id) { lookalike in
                    HStack(spacing: 8) {
                        Image(systemName: "info.circle")
                            .font(.system(size: metrics.fontSize - 2))
                            .foregroundStyle(Ink.text3)
                            .accessibilityHidden(true)
                        // The wallet's name sits after "in" in both
                        // languages, so the sentence is built around it
                        // rather than carry a plural with two arguments.
                        Text(verbatim: String(localized: "\(lookalike.rows) rows also look like entries in")
                            + " \(lookalike.wallet.name): "
                            + String(localized: "tick it under Duplicates also in"))
                            .font(Face.ui(metrics.fontSize - 1))
                            .foregroundStyle(Ink.text2)
                            .fixedSize(horizontal: false, vertical: true)
                        Button(String(localized: "Tick \(lookalike.wallet.name)")) {
                            model.setAlsoChecked(true, wallet: lookalike.wallet.id)
                        }
                        .buttonStyle(.chrome(.ghost, small: true))
                        .fixedSize()
                        // Not while a sync round or the import runs: the
                        // plan on screen is about to be compared or written.
                        .disabled(model.isLocked)
                    }
                }
            }
            .padding(.leading, indented ? metrics.labelWidth + 8 : 0)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

// MARK: - A description rule

/// One description rule, edited in place: the text to look for, what the
/// rows it finds are ("Keep the type" leaves that to the type rules), the
/// other wallet of a transfer or the category of an entry, the note that
/// replaces the bank's text, and a way to remove it.
///
/// A line under the type rules on the mapping page, with the rows of the
/// file it decides; a small form in the review's popover
/// (`StatementReview`), opened on a rule made from a row.
struct DescriptionRuleEditor: View {
    enum Layout { case line, form }

    @Bindable var model: StatementImportModel
    let index: Int
    let layout: Layout
    /// The list under the category field: hung by the page around a line,
    /// drawn under the field in the form.
    let completion: CategoryCompletionModel

    @Environment(\.formMetrics) private var metrics

    /// The remove button's track, as the type rules have it.
    private static let removeWidth: CGFloat = 20
    /// "12 righe" with room to spare.
    private static let countWidth: CGFloat = 60

    var body: some View {
        if model.mapping.descriptionRules.indices.contains(index) {
            switch layout {
            case .line: line
            case .form: form
            }
        }
    }

    private var line: some View {
        HStack(spacing: 8) {
            containsField.frame(width: metrics.labelWidth)
            actionPicker
            detail
            noteField
            Text(String(localized: "\(model.matchCount(ofRule: index)) rows"))
                .font(Face.ui(metrics.fontSize - 1))
                .foregroundStyle(Ink.text3)
                .lineLimit(1)
                .frame(width: Self.countWidth, alignment: .trailing)
                .help(String(localized: "Rows of this file the rule decides"))
            SetupIconButton(symbol: "xmark", help: String(localized: "Remove"), label: String(localized: "Remove")) {
                model.removeDescriptionRule(at: index)
            }
            .frame(width: Self.removeWidth)
        }
        .frame(minHeight: metrics.controlHeight + 2)
    }

    private var form: some View {
        VStack(alignment: .leading, spacing: metrics.rowSpacing) {
            FormRow(String(localized: "Contains")) { containsField }
            FormRow(String(localized: "Kind")) { actionPicker }
            if isTransfer {
                FormRow(String(localized: "Wallet")) { walletPicker }
            } else if takesCategory {
                FormRow(String(localized: "Category")) { categoryField }
                if completion.isOpen {
                    CategoryCompletionList(completion: completion)
                        .padding(.leading, metrics.labelWidth + 8)
                }
            }
            FormRow(String(localized: "Note")) { noteField }
            FormNote(String(localized: "\(model.matchCount(ofRule: index)) rows"))
        }
    }

    // MARK: Fields

    private var containsField: some View {
        FormTextField(
            label: String(localized: "Contains"),
            text: Binding(
                get: { rule?.contains ?? "" },
                set: { new in update { $0.contains = new } }
            ),
            prompt: String(localized: "contains…")
        )
        .help(String(localized: "Found anywhere in the bank’s description, case aside"))
    }

    /// "Keep the type" first: the rule then only cleans the note or picks
    /// the category, and the type rules still say what the row is.
    private var actionPicker: some View {
        FormPicker(
            label: String(localized: "Kind"),
            selection: Binding(
                get: { rule?.action.map(StatementActionChoice.init) },
                set: { choice in
                    let kept = currentAction
                    update { rule in rule.action = choice.map { picked in picked.action(keeping: kept) } }
                }
            ),
            options: [nil] + StatementActionChoice.allCases.map { Optional($0) },
            title: { $0.map(StatementText.action) ?? String(localized: "Keep the type") }
        )
    }

    /// Beside the kind: the other wallet of a transfer, the category of an
    /// entry, nothing for a skip; an empty slot keeps the columns.
    @ViewBuilder
    private var detail: some View {
        if isTransfer {
            walletPicker
        } else if takesCategory {
            categoryField
        } else {
            Color.clear.frame(maxWidth: .infinity, maxHeight: 1)
        }
    }

    private var walletPicker: some View {
        FormPicker(
            label: String(localized: "Wallet"),
            selection: Binding(
                get: { rule?.action?.counterWalletId },
                set: { wallet in update { $0.action = $0.action?.withCounterWallet(wallet) } }
            ),
            options: [nil] + model.counterWallets.map { Optional($0.id) },
            title: walletName
        )
    }

    /// Its own owner, so that several rules can each open the list under
    /// their own field.
    private var categoryField: some View {
        CategoryFormField(
            label: String(localized: "Category"),
            text: Binding(
                get: { rule?.category ?? "" },
                set: { new in update { $0.category = new.trimmingCharacters(in: .whitespaces).isEmpty ? nil : new } }
            ),
            store: model.store,
            completion: completion,
            owner: "rule-\(index)",
            prompt: String(localized: "category…")
        )
    }

    private var noteField: some View {
        FormTextField(
            label: String(localized: "Note"),
            text: Binding(
                get: { rule?.note ?? "" },
                set: { new in update { $0.note = new.trimmingCharacters(in: .whitespaces).isEmpty ? nil : new } }
            ),
            prompt: String(localized: "note…")
        )
        .help(String(localized: "Replaces the bank’s text as the note; empty keeps it"))
    }

    // MARK: The rule

    /// The rule by position, `nil` once it is gone: a field still focused
    /// while its rule is removed reads and writes nothing instead of
    /// trapping.
    private var rule: StatementDescriptionRule? {
        model.mapping.descriptionRules.indices.contains(index) ? model.mapping.descriptionRules[index] : nil
    }

    private func update(_ change: (inout StatementDescriptionRule) -> Void) {
        guard model.mapping.descriptionRules.indices.contains(index) else { return }
        change(&model.mapping.descriptionRules[index])
    }

    private var currentAction: StatementAction { rule?.action ?? .skip }

    private var isTransfer: Bool {
        rule?.action.map { StatementActionChoice($0).isTransfer } ?? false
    }

    /// "Keep the type" may well leave an entry, so it keeps the field.
    private var takesCategory: Bool {
        rule?.action.map(StatementImportModel.takesCategory) ?? true
    }

    private func walletName(_ id: Uuid?) -> String {
        guard let id else { return String(localized: "Choose…") }
        return model.store.wallets.first { $0.id == id }?.name ?? TransactionRow.placeholder
    }
}

// MARK: - Pieces

/// The skipped statuses as chips in a row, each with its own remove button.
private struct StatusChips: View {
    let tokens: [String]
    let remove: (String) -> Void

    var body: some View {
        if tokens.isEmpty {
            Text(TransactionRow.placeholder)
                .font(Face.row)
                .foregroundStyle(Ink.text3)
        } else {
            HStack(spacing: 6) {
                ForEach(tokens, id: \.self) { token in
                    HStack(spacing: 4) {
                        Text(token)
                            .foregroundStyle(Ink.text)
                        Button {
                            remove(token)
                        } label: {
                            Image(systemName: "xmark")
                                .font(.system(size: 8, weight: .bold))
                                .foregroundStyle(Ink.text3)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .help(String(localized: "Remove"))
                        .accessibilityLabel(String(localized: "Remove"))
                    }
                    .font(Face.ui(11.5))
                    .padding(.horizontal, 7)
                    .frame(height: 20)
                    .background(Ink.raised, in: RoundedRectangle(cornerRadius: 4))
                    .fixedSize()
                }
            }
        }
    }
}

/// A field that adds what is typed as a chip on ↩, then empties.
private struct StatusField: View {
    let add: (String) -> Void
    @State private var text = ""

    var body: some View {
        FormTextField(label: String(localized: "status…"), text: $text, prompt: String(localized: "status…"))
            .onSubmit {
                add(text)
                text = ""
            }
    }
}
