import SwiftUI
import SparagneCore

/// The mapping page: what the file is, where it goes, a few of its rows, and
/// how its columns become transactions (`StatementMapping`). Every change
/// re-runs the preview, whose counts sit under the page.
///
/// Drawn with the pieces every form of the app shares (`FormKit`), flat on
/// the sheet: two columns of groups, with the sample and the type rules
/// across both, since they need the width.
struct StatementMappingEditor: View {
    @Bindable var model: StatementImportModel

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
            FormValueRow(String(localized: "Time zone"), value: model.timezone)
        }
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
                        set: { model.mapping.dateFormat = $0.format(keeping: model.mapping.dateFormat) }
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
                            set: { model.mapping.dateFormat = .custom(pattern: $0) }
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
                    model.mapping.decimalComma = $0
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
            HStack(spacing: 4) {
                Image(systemName: "plus")
                    .font(.system(size: 9, weight: .semibold))
                Text(String(localized: "Add Rule"))
            }
            .font(Face.ui(11, .medium))
            .foregroundStyle(Ink.text2)
            .padding(.horizontal, 8)
            .frame(height: 22)
            .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
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
