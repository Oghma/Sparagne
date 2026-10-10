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
    static let padding: CGFloat = 8
}

/// "Piano di riparto": the plan's lines in their order, which is their
/// priority, with what each would get. The preview is the table: Spazio,
/// Riceve, Saldo dopo and Stato are the plan worked out on the period's total
/// while one is due, on what has come in so far otherwise.
///
/// Editing is per row, as in the Setup tables (`EnvelopeTable`): a click
/// opens the row on a draft, ↩ commits, esc throws the draft away and
/// leaving the row saves it. Every change sends the whole list
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
                RecurringCardHeader(title: String(localized: "Allocation plan")) {
                    Text(hint)
                        .font(Face.ui(11.5, .medium))
                        .foregroundStyle(Ink.text3)
                        .lineLimit(2)
                        .multilineTextAlignment(.trailing)
                }
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
        // Another vault: whatever was half typed belongs to the plan that is
        // gone.
        .onChange(of: store.currentVault?.id) { _, _ in
            editing = nil
            focus = nil
            newLine = AllocationLineDraft()
        }
        .onChange(of: focus) { old, new in focusMoved(from: old, to: new) }
        .onChange(of: editing != nil || focus != nil, initial: true) { _, open in rowOpen = open }
        // Turned read-only under an open row: it could not be saved.
        .onChange(of: store.isReadOnly) { _, _ in cancel() }
    }

    private var hint: String {
        store.canWrite
            ? String(localized: "the order is the priority: when the total runs short, the last lines get less \u{00B7} \u{2325}\u{2191} \u{2325}\u{2193} move the line")
            : String(localized: "the order is the priority: when the total runs short, the last lines get less")
    }

    // MARK: - Heading

    private var header: some View {
        HStack(spacing: 0) {
            cell(width: PlanColumn.order, alignment: .center) { Text(verbatim: "#") }
            cell(minWidth: PlanColumn.envelopeMinimum) { Text(String(localized: "Envelope")) }
            cell(width: PlanColumn.rule) { Text(String(localized: "Rule")) }
            cell(width: PlanColumn.room, alignment: .trailing) { Text(String(localized: "Room")) }
            cell(width: PlanColumn.gets, alignment: .trailing) { Text(String(localized: "Gets")) }
            cell(width: PlanColumn.after, alignment: .trailing) { Text(String(localized: "Balance after")) }
            cell(width: PlanColumn.status) { Text(String(localized: "Status")) }
            cell(width: PlanColumn.move) { Text(verbatim: "") }
        }
        .font(Face.ui(11, .medium))
        .foregroundStyle(Ink.text3)
        .padding(.horizontal, 4)
        .frame(height: Metrics.headerHeight)
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }

    // MARK: - A line

    private func row(_ line: AllocationLine, index: Int, preview: PreviewLine?, names: NameBook) -> some View {
        let key = line.flowId
        let isEditing = editing == key
        let flow = store.snapshot?.flows.first { $0.id == key }
        let gone = flow.map(\.archived) ?? true
        return HStack(spacing: 0) {
            cell(width: PlanColumn.order, alignment: .center) {
                Text(index + 1, format: .number).foregroundStyle(Ink.text3)
            }
            if isEditing {
                envelopeMenu(draft: $draft, key: key, placeholder: names.flow(key) ?? TransactionRow.placeholder)
                ruleEditor(draft: $draft, focusKey: .line(key))
            } else {
                cell(minWidth: PlanColumn.envelopeMinimum) {
                    Text(names.flow(key) ?? TransactionRow.placeholder)
                        .foregroundStyle(gone ? Ink.text3 : Ink.text)
                        .strikethrough(gone)
                }
                .onTapGesture { open(line) }
                cell(width: PlanColumn.rule) {
                    Text(AllocationText.rule(line.rule, cap: flow.flatMap { EnvelopeCapKind.cap(of: $0.mode) }))
                        .foregroundStyle(Ink.text2)
                }
                .onTapGesture { open(line) }
            }
            figures(preview)
                .onTapGesture { open(line) }
            moveButtons(key: key, index: index)
        }
        .font(Face.ui(12))
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
        .onHover { inside in
            if inside {
                hovered = key
            } else if hovered == key {
                hovered = nil
            }
        }
        .onKeyPress(.escape) {
            guard isEditing else { return .ignored }
            cancel()
            return .handled
        }
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
            cell(width: PlanColumn.room, alignment: .trailing) {
                if let line {
                    Text(line.room.map(LedgerMoney.bare) ?? TransactionRow.placeholder).foregroundStyle(Ink.text3)
                } else {
                    Text(verbatim: "")
                }
            }
            cell(width: PlanColumn.gets, alignment: .trailing) {
                Text(line.map { LedgerMoney.bare($0.amount) } ?? "")
                    .foregroundStyle(line?.amount == 0 ? Ink.text3 : Ink.text)
            }
            cell(width: PlanColumn.after, alignment: .trailing) {
                Text(line.map { LedgerMoney.bare($0.balanceAfter) } ?? "")
                    .foregroundStyle((line?.balanceAfter ?? 0) < 0 ? Ink.negative : Ink.text2)
            }
            cell(width: PlanColumn.status) {
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
        }
    }

    /// ↑ and ↓, for a vault the account may change.
    private func moveButtons(key: Uuid, index: Int) -> some View {
        cell(width: PlanColumn.move, alignment: .trailing) {
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
    }

    private func moveButton(symbol: String, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(Ink.text3)
                .frame(width: 20, height: 20)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(label)
        .accessibilityLabel(label)
    }

    // MARK: - The open row's cells

    /// Busta: a menu of the active envelopes, those another line already has
    /// left out.
    private func envelopeMenu(draft: Binding<AllocationLineDraft>, key: Uuid?, placeholder: String) -> some View {
        let names = NameBook(snapshot: store.snapshot)
        let taken = Set(lines.map(\.flowId)).subtracting([key].compactMap { $0 })
        let offered = store.flows.filter { !$0.isUnallocated && !taken.contains($0.id) }
        let chosen = draft.wrappedValue.flowId.flatMap { names.flow($0) }
        return cell(minWidth: PlanColumn.envelopeMinimum) {
            Menu {
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
                .font(Face.ui(12))
                .contentShape(Rectangle())
            }
            // As in `FormMenu`: the plain style keeps the label as drawn.
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .accessibilityLabel(String(localized: "Envelope"))
            .accessibilityValue(chosen ?? "")
        }
    }

    /// Regola: Fisso / % / Al tetto, and the value beside it. Al tetto asks
    /// for none: the envelope's cap stands in its place, and holds the focus
    /// so ↩ and esc still reach the row.
    private func ruleEditor(draft: Binding<AllocationLineDraft>, focusKey: PlanFocus) -> some View {
        let kind = draft.wrappedValue.kind
        let cap = draft.wrappedValue.flowId
            .flatMap { id in store.snapshot?.flows.first { $0.id == id } }
            .flatMap { EnvelopeCapKind.cap(of: $0.mode) }
        return cell(width: PlanColumn.rule) {
            HStack(spacing: 6) {
                RecurringSegments(
                    options: AllocationRuleKind.allCases,
                    selection: kind,
                    label: AllocationText.kind,
                    select: { new in
                        draft.wrappedValue.kind = new
                        refocus(focusKey)
                    },
                    name: String(localized: "Rule")
                )
                if kind.takesValue {
                    TextField(String(localized: "Amount or percentage"), text: draft.value, prompt: Text(verbatim: ""))
                        .textFieldStyle(.plain)
                        .labelsHidden()
                        .font(Face.ui(12))
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
        }
    }

    // MARK: - The empty row

    /// "Aggiungi busta…": pick an envelope, a rule and its value, then ↩.
    /// The first line of a vault without a plan creates it.
    private func newLineRow(names: NameBook) -> some View {
        let active = newLine.flowId != nil || focus == .newLine
        return HStack(spacing: 0) {
            cell(width: PlanColumn.order, alignment: .center) {
                Image(systemName: "plus")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(Ink.text3)
                    .accessibilityHidden(true)
            }
            envelopeMenu(draft: $newLine, key: nil, placeholder: String(localized: "Add envelope\u{2026}"))
            if active {
                ruleEditor(draft: $newLine, focusKey: .newLine)
            } else {
                cell(width: PlanColumn.rule) {
                    Text(String(localized: "Fixed \u{00B7} % \u{00B7} To cap")).foregroundStyle(Ink.text3)
                }
            }
            cell(width: PlanColumn.room) { Text(verbatim: "") }
            cell(width: PlanColumn.gets) { Text(verbatim: "") }
            cell(width: PlanColumn.after) { Text(verbatim: "") }
            cell(width: PlanColumn.status) { Text(verbatim: "") }
            cell(width: PlanColumn.move) { Text(verbatim: "") }
        }
        .font(Face.ui(12))
        .padding(.horizontal, 4)
        .frame(height: active ? 30 : 26)
        .background(active ? Ink.raised : .clear)
        .overlay(alignment: .leading) {
            if active { Rectangle().fill(Ink.accent).frame(width: 2) }
        }
        .onKeyPress(.escape) {
            newLine = AllocationLineDraft()
            focus = nil
            return .handled
        }
    }

    // MARK: - Totals

    /// Totale: what the percent and the fixed lines ask for, and what the
    /// lines get together.
    private var totals: some View {
        let figures = AllocationFigures(lines: lines)
        return HStack(spacing: 0) {
            cell(width: PlanColumn.order) { Text(verbatim: "") }
            cell(minWidth: PlanColumn.envelopeMinimum) { Text(String(localized: "Total")) }
            cell(width: PlanColumn.rule) {
                Text(String(localized: "percentages \(AllocationPercent.label(figures.percent)) \u{00B7} fixed \(LedgerMoney.bare(figures.fixed))"))
                    .fontWeight(.medium)
                    .foregroundStyle(figures.percent > AllocationPercent.full ? Ink.accent : Ink.text2)
            }
            cell(width: PlanColumn.room) { Text(verbatim: "") }
            cell(width: PlanColumn.gets, alignment: .trailing) {
                Text(preview.map { LedgerMoney.bare($0.distributed) } ?? "")
            }
            cell(width: PlanColumn.after) { Text(verbatim: "") }
            cell(width: PlanColumn.status) { Text(verbatim: "") }
            cell(width: PlanColumn.move) { Text(verbatim: "") }
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

    /// esc: throw the draft away. `editing` goes before the focus, so the
    /// focus change that follows is not read as leaving the row.
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

    /// Leaving the open row saves it, as `EnvelopeTable.focusMoved` does. A
    /// blur while the pointer is still over the row is the row's own menu or
    /// segments taking the focus, not a departure.
    private func focusMoved(from old: PlanFocus?, to new: PlanFocus?) {
        guard let editing else { return }
        if let new {
            if new != .line(editing) { commit(editing) }
        } else if old == .line(editing), hovered != editing {
            commit(editing)
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

    // MARK: - Pieces

    private func cell<Content: View>(
        width: CGFloat? = nil,
        minWidth: CGFloat? = nil,
        alignment: Alignment = .leading,
        @ViewBuilder content: () -> Content
    ) -> some View {
        content()
            .lineLimit(1)
            .truncationMode(.tail)
            .padding(.horizontal, PlanColumn.padding)
            .frame(
                minWidth: width ?? minWidth,
                idealWidth: width,
                maxWidth: width ?? .infinity,
                alignment: alignment
            )
            .contentShape(Rectangle())
    }
}

/// What holds the focus in the table: the value of a line, by the envelope
/// it had when it was opened, or of the empty row.
private enum PlanFocus: Hashable {
    case line(Uuid)
    case newLine
}
