import SwiftUI
import SparagneCore

/// Column widths, fixed so the header, the rows and the empty line agree without a layout pass.
private enum EnvelopeColumn {
    static let nameMinimum: CGFloat = 100
    static let type: CGFloat = 104
    static let cap: CGFloat = 84
    static let negative: CGFloat = 44
    static let balance: CGFloat = 120
}

/// The vault's envelopes as a table edited by rows (`editsRows`); the last line adds one.
struct EnvelopeTable: View {
    @Bindable var store: AppStore

    @State private var rows = RowEditing(EnvelopeDraft())
    @State private var hovered: Uuid?
    @FocusState private var focus: EnvelopeCell?

    var body: some View {
        SetupCard(title: String(localized: "Flows"), hint: String(localized: "one for each purpose: the house, the holidays, …")) {
            SetupTableBody {
                header
                ForEach(store.flows.filter { matchesFilter($0.name) }, id: \.id) { flow in
                    activeRow(flow)
                }
                // A viewer reads the envelopes; nothing to add.
                if store.canWrite { newLineRow }
                archived
            }
        }
        .editsRows($rows, focus: $focus, hovered: hovered, store: store) { commit($0) }
    }

    /// The top bar's search; the empty line stays.
    private func matchesFilter(_ name: String) -> Bool {
        TableFilter.matches(store.tabFilter, [name])
    }

    private var header: some View {
        SetupHeaderRow {
            GridCell { Text(String(localized: "Name")) }
                .frame(minWidth: EnvelopeColumn.nameMinimum)
            GridCell(width: EnvelopeColumn.type) { Text(String(localized: "Cap type")) }
            GridCell(width: EnvelopeColumn.cap, alignment: .trailing) { Text(String(localized: "Limit")) }
            GridCell(width: EnvelopeColumn.negative) { Text(String(localized: "Neg.")) }
            GridCell(width: EnvelopeColumn.balance, alignment: .trailing) {
                Text(String(localized: "Balance"))
            }
            GridCell(width: SetupColumn.action) { EmptyView() }
        }
    }

    // MARK: - An active envelope

    @ViewBuilder
    private func activeRow(_ flow: FlowView) -> some View {
        let isEditing = rows.row == flow.id
        SetupRow(highlighted: isEditing || hovered == flow.id, editing: isEditing) {
            if isEditing {
                editingCells(flow)
            } else {
                displayCells(flow)
            }
        }
        .hoverId($hovered, flow.id)
        .onEscape(when: isEditing) { rows.cancel($focus) }
        .contextMenu {
            // Unallocated is a system envelope: the core refuses to update or
            // archive it.
            if !flow.isUnallocated, store.canWrite {
                Button(String(localized: "Archive"), role: .destructive) {
                    Task { await store.archiveEnvelope(flow.id) }
                }
            }
        }
    }

    /// A closed row: every cell is a click target that opens the row there.
    /// The system envelope reads in `text2` and carries the "system" tag; an
    /// archived one is `dimmed`, its name struck through, and opens nowhere.
    @ViewBuilder
    private func displayCells(_ flow: FlowView, dimmed: Bool = false) -> some View {
        let system = flow.isUnallocated
        let secondary = dimmed ? Ink.text3 : Ink.text2
        GridCell {
            HStack(spacing: 6) {
                rowText(store.flowName(flow), tint: dimmed ? Ink.text3 : system ? Ink.text2 : Ink.text)
                    .strikethrough(dimmed)
                if system { SetupTag(text: String(localized: "system")) }
            }
        }
        .frame(minWidth: EnvelopeColumn.nameMinimum)
        .onTapGesture { open(flow, at: .name) }
        GridCell(width: EnvelopeColumn.type) {
            if system {
                dash
            } else {
                rowText(EnvelopeCapKind(flow.mode).label, tint: secondary)
            }
        }
        .onTapGesture { open(flow, at: .type) }
        GridCell(width: EnvelopeColumn.cap, alignment: .trailing) {
            if let cap = EnvelopeCapKind.cap(of: flow.mode), !system {
                rowText(LedgerMoney.bare(cap), tint: secondary)
            } else {
                dash
            }
        }
        .onTapGesture { open(flow, at: .cap) }
        GridCell(width: EnvelopeColumn.negative) {
            if system {
                dash
            } else {
                rowText(Self.label(negative: flow.allowNegative), tint: secondary)
            }
        }
        .onTapGesture { open(flow, at: .negative) }
        balanceCell(flow, dimmed: dimmed)
            .onTapGesture { open(flow, at: .name) }
        // Only on a closed row, so it never fights the open row's save on
        // leaving. An archived row has its own, which restores.
        if !dimmed {
            SetupArchiveCell(name: flow.name, shown: !system && hovered == flow.id && store.canWrite) {
                Task { await store.archiveEnvelope(flow.id) }
            }
        }
    }

    /// The open row. The cap of an unlimited envelope stays editable: typing
    /// an amount and picking a type in either order must work.
    @ViewBuilder
    private func editingCells(_ flow: FlowView) -> some View {
        GridCell {
            SetupTextField(
                placeholder: "", text: $rows.draft.name,
                focus: $focus, key: EnvelopeCell(row: flow.id, field: .name), submit: commitFocusedLine
            )
        }
        .frame(minWidth: EnvelopeColumn.nameMinimum)
        typeCell($rows.draft)
        GridCell(width: EnvelopeColumn.cap, alignment: .trailing) {
            SetupTextField(
                placeholder: String(localized: "cap…"), text: $rows.draft.cap, alignment: .trailing,
                focus: $focus, key: EnvelopeCell(row: flow.id, field: .cap), submit: commitFocusedLine
            )
        }
        negativeCell($rows.draft)
        balanceCell(flow)
        GridCell(width: SetupColumn.action) { EmptyView() }
    }

    /// The balance, and under it the 2 pt bar of a capped envelope: how much
    /// of the cap it has used, asked of `FundGauge` as the summary's gauges
    /// are. A `dimmed` one has no bar.
    private func balanceCell(_ flow: FlowView, dimmed: Bool = false) -> some View {
        let gauge = flow.isUnallocated || dimmed
            ? nil
            : FundGauge(id: flow.id, name: flow.name, mode: flow.mode, balance: flow.balance, incomeTotal: flow.incomeTotal)
        return GridCell(width: EnvelopeColumn.balance, alignment: .trailing) {
            VStack(alignment: .trailing, spacing: 2) {
                Text(LedgerMoney.amount(flow.balance))
                    .font(Face.row)
                    .foregroundStyle(dimmed ? Ink.text3 : Self.tint(balance: flow.balance))
                if let gauge { CapBar(fraction: gauge.fraction) }
            }
        }
    }

    private var archived: some View {
        ForEach(store.archivedFlows.filter { matchesFilter($0.name) }, id: \.id) { flow in
            SetupArchivedRow(name: flow.name, id: flow.id, hovered: $hovered, canWrite: store.canWrite) {
                Task { await store.restoreEnvelope(flow.id) }
            } cells: {
                displayCells(flow, dimmed: true)
            }
        }
    }

    // MARK: - The empty line

    /// Always under the active envelopes: fill it left to right and press ↩.
    /// SALDO is the opening allocation moved out of Unallocated,
    /// which the placeholder says.
    private var newLineRow: some View {
        let active = focus?.row == nil && focus != nil
        return SetupRow(highlighted: active, editing: active) {
            GridCell {
                SetupTextField(
                    placeholder: String(localized: "New flow…"), text: $rows.newLine.name,
                    focus: $focus, key: EnvelopeCell(row: nil, field: .name), submit: commitFocusedLine
                )
            }
            .frame(minWidth: EnvelopeColumn.nameMinimum)
            typeCell($rows.newLine, emptyLabel: String(localized: "No cap"))
            GridCell(width: EnvelopeColumn.cap, alignment: .trailing) {
                SetupTextField(
                    placeholder: "—", text: $rows.newLine.cap, alignment: .trailing,
                    focus: $focus, key: EnvelopeCell(row: nil, field: .cap), submit: commitFocusedLine
                )
            }
            negativeCell($rows.newLine)
            GridCell(width: EnvelopeColumn.balance, alignment: .trailing) {
                SetupTextField(
                    placeholder: String(localized: "allocation"), text: $rows.newLine.allocation, alignment: .trailing,
                    focus: $focus, key: EnvelopeCell(row: nil, field: .allocation), submit: commitFocusedLine
                )
            }
            GridCell(width: SetupColumn.action) { EmptyView() }
        }
        .onEscape { rows.cancelNewLine($focus) }
    }

    // MARK: - Shared cells

    /// TIPO is a menu, not text: the three kinds are the whole vocabulary
    /// and a typo has no meaning here. On the empty
    /// line an unchosen kind reads as "No cap" rather than "None".
    private func typeCell(_ line: Binding<EnvelopeDraft>, emptyLabel: String? = nil) -> some View {
        GridCell(width: EnvelopeColumn.type) {
            Menu {
                ForEach(EnvelopeCapKind.allCases) { kind in
                    Button(kind.label) { line.wrappedValue.kind = kind }
                }
            } label: {
                Text(line.wrappedValue.kind == .none ? (emptyLabel ?? EnvelopeCapKind.none.label) : line.wrappedValue.kind.label)
                    .font(Face.row)
                    .foregroundStyle(emptyLabel != nil && line.wrappedValue.kind == .none ? Ink.text3 : Ink.text)
            }
            .menuStyle(.borderlessButton)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// NEG is one click, as the mockup writes it: no menu for two values.
    private func negativeCell(_ line: Binding<EnvelopeDraft>) -> some View {
        GridCell(width: EnvelopeColumn.negative) {
            Button {
                line.wrappedValue.allowNegative.toggle()
            } label: {
                Text(Self.label(negative: line.wrappedValue.allowNegative))
                    .font(Face.row)
                    .foregroundStyle(Ink.text2)
            }
            .buttonStyle(.plain)
        }
    }

    private func rowText(_ value: String, tint: Color = Ink.text) -> some View {
        Text(value).font(Face.row).foregroundStyle(tint)
    }

    private var dash: some View {
        Text(verbatim: "—").font(Face.row).foregroundStyle(Ink.text3)
    }

    private static func label(negative: Bool) -> String {
        negative ? String(localized: "yes") : String(localized: "no")
    }

    /// A zero reads as background, an overdrawn envelope as a warning.
    private static func tint(balance: Int64) -> Color {
        if balance == 0 { return Ink.text3 }
        return balance < 0 ? Ink.negative : Ink.text
    }

    // MARK: - Actions

    /// Opens `flow` with the caret in `field`, saving whatever row was open
    /// before. A row that refuses to save keeps the focus, so the click that
    /// would have left it does not lose what was typed.
    ///
    /// TIPO and NEG hold no text field, so they cannot take the focus: a click
    /// on NEG toggles the draft straight away (one click, as the mockup
    /// promises) and a click on TIPO leaves the caret on NOME with the menu
    /// one click away.
    private func open(_ flow: FlowView, at field: EnvelopeField) {
        guard !flow.isUnallocated, !flow.archived, store.canWrite else { return }
        if let other = rows.other(than: flow.id), !commit(other) { return }
        rows.open(flow.id) { EnvelopeDraft(flow: flow) }
        if field == .negative { rows.draft.allowNegative.toggle() }
        focus = EnvelopeCell(row: flow.id, field: field.caret)
    }

    private func commitFocusedLine() {
        rows.submit($focus, commit: commit) { Task { await commitNewLine() } }
    }

    /// Writes the draft back and says whether it went through. Only the cells
    /// that changed travel, so two people editing different envelopes of the
    /// same vault do not conflict.
    @discardableResult
    private func commit(_ flowId: Uuid) -> Bool {
        guard rows.row == flowId else { return true }
        guard let flow = store.flows.first(where: { $0.id == flowId }) else {
            // Archived, or gone from under the row by a sync while it was open.
            rows.row = nil
            return true
        }
        do {
            let patch = try rows.draft.patch(against: flow, currency: store.currency)
            rows.row = nil
            // The row closes now: the draft was already checked against the
            // cells, and the write itself belongs to the core actor.
            if !patch.isEmpty {
                Task {
                    await store.updateEnvelope(
                        flowId,
                        name: patch.name,
                        mode: patch.mode,
                        allowNegative: patch.allowNegative
                    )
                }
            }
            return true
        } catch let failure as EnvelopeDraftError {
            store.report(failure.underlying)
            focus = EnvelopeCell(row: flowId, field: failure.field)
            return false
        } catch {
            store.report(error)
            return false
        }
    }

    /// ↩ on the empty line. A refused name (a duplicate, an allocation larger
    /// than Unallocated) surfaces through the store's alert and the line keeps
    /// its text, so the mistake can be fixed instead of retyped.
    private func commitNewLine() async {
        do {
            guard let entry = try rows.newLine.envelope(currency: store.currency) else { return }
            let created = await store.createEnvelope(
                name: entry.name,
                mode: entry.mode,
                allowNegative: entry.allowNegative,
                openingAllocation: entry.allocation
            )
            if created {
                rows.clearNewLine()
            }
            focus = EnvelopeCell(row: nil, field: .name)
        } catch let failure as EnvelopeDraftError {
            store.report(failure.underlying)
            focus = EnvelopeCell(row: nil, field: failure.field)
        } catch {
            store.report(error)
        }
    }
}

/// A text cell of the envelope table: only the text cells can hold a caret.
typealias EnvelopeCell = SetupFocus<EnvelopeField>

/// The cells a line is typed into. `allocation` is the empty line's SALDO,
/// which is the opening allocation and exists nowhere else.
enum EnvelopeField: Hashable, CaseIterable {
    case name
    case type
    case cap
    case negative
    case allocation

    /// Where the caret goes when this cell is clicked. TIPO and NEG are a
    /// menu and a button, so they send it to NOME.
    var caret: EnvelopeField {
        switch self {
        case .type, .negative: .name
        default: self
        }
    }
}

// MARK: - The three kinds of cap

/// `FlowMode` as the TIPO column reads it: which kind of cap, with the amount apart.
enum EnvelopeCapKind: String, CaseIterable, Identifiable {
    case none
    case net
    case income

    var id: String { rawValue }

    var label: String {
        switch self {
        case .none: String(localized: "None")
        case .net: String(localized: "On balance")
        case .income: String(localized: "On income")
        }
    }

    var isCapped: Bool { self != .none }

    init(_ mode: FlowMode) {
        switch mode {
        case .unlimited: self = .none
        case .netCapped: self = .net
        case .incomeCapped: self = .income
        }
    }

    func mode(cap: Int64) -> FlowMode {
        switch self {
        case .none: .unlimited
        case .net: .netCapped(cap: cap)
        case .income: .incomeCapped(cap: cap)
        }
    }

    static func cap(of mode: FlowMode) -> Int64? {
        switch mode {
        case .unlimited: nil
        case .netCapped(let cap), .incomeCapped(let cap): cap
        }
    }
}

// MARK: - The draft behind an edited line

/// A cell that refused what was typed. The table reports `underlying`, which
/// carries the core's own message, and puts the focus back on `field`.
struct EnvelopeDraftError: Error {
    let field: EnvelopeField
    let underlying: Error
}

/// What only an `UpdateFlow` needs to say: the fields that actually changed.
struct EnvelopePatch: Equatable {
    var name: String?
    var mode: FlowMode?
    var allowNegative: Bool?

    var isEmpty: Bool { name == nil && mode == nil && allowNegative == nil }
}

/// The text and the choices in the cells of the line being edited, strings until it commits.
struct EnvelopeDraft: Equatable {
    var name = ""
    var kind: EnvelopeCapKind = .none
    var cap = ""
    var allowNegative = false
    /// Only the empty line has one: the opening allocation from Unallocated.
    var allocation = ""

    init() {}

    init(flow: FlowView) {
        name = flow.name
        kind = EnvelopeCapKind(flow.mode)
        cap = EnvelopeCapKind.cap(of: flow.mode).map(LedgerMoney.editable) ?? ""
        allowNegative = flow.allowNegative
    }

    /// The diff against the envelope as it is stored: only the cells that
    /// changed. An emptied NOME leaves the name alone, the way the ledger
    /// treats an emptied amount: clearing a cell is not a way to say "nothing".
    func patch(against flow: FlowView, currency: Currency) throws -> EnvelopePatch {
        var patch = EnvelopePatch()

        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty, trimmed != flow.name { patch.name = trimmed }

        let mode = try resolvedMode(currency: currency)
        if mode != flow.mode { patch.mode = mode }

        if allowNegative != flow.allowNegative { patch.allowNegative = allowNegative }
        return patch
    }

    /// The fields of a new envelope, resolved and parsed. `nil` when NOME is
    /// still blank: ↩ on an empty line is not an error, it is nothing.
    func envelope(
        currency: Currency
    ) throws -> (name: String, mode: FlowMode, allowNegative: Bool, allocation: Int64)? {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let mode = try resolvedMode(currency: currency)
        let opening = try amount(allocation, in: .allocation, currency: currency) ?? 0
        return (name: trimmed, mode: mode, allowNegative: allowNegative, allocation: opening)
    }

    /// TIPO and TETTO read together: a capped kind without an amount, or an
    /// amount the core's parser refuses, is a mistake in the TETTO cell.
    func resolvedMode(currency: Currency) throws -> FlowMode {
        guard kind.isCapped else { return .unlimited }
        guard let parsed = try amount(cap, in: .cap, currency: currency) else {
            throw EnvelopeDraftError(
                field: .cap,
                underlying: DomainError.InvalidCommand(
                    message: String(localized: "A capped flow needs a cap amount")
                )
            )
        }
        return kind.mode(cap: parsed)
    }

    /// Money as the core parses it; `nil` for an empty cell. Failures are
    /// tagged with the cell they came from so the table can send the focus
    /// back there.
    private func amount(_ text: String, in field: EnvelopeField, currency: Currency) throws -> Int64? {
        do {
            return try LedgerMoney.cell(text, currency: currency)
        } catch {
            throw EnvelopeDraftError(field: field, underlying: error)
        }
    }
}
