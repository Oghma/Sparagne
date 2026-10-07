import SwiftUI
import SparagneCore

/// Column geometry of the envelope table, as the canvas has it
/// (`docs/v2/UI.md` §2.3): fixed widths so the header, the rows and the empty
/// line agree without a layout pass. NOME takes what is left.
private enum EnvelopeColumn {
    static let nameMinimum: CGFloat = 100
    static let type: CGFloat = 116
    static let cap: CGFloat = 96
    static let negative: CGFloat = 56
    static let balance: CGFloat = 140
}

/// The envelopes of the vault as an editable table (`docs/v2/UI.md` §2.3):
/// the last line adds one, every cell edits in place.
///
/// Editing is per row, exactly as in the ledger (`Views/Ledger/LedgerGrid`):
/// clicking a cell opens the whole row on a draft, ↩ commits the diff as one
/// `UpdateFlow` with only the changed fields, esc throws the draft away and
/// leaving the row saves it the way a spreadsheet does.
struct EnvelopeTable: View {
    @Bindable var store: AppStore

    /// Which envelope is open for editing; the empty line is `nil` and lives
    /// in `newLine` instead, so the two drafts never overwrite each other.
    @State private var editing: Uuid?
    @State private var draft = EnvelopeDraft()
    @State private var newLine = EnvelopeDraft()
    /// The row under the pointer: it tints the row, and it also tells a blur
    /// caused by the row's own menu from a real departure (see `focusMoved`).
    @State private var hovered: Uuid?
    @FocusState private var focus: EnvelopeCell?

    var body: some View {
        SetupCard(title: String(localized: "Envelopes"), hint: String(localized: "a cap makes it a fund in the summary")) {
            SetupTableBody {
                header
                ForEach(store.flows, id: \.id) { flow in
                    activeRow(flow)
                }
                // A viewer reads the envelopes; nothing to add.
                if store.canWrite { newLineRow }
                archived
            }
        }
        // A different vault means different envelopes: whatever was half
        // typed belongs to the vault that is gone.
        .onChange(of: store.currentVault?.id) { _, _ in
            editing = nil
            focus = nil
            newLine = EnvelopeDraft()
        }
        .onChange(of: focus) { old, new in focusMoved(from: old, to: new) }
        // Turned read-only under an open row: it could not be saved.
        .onChange(of: store.isReadOnly) { _, _ in cancel() }
    }

    // MARK: - Header

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
        .onHover { inside in
            if inside {
                hovered = flow.id
            } else if hovered == flow.id {
                hovered = nil
            }
        }
        .onKeyPress(.escape) {
            guard isEditing else { return .ignored }
            cancel()
            return .handled
        }
        .contextMenu {
            // Unallocated is a system envelope: the core refuses to update or
            // archive it (`docs/v2/UI.md` §2.3).
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
        actionCell(flow, system: system)
    }

    /// The archive icon, only for the row under the pointer: the same
    /// `ArchiveFlow` the context menu already sends, one click closer.
    /// Hidden while editing, so it never fights a row's own commit-on-blur.
    @ViewBuilder
    private func actionCell(_ flow: FlowView, system: Bool) -> some View {
        SetupCell(width: SetupColumn.action) {
            if !system, hovered == flow.id, store.canWrite {
                SetupIconButton(
                    symbol: "archivebox",
                    help: String(localized: "Archive"),
                    label: String(localized: "Archive \(flow.name)")
                ) {
                    Task { await store.archiveEnvelope(flow.id) }
                }
            }
        }
    }

    /// The open row. The cap of an unlimited envelope stays editable: typing
    /// an amount and picking a type in either order must work.
    @ViewBuilder
    private func editingCells(_ flow: FlowView) -> some View {
        SetupCell {
            textField($draft.name, key: EnvelopeCell(row: flow.id, field: .name), placeholder: "")
        }
        .frame(minWidth: EnvelopeColumn.nameMinimum)
        typeCell($draft)
        SetupCell(width: EnvelopeColumn.cap, alignment: .trailing) {
            textField(
                $draft.cap,
                key: EnvelopeCell(row: flow.id, field: .cap),
                placeholder: String(localized: "cap…"),
                alignment: .trailing
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

    // MARK: - The archived envelopes

    @ViewBuilder
    private var archived: some View {
        ForEach(store.archivedFlows, id: \.id) { flow in
            SetupRow(highlighted: hovered == flow.id) {
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
                SetupCell(width: SetupColumn.action) {
                    if hovered == flow.id, store.canWrite {
                        SetupIconButton(
                            symbol: "tray.and.arrow.up",
                            help: String(localized: "Restore"),
                            label: String(localized: "Restore \(flow.name)")
                        ) {
                            Task { await store.restoreEnvelope(flow.id) }
                        }
                    }
                }
            }
            .onHover { inside in
                if inside {
                    hovered = flow.id
                } else if hovered == flow.id {
                    hovered = nil
                }
            }
            .contextMenu {
                if store.canWrite {
                    Button(String(localized: "Restore")) {
                        Task { await store.restoreEnvelope(flow.id) }
                    }
                }
            }
        }
    }

    // MARK: - The empty line

    /// Always under the active envelopes: fill it left to right and press ↩.
    /// SALDO is the opening allocation moved out of Unallocated
    /// (`docs/v2/DISTILLATO_V1.md` §2.2), which the placeholder says.
    private var newLineRow: some View {
        let active = focus?.row == nil && focus != nil
        return SetupRow(highlighted: active, editing: active) {
            SetupCell {
                textField(
                    $newLine.name,
                    key: EnvelopeCell(row: nil, field: .name),
                    placeholder: String(localized: "New envelope…")
                )
            }
            .frame(minWidth: EnvelopeColumn.nameMinimum)
            typeCell($newLine, emptyLabel: String(localized: "No cap"))
            SetupCell(width: EnvelopeColumn.cap, alignment: .trailing) {
                textField(
                    $newLine.cap,
                    key: EnvelopeCell(row: nil, field: .cap),
                    placeholder: "—",
                    alignment: .trailing
                )
            }
            negativeCell($newLine)
            SetupCell(width: EnvelopeColumn.balance, alignment: .trailing) {
                textField(
                    $newLine.allocation,
                    key: EnvelopeCell(row: nil, field: .allocation),
                    placeholder: String(localized: "allocation"),
                    alignment: .trailing
                )
            }
            SetupCell(width: SetupColumn.action) { EmptyView() }
        }
        .onKeyPress(.escape) {
            newLine = EnvelopeDraft()
            focus = nil
            return .handled
        }
    }

    // MARK: - Shared cells

    private func textField(
        _ text: Binding<String>,
        key: EnvelopeCell,
        placeholder: String,
        alignment: TextAlignment = .leading
    ) -> some View {
        TextField(placeholder, text: text)
            .textFieldStyle(.plain)
            .font(Face.row)
            .multilineTextAlignment(alignment)
            .foregroundStyle(Ink.text)
            .focused($focus, equals: key)
            .onSubmit { commitFocusedLine() }
    }

    /// TIPO is a menu, not text: the three kinds are the whole vocabulary
    /// (`docs/v2/UI.md` §2.3) and a typo has no meaning here. On the empty
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
        Text("—").font(Face.row).foregroundStyle(Ink.text3)
    }

    private static func label(negative: Bool) -> String {
        negative ? String(localized: "yes") : String(localized: "no")
    }

    /// A zero reads as background (`docs/v2/UI.md` §5 gives zeros to `dim`),
    /// an overdrawn envelope as a warning.
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

    /// esc: throw the draft away. `editing` is cleared before the focus, so
    /// the focus change that follows is not read as "left the row" and does
    /// not save what esc was meant to undo.
    private func cancel() {
        editing = nil
        focus = nil
    }

    /// ↩ in whichever line holds the caret.
    private func commitFocusedLine() {
        if let row = focus?.row {
            if commit(row) { focus = nil }
        } else {
            Task { await commitNewLine() }
        }
    }

    /// The commit on blur a spreadsheet does: ⇥ off the last cell of the row,
    /// a click on another row or on the categories table all write what was
    /// typed, as `LedgerGrid.commitIfLeft` does.
    ///
    /// The exception is the row's own menu and its NEG button: opening a menu
    /// resigns the text field without giving the focus to anything, which
    /// would close the row under the pointer. The pointer is what tells the
    /// two apart, so a blur while hovering the open row is not a departure.
    private func focusMoved(from old: EnvelopeCell?, to new: EnvelopeCell?) {
        guard let editing else { return }
        if let new {
            if new.row != editing { commit(editing) }
        } else if old?.row == editing, hovered != editing {
            commit(editing)
        }
    }

    /// Writes the draft back and says whether it went through. Only the cells
    /// that changed travel, so two people editing different envelopes of the
    /// same vault do not conflict (`docs/v2/ARCH.md` §4).
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

// MARK: - Focus

/// A cell of the envelope table: which row (nil = the empty line) and which
/// field. Only the text cells appear here, because only they can hold a caret.
struct EnvelopeCell: Hashable {
    let row: Uuid?
    let field: EnvelopeField
}

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

/// `FlowMode` as a flat choice, the way the TIPO column reads
/// (`docs/v2/UI.md` §2.3: nessuno / netto / entrate). The mode is data, not a
/// type (`docs/v2/DISTILLATO_V1.md` §2.2), so this is a lossless split into
/// "which kind" plus "how much".
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

    /// True when the kind needs an amount beside it.
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

/// The text and the choices in the cells of the line being edited. Everything
/// stays a string until it commits, so a half-typed cap never has to be a
/// number. A plain struct with no view in it, so the diff and the parsing are
/// testable on their own.
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
