import SwiftUI
import SparagneCore

/// Column geometry of the plan's table, shared by the heading, the rows and
/// the totals. Busta takes what is left. At the window's narrowest width the
/// card has about 800 points beside the inspector; the widths below add up
/// to 794 with Busta at its minimum.
private enum PlanColumn {
    static let order: CGFloat = 28
    static let envelopeMinimum: CGFloat = 120
    /// The open row's Fisso / % / Al tetto and the value box beside it.
    static let rule: CGFloat = 222
    static let room: CGFloat = 88
    static let gets: CGFloat = 88
    static let after: CGFloat = 92
    static let status: CGFloat = 104
    static let move: CGFloat = 52
}

/// "Piano di riparto": the plan's lines in their order, which is their
/// priority, with what each would get. The preview is the table: Spazio,
/// Riceve, Saldo dopo and Stato are the plan worked out on the period's total
/// while one is due, on what has come in so far otherwise.
///
/// Editing is per row, as in the Setup tables (`editsRows`): a click opens
/// the row on a draft, ↩ commits, esc throws the draft away and leaving the
/// row saves it. Every change sends the whole list
/// (`UpdateAllocationPlan` replaces it); the first line typed in the empty
/// row creates the plan. ↑ and ↓ at the end of a row, ⌥↑ and ⌥↓ on the open
/// row and the row's menu move it.
struct AllocationPlanTable: View {
    let store: AppStore
    let lines: [AllocationLine]
    let preview: AllocationPreview?
    /// A period is due: a shortfall is news, and warns.
    let due: Bool
    /// Whether a row is open, for the inspector: its ↩ and esc wait while
    /// one is.
    @Binding var rowOpen: Bool
    /// Sends the whole list, which the table shows at once; the task says
    /// whether it went through.
    let save: ([AllocationLine]) -> Task<Bool, Never>

    /// The open line, by the envelope it had when it was opened; the empty
    /// row is apart (`newLine`), so the two drafts never overwrite each
    /// other.
    @State private var editing: Uuid?
    @State private var draft = AllocationLineDraft()
    @State private var newLine = AllocationLineDraft()
    @State private var hovered: Uuid?
    @FocusState private var focus: PlanFocus?

    var body: some View {
        let names = NameBook(snapshot: store.snapshot)
        let matched = AllocationPreviewMatch.lines(lines, in: preview)
        Panel(padding: 0) {
            VStack(alignment: .leading, spacing: 0) {
                CardHeading(title: String(localized: "Allocation plan"), hint: hint)
                    .padding(.horizontal, 12)
                    .padding(.top, 10)
                header
                ForEach(Array(lines.enumerated()), id: \.element.flowId) { index, line in
                    row(line, index: index, preview: matched[index], names: names)
                }
                if store.canWrite { newLineRow(names: names) }
                totals
            }
        }
        .editsRows(editing: $editing, focus: $focus, hovered: hovered, store: store) {
            newLine = AllocationLineDraft()
        } commit: { commit($0) }
        .onChange(of: editing != nil || focus != nil, initial: true) { _, open in rowOpen = open }
    }

    private var hint: String {
        store.canWrite
            ? String(localized: "the order is the priority: when the total runs short, the last lines get less \u{00B7} \u{2325}\u{2191} \u{2325}\u{2193} move the line")
            : String(localized: "the order is the priority: when the total runs short, the last lines get less")
    }

    private var header: some View {
        SetupHeaderRow(horizontalPadding: 4, accessibilityHeader: true) {
            Text(verbatim: "#").tableCell(width: PlanColumn.order, alignment: .center, hitTestable: true)
            Text(String(localized: "Envelope")).tableCell(minWidth: PlanColumn.envelopeMinimum, hitTestable: true)
            Text(String(localized: "Rule")).tableCell(width: PlanColumn.rule, hitTestable: true)
            Text(String(localized: "Room")).tableCell(width: PlanColumn.room, alignment: .trailing, hitTestable: true)
            Text(String(localized: "Gets")).tableCell(width: PlanColumn.gets, alignment: .trailing, hitTestable: true)
            Text(String(localized: "Balance after")).tableCell(width: PlanColumn.after, alignment: .trailing, hitTestable: true)
            Text(String(localized: "Status")).tableCell(width: PlanColumn.status, hitTestable: true)
            Text(verbatim: "").tableCell(width: PlanColumn.move, hitTestable: true)
        }
    }

    // MARK: - A line

    private func row(_ line: AllocationLine, index: Int, preview: PreviewLine?, names: NameBook) -> some View {
        let key = line.flowId
        let isEditing = editing == key
        let flow = store.snapshot?.flows.first { $0.id == key }
        let gone = flow.map(\.archived) ?? true
        return HStack(spacing: 0) {
            Text(index + 1, format: .number).foregroundStyle(Ink.text3)
                .tableCell(width: PlanColumn.order, alignment: .center, hitTestable: true)
            if isEditing {
                envelopeMenu(draft: $draft, key: key, placeholder: names.flow(key) ?? TransactionRow.placeholder)
                ruleEditor(draft: $draft, focusKey: .line(key))
            } else {
                Text(names.flow(key) ?? TransactionRow.placeholder)
                    .foregroundStyle(gone ? Ink.text3 : Ink.text)
                    .strikethrough(gone)
                    .tableCell(minWidth: PlanColumn.envelopeMinimum, hitTestable: true)
                    .onTapGesture { open(line) }
                Text(AllocationText.rule(line.rule, cap: flow.flatMap { EnvelopeCapKind.cap(of: $0.mode) }))
                    .foregroundStyle(Ink.text2)
                    .tableCell(width: PlanColumn.rule, hitTestable: true)
                    .onTapGesture { open(line) }
            }
            figures(preview)
                .onTapGesture { open(line) }
            moveButtons(key: key, index: index)
        }
        .font(Face.row)
        .padding(.horizontal, 4)
        .frame(height: isEditing ? 30 : 26)
        .background(isEditing || hovered == key ? Ink.raised : .clear)
        .overlay(alignment: .leading) {
            if isEditing { Rectangle().fill(Ink.accent).frame(width: 2) }
        }
        .overlay(alignment: .bottom) {
            if !isEditing { Rectangle().fill(Ink.rowLine).frame(height: 1) }
        }
        .contentShape(Rectangle())
        .hoverId($hovered, key)
        .onEscape(when: isEditing, perform: cancel)
        // ⌥↑ ⌥↓ move the open row; the arrows alone stay the caret's.
        .onKeyPress(keys: [.upArrow, .downArrow], phases: .down) { press in
            guard isEditing, press.modifiers.contains(.option) else { return .ignored }
            move(key, by: press.key == .upArrow ? -1 : 1)
            return .handled
        }
        .accessibilityElement(children: .contain)
        .accessibilityAction(named: String(localized: "Edit")) { open(line) }
        .contextMenu {
            if store.canWrite {
                Button(String(localized: "Move Up")) { move(key, by: -1) }
                    .disabled(index == 0)
                Button(String(localized: "Move Down")) { move(key, by: 1) }
                    .disabled(index == lines.count - 1)
                Divider()
                Button(String(localized: "Remove from the Plan"), role: .destructive) { remove(key) }
                    .disabled(lines.count < 2)
            }
        }
    }

    /// Spazio, Riceve, Saldo dopo and Stato: the line's preview, empty while
    /// the preview of these lines is on its way.
    private func figures(_ line: PreviewLine?) -> some View {
        Group {
            Group {
                if let line {
                    Text(line.room.map(LedgerMoney.bare) ?? TransactionRow.placeholder).foregroundStyle(Ink.text3)
                } else {
                    Text(verbatim: "")
                }
            }
            .tableCell(width: PlanColumn.room, alignment: .trailing, hitTestable: true)
            Text(line.map { LedgerMoney.bare($0.amount) } ?? "")
                .foregroundStyle(line?.amount == 0 ? Ink.text3 : Ink.text)
                .tableCell(width: PlanColumn.gets, alignment: .trailing, hitTestable: true)
            Text(line.map { LedgerMoney.bare($0.balanceAfter) } ?? "")
                .foregroundStyle((line?.balanceAfter ?? 0) < 0 ? Ink.negative : Ink.text2)
                .tableCell(width: PlanColumn.after, alignment: .trailing, hitTestable: true)
            Group {
                if let line {
                    let tag = AllocationText.tag(line, due: due)
                    if tag.warns {
                        RowTag(text: tag.text, tint: Ink.accent)
                    } else {
                        SetupTag(text: tag.text)
                    }
                } else {
                    Text(verbatim: "")
                }
            }
            .tableCell(width: PlanColumn.status, hitTestable: true)
        }
    }

    /// ↑ and ↓, for a vault the account may change.
    private func moveButtons(key: Uuid, index: Int) -> some View {
        Group {
            if store.canWrite {
                HStack(spacing: 2) {
                    moveButton(symbol: "chevron.up", label: String(localized: "Move Up")) { move(key, by: -1) }
                        .disabled(index == 0)
                    moveButton(symbol: "chevron.down", label: String(localized: "Move Down")) { move(key, by: 1) }
                        .disabled(index == lines.count - 1)
                }
            } else {
                Text(verbatim: "")
            }
        }
        .tableCell(width: PlanColumn.move, alignment: .trailing, hitTestable: true)
    }

    private func moveButton(symbol: String, label: String, action: @escaping () -> Void) -> some View {
        SetupIconButton(symbol: symbol, help: label, label: label, symbolSize: 9, symbolWeight: .bold, action: action)
    }

    // MARK: - The open row's cells

    /// Busta: a menu of the envelopes a line may take
    /// (`AllocationLineDraft.offered`).
    private func envelopeMenu(draft: Binding<AllocationLineDraft>, key: Uuid?, placeholder: String) -> some View {
        let names = NameBook(snapshot: store.snapshot)
        let offered = AllocationLineDraft.offered(store.flows, lines: lines, keeping: key)
        let chosen = draft.wrappedValue.flowId.flatMap { names.flow($0) }
        return Menu {
            ForEach(offered, id: \.id) { flow in
                Button(flow.name) {
                    draft.wrappedValue.flowId = flow.id
                    refocus(key.map(PlanFocus.line) ?? .newLine)
                }
            }
            if offered.isEmpty {
                Text(String(localized: "No envelope left to add"))
            }
        } label: {
            HStack(spacing: 4) {
                Text(chosen ?? placeholder)
                    .lineLimit(1)
                    .foregroundStyle(chosen == nil ? Ink.text3 : Ink.text)
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(Ink.text3)
                Spacer(minLength: 0)
            }
            .font(Face.row)
            .contentShape(Rectangle())
        }
        .plainMenu()
        .accessibilityLabel(String(localized: "Envelope"))
        .accessibilityValue(chosen ?? "")
        .tableCell(minWidth: PlanColumn.envelopeMinimum, hitTestable: true)
    }

    /// Regola: Fisso / % / Al tetto, and the value beside it. Al tetto asks
    /// for none: the envelope's cap stands in its place, and holds the focus
    /// so ↩ and esc still reach the row.
    private func ruleEditor(draft: Binding<AllocationLineDraft>, focusKey: PlanFocus) -> some View {
        let kind = draft.wrappedValue.kind
        let cap = draft.wrappedValue.cap(in: store.snapshot?.flows ?? [])
        return HStack(spacing: 6) {
            RecurringSegments(
                options: AllocationRuleKind.allCases,
                selection: Binding(
                    get: { kind },
                    set: { new in
                        draft.wrappedValue.kind = new
                        refocus(focusKey)
                    }
                ),
                label: AllocationText.kind,
                name: String(localized: "Rule")
            )
            if kind.takesValue {
                TextField(String(localized: "Amount or percentage"), text: draft.value, prompt: Text(verbatim: ""))
                    .textFieldStyle(.plain)
                    .labelsHidden()
                    .font(Face.row)
                    .multilineTextAlignment(.trailing)
                    .padding(.horizontal, 5)
                    .frame(width: kind == .fixed ? 64 : 44, height: 20)
                    .background(Ink.card, in: RoundedRectangle(cornerRadius: 5))
                    .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Ink.line2, lineWidth: 1))
                    .focused($focus, equals: focusKey)
                    .onSubmit { commitFocused() }
                    .accessibilityLabel(String(localized: "Amount or percentage"))
            } else {
                Text(cap.map(LedgerMoney.bare) ?? String(localized: "no cap"))
                    .foregroundStyle(Ink.text3)
                    .focusable()
                    .focusEffectDisabled()
                    .focused($focus, equals: focusKey)
                    .onKeyPress(.return) {
                        commitFocused()
                        return .handled
                    }
                    .accessibilityLabel(String(localized: "Cap"))
                    .accessibilityValue(cap.map(LedgerMoney.bare) ?? String(localized: "no cap"))
            }
        }
        .tableCell(width: PlanColumn.rule, hitTestable: true)
    }

    // MARK: - The empty row

    /// "Aggiungi busta…": pick an envelope, a rule and its value, then ↩.
    /// The first line of a vault without a plan creates it.
    private func newLineRow(names: NameBook) -> some View {
        let active = newLine.flowId != nil || focus == .newLine
        return HStack(spacing: 0) {
            Image(systemName: "plus")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(Ink.text3)
                .accessibilityHidden(true)
                .tableCell(width: PlanColumn.order, alignment: .center, hitTestable: true)
            envelopeMenu(draft: $newLine, key: nil, placeholder: String(localized: "Add envelope\u{2026}"))
            if active {
                ruleEditor(draft: $newLine, focusKey: .newLine)
            } else {
                Text(String(localized: "Fixed \u{00B7} % \u{00B7} To cap")).foregroundStyle(Ink.text3)
                    .tableCell(width: PlanColumn.rule, hitTestable: true)
            }
            blankCells([PlanColumn.room, PlanColumn.gets, PlanColumn.after, PlanColumn.status, PlanColumn.move])
        }
        .font(Face.row)
        .padding(.horizontal, 4)
        .frame(height: active ? 30 : 26)
        .background(active ? Ink.raised : .clear)
        .overlay(alignment: .leading) {
            if active { Rectangle().fill(Ink.accent).frame(width: 2) }
        }
        .onEscape {
            newLine = AllocationLineDraft()
            focus = nil
        }
    }

    // MARK: - Totals

    /// Totale: what the percent and the fixed lines ask for, and what the
    /// lines get together.
    private var totals: some View {
        let figures = AllocationFigures(lines: lines)
        return HStack(spacing: 0) {
            blankCells([PlanColumn.order])
            Text(String(localized: "Total")).tableCell(minWidth: PlanColumn.envelopeMinimum, hitTestable: true)
            Text(String(localized: "percentages \(AllocationPercent.label(figures.percent)) \u{00B7} fixed \(LedgerMoney.bare(figures.fixed))"))
                .fontWeight(.medium)
                .foregroundStyle(figures.percent > AllocationPercent.full ? Ink.accent : Ink.text2)
                .tableCell(width: PlanColumn.rule, hitTestable: true)
            blankCells([PlanColumn.room])
            Text(preview.map { LedgerMoney.bare($0.distributed) } ?? "")
                .tableCell(width: PlanColumn.gets, alignment: .trailing, hitTestable: true)
            blankCells([PlanColumn.after, PlanColumn.status, PlanColumn.move])
        }
        .font(Face.ui(12, .semibold))
        .foregroundStyle(Ink.text)
        .padding(.horizontal, 4)
        .frame(height: 26)
        .overlay(alignment: .top) { Rectangle().fill(Ink.line2).frame(height: 1) }
        .accessibilityElement(children: .combine)
    }

    // MARK: - Actions

    /// Opens `line`, saving whatever row was open before. A row that refuses
    /// to save keeps the focus, so the click that would have left it does
    /// not lose what was typed.
    private func open(_ line: AllocationLine) {
        guard store.canWrite else { return }
        if editing != line.flowId {
            if let editing, !commit(editing) { return }
            editing = line.flowId
            draft = AllocationLineDraft(line: line)
        }
        focus = .line(line.flowId)
    }

    private func cancel() {
        editing = nil
        focus = nil
    }

    /// The focus again on `key` once the cell that holds it is drawn: a
    /// segment or an envelope picked takes it from the text field, and Al
    /// tetto swaps the field for the cap.
    private func refocus(_ key: PlanFocus) {
        Task { focus = key }
    }

    /// ↩ in whichever row holds the focus.
    private func commitFocused() {
        switch focus {
        case .line(let key):
            if commit(key) { focus = nil }
        case .newLine:
            commitNewLine()
        case nil:
            break
        }
    }

    /// Writes the open row into the list and sends it; says whether the draft
    /// was a line. The row closes at once: the list was already checked, and
    /// the write itself belongs to the core.
    @discardableResult
    private func commit(_ key: Uuid) -> Bool {
        guard editing == key else { return true }
        guard lines.contains(where: { $0.flowId == key }) else {
            // Gone from under the row, by a sync while it was open.
            editing = nil
            return true
        }
        do {
            let updated = try draft.committing(into: lines, replacing: key, currency: store.currency)
            editing = nil
            if updated != lines { send(updated) }
            return true
        } catch {
            refuse(error, at: .line(key))
            return false
        }
    }

    /// ↩ on the empty row: the line goes to the end of the plan, and the row
    /// empties for the next. A refusal puts back what was typed.
    private func commitNewLine() {
        guard !newLine.isBlank else { return }
        do {
            let updated = try newLine.committing(into: lines, replacing: nil, currency: store.currency)
            let typed = newLine
            newLine = AllocationLineDraft()
            focus = nil
            let saving = save(updated)
            Task {
                if !(await saving.value) { newLine = typed }
            }
        } catch {
            refuse(error, at: .newLine)
        }
    }

    /// The line of `key` moved `offset` places, with the open row's draft
    /// written in first: the two go as one list. The open row stays open.
    private func move(_ key: Uuid, by offset: Int) {
        guard store.canWrite, let current = linesWithDraft(for: key),
              let index = current.lines.firstIndex(where: { $0.flowId == current.target }),
              let moved = AllocationLines.moving(current.lines, at: index, by: offset)
        else { return }
        // The draft went in with the move: the open row follows its line, or
        // closes when another one moved.
        editing = editing == key ? current.target : nil
        send(moved)
    }

    /// The line of `key` out of the plan, with the open row's draft written
    /// in first. Not the last line: a plan has at least one.
    private func remove(_ key: Uuid) {
        guard store.canWrite, let current = linesWithDraft(for: key),
              let remaining = AllocationLines.removing(current.target, from: current.lines)
        else { return }
        editing = nil
        send(remaining)
    }

    /// The list with the open row's draft in it, and the envelope `key`'s
    /// line has there (the open row's draft may have changed it); `nil` when
    /// the draft is refused, which is reported.
    private func linesWithDraft(for key: Uuid) -> (lines: [AllocationLine], target: Uuid)? {
        guard let editing else { return (lines, key) }
        do {
            let updated = try draft.committing(into: lines, replacing: editing, currency: store.currency)
            return (updated, key == editing ? (draft.flowId ?? key) : key)
        } catch {
            refuse(error, at: .line(editing))
            return nil
        }
    }

    private func send(_ updated: [AllocationLine]) {
        _ = save(updated)
    }

    private func refuse(_ error: Error, at key: PlanFocus) {
        store.report((error as? AllocationDraftError)?.underlying ?? error)
        focus = key
    }

    /// Cells with nothing in them, holding their columns' widths: an empty
    /// text rather than no view, which a frame would drop from the row.
    private func blankCells(_ widths: [CGFloat]) -> some View {
        ForEach(Array(widths.enumerated()), id: \.offset) { _, width in
            Text(verbatim: "").tableCell(width: width, hitTestable: true)
        }
    }
}

/// What holds the focus in the table: the value of a line, by the envelope
/// it had when it was opened, or of the empty row. Segments and the envelope
/// menu take the focus from it while the pointer is over the row, which is
/// not leaving it (`editsRows`).
private nonisolated enum PlanFocus: RowFocus {
    case line(Uuid)
    case newLine

    var row: Uuid? {
        if case .line(let key) = self { key } else { nil }
    }
}
