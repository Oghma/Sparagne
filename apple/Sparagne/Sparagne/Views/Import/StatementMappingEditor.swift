import SwiftUI
import SparagneCore

/// The mapping page: what the file is, where it goes, a few of its rows, and
/// how its columns become transactions (`StatementMapping`). Every change
/// re-runs the preview, whose counts sit in the footer.
struct StatementMappingEditor: View {
    @Bindable var model: StatementImportModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 12) {
                statement
                target
            }
            sample
            HStack(alignment: .top, spacing: 12) {
                columns
                VStack(alignment: .leading, spacing: 12) {
                    format
                    if model.mapping.statusColumn != nil { statuses }
                }
            }
            types
        }
        .padding(Metrics.gutter)
    }

    // MARK: - The file and the target

    private var statement: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(text: String(localized: "Statement"))
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
                    FieldRow(label: String(localized: "Rows")) {
                        Text((model.detection?.rows).map { "\($0)" } ?? TransactionRow.placeholder)
                    }
                    FieldRow(label: String(localized: "Delimiter")) {
                        Text(StatementText.delimiter(model.detection?.delimiter ?? model.mapping.delimiter))
                    }
                    FieldRow(label: String(localized: "Mapping")) {
                        Text(sourceText).foregroundStyle(Ink.dim)
                    }
                }
            }
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
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(text: String(localized: "Target"))
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
                    FieldRow(label: String(localized: "Wallet")) {
                        Picker(String(localized: "Wallet"), selection: $model.walletId) {
                            Text(String(localized: "Choose…")).tag(Uuid?.none)
                            ForEach(model.store.wallets, id: \.id) { wallet in
                                Text(wallet.name).tag(Uuid?.some(wallet.id))
                            }
                        }
                        .fieldPicker()
                    }
                    FieldRow(label: String(localized: "Envelope")) {
                        Picker(String(localized: "Envelope"), selection: $model.flowId) {
                            ForEach(model.store.flows, id: \.id) { flow in
                                // The core reads no envelope as Unallocated.
                                Text(model.store.flowName(flow)).tag(flow.isUnallocated ? Uuid?.none : Uuid?.some(flow.id))
                            }
                        }
                        .fieldPicker()
                    }
                    FieldRow(label: String(localized: "Time zone")) {
                        Text(model.timezone).foregroundStyle(Ink.dim)
                    }
                }
            }
        }
    }

    // MARK: - The sample

    /// The first rows of the file as they are, so the columns can be told
    /// apart by their content.
    private var sample: some View {
        Panel(padding: 0) {
            VStack(alignment: .leading, spacing: 0) {
                SectionLabel(text: String(localized: "Sample"))
                    .padding(.horizontal, 14)
                    .frame(height: 28)
                Hairline()
                ScrollView(.horizontal) {
                    VStack(alignment: .leading, spacing: 0) {
                        sampleLine(model.detection?.headers ?? [], heading: true)
                        Hairline()
                        ForEach(Array((model.detection?.sample ?? []).enumerated()), id: \.offset) { _, row in
                            sampleLine(row, heading: false)
                        }
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }

    private func sampleLine(_ cells: [String], heading: Bool) -> some View {
        HStack(spacing: 0) {
            ForEach(Array(cells.enumerated()), id: \.offset) { _, cell in
                Group {
                    if heading {
                        SectionLabel(text: cell)
                    } else {
                        Text(cell.trimmingCharacters(in: .whitespaces))
                            .font(Face.footnote)
                            .foregroundStyle(Ink.text)
                    }
                }
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(width: 118, alignment: .leading)
                .padding(.horizontal, ImportMetrics.cellPadding)
                .help(cell)
            }
        }
        .frame(height: 22)
    }

    // MARK: - Columns and format

    private var columns: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(text: String(localized: "Columns"))
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
                    FieldRow(label: String(localized: "Date"), required: true) {
                        requiredColumn(String(localized: "Date"), $model.mapping.dateColumn)
                    }
                    FieldRow(label: String(localized: "Amount"), required: true) {
                        requiredColumn(String(localized: "Amount"), $model.mapping.amountColumn)
                    }
                    FieldRow(label: String(localized: "Description")) { descriptionColumns }
                    FieldRow(label: String(localized: "Type")) {
                        optionalColumn(String(localized: "Type"), $model.mapping.typeColumn)
                    }
                    FieldRow(label: String(localized: "Status")) {
                        optionalColumn(String(localized: "Status"), $model.mapping.statusColumn)
                    }
                    FieldRow(label: String(localized: "Currency")) {
                        optionalColumn(String(localized: "Currency"), $model.mapping.currencyColumn)
                    }
                    FieldRow(label: String(localized: "Bank category")) {
                        optionalColumn(String(localized: "Bank category"), $model.mapping.categoryColumn)
                            .help(String(localized: "The bank’s category only picks one of yours with the same name or alias; it never creates one."))
                    }
                    FieldRow(label: String(localized: "Original amount")) {
                        optionalColumn(String(localized: "Original amount"), $model.mapping.originalAmountColumn)
                    }
                    FieldRow(label: String(localized: "Original currency")) {
                        optionalColumn(String(localized: "Original currency"), $model.mapping.originalCurrencyColumn)
                    }
                }
            }
        }
    }

    private func requiredColumn(_ title: String, _ selection: Binding<String>) -> some View {
        Picker(title, selection: selection) {
            Text(String(localized: "Choose…")).tag("")
            ForEach(model.columns, id: \.self) { Text($0).tag($0) }
        }
        .fieldPicker()
    }

    private func optionalColumn(_ title: String, _ selection: Binding<String?>) -> some View {
        Picker(title, selection: selection) {
            Text(String(localized: "No column")).tag(String?.none)
            ForEach(model.columns, id: \.self) { Text($0).tag(String?.some($0)) }
        }
        .fieldPicker()
    }

    /// Several columns joined with a space make the note, in the order they
    /// were ticked.
    private var descriptionColumns: some View {
        Menu {
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
        } label: {
            Text(
                model.mapping.descriptionColumns.isEmpty
                    ? String(localized: "No column")
                    : model.mapping.descriptionColumns.joined(separator: " + ")
            )
        }
        .fieldPicker()
    }

    private var format: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(text: String(localized: "Format"))
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
                    FieldRow(label: String(localized: "Date format")) {
                        Picker(String(localized: "Date format"), selection: Binding(
                            get: { StatementDateChoice(model.mapping.dateFormat) },
                            set: { model.mapping.dateFormat = $0.format(keeping: model.mapping.dateFormat) }
                        )) {
                            ForEach(StatementDateChoice.allCases) { Text($0.label).tag($0) }
                        }
                        .fieldPicker()
                    }
                    if StatementDateChoice(model.mapping.dateFormat) == .custom {
                        FieldRow(label: String(localized: "Pattern")) {
                            TextField(String(localized: "Pattern"), text: Binding(
                                get: { StatementDateChoice.pattern(of: model.mapping.dateFormat) ?? "" },
                                set: { model.mapping.dateFormat = .custom(pattern: $0) }
                            ))
                            .textFieldStyle(.roundedBorder)
                            .font(Face.row)
                            .frame(width: ImportMetrics.fieldWidth)
                            .help(String(localized: "A strftime pattern, for example:") + " %d.%m.%Y %H:%M")
                        }
                    }
                    FieldRow(label: String(localized: "Amount sign")) {
                        Picker(String(localized: "Amount sign"), selection: $model.mapping.amountSign) {
                            Text(String(localized: "Spending is negative")).tag(AmountSign.outflowNegative)
                            Text(String(localized: "Spending is positive")).tag(AmountSign.outflowPositive)
                        }
                        .fieldPicker()
                    }
                    FieldRow(label: String(localized: "Decimal comma")) {
                        Toggle(String(localized: "Decimal comma"), isOn: $model.mapping.decimalComma)
                            .labelsHidden()
                            .toggleStyle(.checkbox)
                    }
                }
            }
        }
    }

    // MARK: - Types and statuses

    /// A rule per value of the type column, then what every other row does.
    /// Without a type column, the fallback is the only rule.
    private var types: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    SectionLabel(text: String(localized: "Types"))
                    Spacer()
                    if model.mapping.typeColumn != nil { addRuleMenu }
                }
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
                    if model.mapping.typeColumn != nil {
                        ForEach(model.mapping.typeRules.indices, id: \.self) { index in
                            ruleRow(index)
                        }
                    }
                    GridRow {
                        Text(
                            model.mapping.typeColumn == nil
                                ? String(localized: "Every row")
                                : String(localized: "Any other row")
                        )
                        .font(Face.row)
                        .foregroundStyle(Ink.dim)
                        .frame(width: ImportMetrics.fieldWidth, alignment: .leading)
                        actionPicker($model.mapping.defaultAction)
                        transferWalletPicker($model.mapping.defaultAction)
                        Color.clear.frame(width: 16, height: 1)
                    }
                }
            }
        }
    }

    private var addRuleMenu: some View {
        Menu {
            ForEach(model.unruledTypeValues, id: \.self) { value in
                Button(value) { model.addRule(value: value) }
            }
            if !model.unruledTypeValues.isEmpty { Divider() }
            Button(String(localized: "Other Value")) { model.addRule(value: "") }
        } label: {
            Text(String(localized: "Add Rule"))
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .font(Face.footnote)
    }

    @ViewBuilder
    private func ruleRow(_ index: Int) -> some View {
        if model.mapping.typeRules.indices.contains(index) {
            GridRow {
                TextField(String(localized: "value"), text: ruleValue(index))
                    .textFieldStyle(.roundedBorder)
                    .font(Face.row)
                    .frame(width: ImportMetrics.fieldWidth)
                actionPicker(ruleAction(index))
                transferWalletPicker(ruleAction(index))
                Button {
                    model.removeRule(at: index)
                } label: {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.borderless)
                .foregroundStyle(Ink.dim)
                .help(String(localized: "Remove"))
                .frame(width: 16)
            }
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
        Picker(String(localized: "Type"), selection: Binding(
            get: { StatementActionChoice(action.wrappedValue) },
            set: { action.wrappedValue = $0.action(keeping: action.wrappedValue) }
        )) {
            ForEach(StatementActionChoice.allCases) { Text(StatementText.action($0)).tag($0) }
        }
        .fieldPicker()
    }

    /// The other wallet of a transfer; empty for every other action, so the
    /// grid keeps its columns.
    @ViewBuilder
    private func transferWalletPicker(_ action: Binding<StatementAction>) -> some View {
        if StatementActionChoice(action.wrappedValue).isTransfer {
            Picker(String(localized: "Wallet"), selection: Binding(
                get: { action.wrappedValue.counterWalletId },
                set: { action.wrappedValue = action.wrappedValue.withCounterWallet($0) }
            )) {
                Text(String(localized: "Choose…")).tag(Uuid?.none)
                ForEach(model.counterWallets, id: \.id) { wallet in
                    Text(wallet.name).tag(Uuid?.some(wallet.id))
                }
            }
            .fieldPicker()
        } else {
            Color.clear.frame(width: ImportMetrics.fieldWidth, height: 1)
        }
    }

    /// The statuses whose rows are skipped, as removable tokens, with the
    /// sample's other statuses one click away.
    private var statuses: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(text: String(localized: "Skip rows with status"))
                FlowTokens(
                    tokens: model.mapping.skipStatuses,
                    remove: { model.removeSkipStatus($0) }
                )
                let offered = model.sampleValues(of: model.mapping.statusColumn).filter { value in
                    !model.mapping.skipStatuses.contains { $0.lowercased() == value.lowercased() }
                }
                HStack(spacing: 6) {
                    StatusField { model.addSkipStatus($0) }
                    ForEach(offered, id: \.self) { value in
                        Button {
                            model.addSkipStatus(value)
                        } label: {
                            Text(verbatim: "+ \(value)")
                        }
                        .buttonStyle(.borderless)
                            .font(Face.footnote)
                            .foregroundStyle(Ink.dim)
                    }
                }
            }
        }
    }
}

// MARK: - Pieces

extension ImportMetrics {
    /// Every picker and field of the mapping page, so the grids line up.
    static let fieldWidth: CGFloat = 190
    static let labelWidth: CGFloat = 130
}

/// A label and its control, one row of a grid.
private struct FieldRow<Content: View>: View {
    let label: String
    var required = false
    @ViewBuilder var content: Content

    var body: some View {
        GridRow {
            Text(required ? label + " *" : label)
                .font(Face.row)
                .foregroundStyle(Ink.dim)
                .frame(width: ImportMetrics.labelWidth, alignment: .leading)
            content
                .font(Face.row)
                .foregroundStyle(Ink.text)
        }
    }
}

private extension View {
    /// A compact menu picker of the mapping page's standard width.
    func fieldPicker() -> some View {
        self
            .labelsHidden()
            .pickerStyle(.menu)
            .controlSize(.small)
            .frame(width: ImportMetrics.fieldWidth, alignment: .leading)
    }
}

/// Removable tokens in a row that wraps.
private struct FlowTokens: View {
    let tokens: [String]
    let remove: (String) -> Void

    var body: some View {
        if tokens.isEmpty {
            Text(TransactionRow.placeholder)
                .font(Face.row)
                .foregroundStyle(Ink.dim)
        } else {
            HStack(spacing: 6) {
                ForEach(tokens, id: \.self) { token in
                    HStack(spacing: 4) {
                        Text(token)
                        Button {
                            remove(token)
                        } label: {
                            Image(systemName: "xmark").font(.system(size: 8, weight: .bold))
                        }
                        .buttonStyle(.borderless)
                        .help(String(localized: "Remove"))
                    }
                    .font(Face.footnote)
                    .foregroundStyle(Ink.text)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Ink.raised)
                    .overlay(Rectangle().strokeBorder(Ink.line, lineWidth: 1))
                }
            }
        }
    }
}

/// A field that adds what is typed as a token on ↩, then empties.
private struct StatusField: View {
    let add: (String) -> Void
    @State private var text = ""

    var body: some View {
        TextField(String(localized: "status…"), text: $text)
            .textFieldStyle(.roundedBorder)
            .font(Face.footnote)
            .frame(width: 110)
            .onSubmit {
                add(text)
                text = ""
            }
    }
}
