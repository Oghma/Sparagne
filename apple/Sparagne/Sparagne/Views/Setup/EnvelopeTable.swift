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

    /// Which envelope is open for editing; the empty line is `nil` and lives
    /// in `newLine` instead, so the two drafts never overwrite each other.
    @State private var editing: Uuid?
    @State private var draft = EnvelopeDraft()
    @State private var newLine = EnvelopeDraft()
    @State private var hovered: Uuid?
    @FocusState private var focus: EnvelopeCell?

    var body: some View {
        SetupCard(title: String(localized: "Envelopes"), hint: String(localized: "one for each purpose: the house, the holidays, …")) {
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
        .editsRows(editing: $editing, focus: $focus, hovered: hovered, store: store) {
            newLine = EnvelopeDraft()
        } commit: { commit($0) }
    }

    /// The top bar's search; the empty line stays.
    private func matchesFilter(_ name: String) -> Bool {
        TableFilter.matches(store.tabFilter, [name])
    }

    private var header: some View {
        SetupHeaderRow {
            SetupCell { Text(String(localized: "Name")) }
                .frame(minWidth: EnvelopeColumn.nameMinimum)
            SetupCell(width: EnvelopeColumn.type) { Text(String(localized: "Cap type")) }
            SetupCell(width: EnvelopeColumn.cap, alignment: .trailing) { Text(String(localized: "Limit")) }
            SetupCell(width: EnvelopeColumn.negative) { Text(String(localized: "Neg.")) }
            SetupCell(width: EnvelopeColumn.balance, alignment: .trailing) {
                Text(String(localized: "Balance"))
            }
            SetupCell(width: SetupColumn.action) { EmptyView() }
        }
    }

    // MARK: - An active envelope

    @ViewBuilder
    private func activeRow(_ flow: FlowView) -> some View {
        let isEditing = editing == flow.id
        SetupRow(highlighted: isEditing || hovered == flow.id, editing: isEditing) {
            if isEditing {
                editingCells(flow)
            } else {
                displayCells(flow)
            }
        }
        .hoverId($hovered, flow.id)
        .onEscape(when: isEditing, perform: cancel)
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
    /// The system envelope reads in `text2` and carries the "system" tag.
    @ViewBuilder
    private func displayCells(_ flow: FlowView) -> some View {
        let system = flow.isUnallocated
        SetupCell {
            HStack(spacing: 6) {
                rowText(store.flowName(flow), tint: system ? Ink.text2 : Ink.text)
                if system { SetupTag(text: String(localized: "system")) }
            }
        }
        .frame(minWidth: EnvelopeColumn.nameMinimum)
        .onTapGesture { open(flow, at: .name) }
        SetupCell(width: EnvelopeColumn.type) {
            if system {
                dash
            } else {
                rowText(EnvelopeCapKind(flow.mode).label, tint: Ink.text2)
            }
        }
        .onTapGesture { open(flow, at: .type) }
        SetupCell(width: EnvelopeColumn.cap, alignment: .trailing) {
            if let cap = EnvelopeCapKind.cap(of: flow.mode), !system {
                rowText(LedgerMoney.bare(cap), tint: Ink.text2)
            } else {
                dash
            }
        }
        .onTapGesture { open(flow, at: .cap) }
        SetupCell(width: EnvelopeColumn.negative) {
            if system {
                dash
            } else {
                rowText(Self.label(negative: flow.allowNegative), tint: Ink.text2)
            }
        }
        .onTapGesture { open(flow, at: .negative) }
        balanceCell(flow)
            .onTapGesture { open(flow, at: .name) }
        // Only on a closed row, so it never fights the open row's save on
        // leaving.
        SetupArchiveCell(name: flow.name, shown: !system && hovered == flow.id && store.canWrite) {
            Task { await store.archiveEnvelope(flow.id) }
        }
    }

    /// The open row. The cap of an unlimited envelope stays editable: typing
    /// an amount and picking a type in either order must work.
    @ViewBuilder
    private func editingCells(_ flow: FlowView) -> some View {
        SetupCell {
            SetupTextField(
                placeholder: "", text: $draft.name,
                focus: $focus, key: EnvelopeCell(row: flow.id, field: .name), submit: commitFocusedLine
            )
        }
        .frame(minWidth: EnvelopeColumn.nameMinimum)
        typeCell($draft)
        SetupCell(width: EnvelopeColumn.cap, alignment: .trailing) {
            SetupTextField(
                placeholder: String(localized: "cap…"), text: $draft.cap, alignment: .trailing,
                focus: $focus, key: EnvelopeCell(row: flow.id, field: .cap), submit: commitFocusedLine
            )
        }
        negativeCell($draft)
        balanceCell(flow)
        SetupCell(width: SetupColumn.action) { EmptyView() }
    }

    /// The balance, and under it the 2 pt bar of a capped envelope: how much
    /// of the cap it has used (`EnvelopeCapFill`).
    private func balanceCell(_ flow: FlowView) -> some View {
        let fill = flow.isUnallocated
            ? nil
            : EnvelopeCapFill.fraction(mode: flow.mode, balance: flow.balance, incomeTotal: flow.incomeTotal)
        return SetupCell(width: EnvelopeColumn.balance, alignment: .trailing) {
            VStack(alignment: .trailing, spacing: 2) {
                Text(LedgerMoney.amount(flow.balance))
                    .font(Face.row)
                    .foregroundStyle(Self.tint(balance: flow.balance))
                    .strikethrough(flow.archived)
                if let fill { CapBar(fraction: fill) }
            }
        }
    }

    private var archived: some View {
        ForEach(store.archivedFlows.filter { matchesFilter($0.name) }, id: \.id) { flow in
            SetupArchivedRow(name: flow.name, id: flow.id, hovered: $hovered, canWrite: store.canWrite) {
                Task { await store.restoreEnvelope(flow.id) }
            } cells: {
                SetupCell {
                    Text(flow.name).font(Face.row).foregroundStyle(Ink.text3).strikethrough()
                }
                .frame(minWidth: EnvelopeColumn.nameMinimum)
                SetupCell(width: EnvelopeColumn.type) {
                    Text(EnvelopeCapKind(flow.mode).label).font(Face.row).foregroundStyle(Ink.text3)
                }
                SetupCell(width: EnvelopeColumn.cap, alignment: .trailing) {
                    Text(EnvelopeCapKind.cap(of: flow.mode).map(LedgerMoney.bare) ?? "—")
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                }
                SetupCell(width: EnvelopeColumn.negative) {
                    Text(Self.label(negative: flow.allowNegative)).font(Face.row).foregroundStyle(Ink.text3)
                }
                SetupCell(width: EnvelopeColumn.balance, alignment: .trailing) {
                    Text(LedgerMoney.amount(flow.balance)).font(Face.row).foregroundStyle(Ink.text3)
                }
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
            SetupCell {
                SetupTextField(
                    placeholder: String(localized: "New envelope…"), text: $newLine.name,
                    focus: $focus, key: EnvelopeCell(row: nil, field: .name), submit: commitFocusedLine
                )
            }
            .frame(minWidth: EnvelopeColumn.nameMinimum)
            typeCell($newLine, emptyLabel: String(localized: "No cap"))
            SetupCell(width: EnvelopeColumn.cap, alignment: .trailing) {
                SetupTextField(
                    placeholder: "—", text: $newLine.cap, alignment: .trailing,
                    focus: $focus, key: EnvelopeCell(row: nil, field: .cap), submit: commitFocusedLine
                )
            }
            negativeCell($newLine)
            SetupCell(width: EnvelopeColumn.balance, alignment: .trailing) {
                SetupTextField(
                    placeholder: String(localized: "allocation"), text: $newLine.allocation, alignment: .trailing,
                    focus: $focus, key: EnvelopeCell(row: nil, field: .allocation), submit: commitFocusedLine
                )
            }
            SetupCell(width: SetupColumn.action) { EmptyView() }
        }
        .onEscape {
            newLine = EnvelopeDraft()
            focus = nil
        }
    }

    // MARK: - Shared cells

    /// TIPO is a menu, not text: the three kinds are the whole vocabulary
    /// and a typo has no meaning here. On the empty
    /// line an unchosen kind reads as "No cap" rather than "None".
    private func typeCell(_ line: Binding<EnvelopeDraft>, emptyLabel: String? = nil) -> some View {
        SetupCell(width: EnvelopeColumn.type) {
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
        SetupCell(width: EnvelopeColumn.negative) {
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
        guard !flow.isUnallocated, store.canWrite else { return }
        if editing != flow.id {
            if let editing, !commit(editing) { return }
            editing = flow.id
            draft = EnvelopeDraft(flow: flow)
        }
        if field == .negative { draft.allowNegative.toggle() }
        focus = EnvelopeCell(row: flow.id, field: field.caret)
    }

    private func cancel() {
        editing = nil
        focus = nil
    }

    private func commitFocusedLine() {
        if let row = focus?.row {
            if commit(row) { focus = nil }
        } else {
            Task { await commitNewLine() }
        }
    }

    /// Writes the draft back and says whether it went through. Only the cells
    /// that changed travel, so two people editing different envelopes of the
    /// same vault do not conflict.
    @discardableResult
    private func commit(_ flowId: Uuid) -> Bool {
        guard editing == flowId else { return true }
        guard let flow = store.flows.first(where: { $0.id == flowId }) else {
            // Archived, or gone from under the row by a sync while it was open.
            editing = nil
            return true
        }
        do {
            let patch = try draft.patch(against: flow, currency: store.currency)
            editing = nil
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
            guard let entry = try newLine.envelope(currency: store.currency) else { return }
            await store.createEnvelope(
                name: entry.name,
                mode: entry.mode,
                allowNegative: entry.allowNegative,
                openingAllocation: entry.allocation
            )
            if store.flows.contains(where: { $0.name == entry.name }) {
                newLine = EnvelopeDraft()
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
                    message: String(localized: "A capped envelope needs a cap amount")
                )
            )
        }
        return kind.mode(cap: parsed)
    }

    /// Money as the core parses it; `nil` for an empty cell. Failures are
    /// tagged with the cell they came from so the table can send the focus
    /// back there.
    private func amount(_ text: String, in field: EnvelopeField, currency: Currency) throws -> Int64? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        do {
            return try parseMoney(text: trimmed, currency: currency)
        } catch {
            throw EnvelopeDraftError(field: field, underlying: error)
        }
    }
}
