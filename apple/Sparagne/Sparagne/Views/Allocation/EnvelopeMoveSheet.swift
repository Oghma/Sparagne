import SwiftUI
import SparagneCore

/// Column geometry of the sheet's table. A takes what is left: the sheet's
/// 640 points of content less the rest leave it about 148.
private enum MoveColumn {
    static let envelopeMinimum: CGFloat = 100
    static let figure: CGFloat = 96
    static let amount: CGFloat = 112
    static let actions: CGFloat = 92
    static let padding: CGFloat = 6
    static let rowHeight: CGFloat = 30
}

/// "Move between envelopes": money out of one envelope into several at once, outside the plan.
struct EnvelopeMoveSheet: View {
    let store: AppStore

    @Environment(\.dismiss) private var dismiss
    @State private var draft: EnvelopeMoveDraft
    /// The envelopes from the draft's day on, read again when the day or a
    /// balance changes: a move dated in the past can only use what the
    /// source held then and still holds at its lowest afterwards. Empty
    /// until the first answer: the envelopes as they are now stand in, which
    /// is what a move dated today sees.
    @State private var balances: [Uuid: EnvelopeMoveBalance] = [:]
    @State private var moving = false
    @State private var hovered: Uuid?
    @FocusState private var focused: Uuid?

    init(store: AppStore) {
        self.store = store
        let source = store.flows.first(where: \.isUnallocated) ?? store.flows.first
        _draft = State(initialValue: EnvelopeMoveDraft(sourceId: source?.id ?? "", day: CoreDate.day(Date())))
    }

    var body: some View {
        let check = draft.check(flows: store.flows, currency: store.currency, balances: balances)
        FormSheet(
            String(localized: "Move between envelopes"),
            subtitle: String(localized: "Once only: the allocation plan and its periods stay as they are."),
            width: 680
        ) {
            FormGroup {
                FormRow(String(localized: "From")) { source(check) }
                FormRow(String(localized: "Date")) {
                    DayField(label: String(localized: "Date"), day: $draft.day)
                        .frame(width: 240)
                }
                FormRow(String(localized: "Note")) {
                    FormTextField(label: String(localized: "Note"), text: $draft.note, prompt: String(localized: "optional"))
                }
            }
            table(check)
            totals(check)
            FormNote(
                String(localized: "Each line becomes a transfer in the ledger, with the same note and date. All of them go through or none does: if one line cannot, nothing moves."),
                indented: false
            )
        } footer: {
            reason(check)
            FormCancelButton { dismiss() }
                .disabled(moving)
            FormPrimaryButton(title: moveTitle(check)) {
                Task { await move() }
            }
            .disabled(!check.canMove || moving)
        }
        // Opened before the vault's envelopes were loaded (a launch option
        // can do that): Unallocated as soon as there is one, unless a
        // source was chosen meanwhile.
        .onChange(of: store.flows.first(where: \.isUnallocated)?.id, initial: true) { _, unallocated in
            if draft.sourceId.isEmpty, let unallocated {
                draft.setSource(unallocated)
            }
        }
        // A sync or a row edited elsewhere changes a balance: the day's
        // figures are read again with it. A failed read keeps the last ones
        // and is on the alert; Sposta checks again before it sends anyway.
        .task(id: BalancesKey(day: draft.day, flows: store.flows)) {
            if let loaded = await store.envelopeBalances(on: draft.day) {
                balances = loaded
            }
        }
    }

    private struct BalancesKey: Equatable {
        let day: NaiveDate
        let flows: [FlowView]
    }

    // MARK: - Da

    /// The source, and its balance beside it on the day chosen.
    private func source(_ check: EnvelopeMoveCheck) -> some View {
        HStack(spacing: 10) {
            FormPicker(
                label: String(localized: "From"),
                selection: Binding(get: { draft.sourceId }, set: { draft.setSource($0) }),
                options: store.flows.map(\.id),
                title: name
            )
            .frame(width: 240)
            if let balance = check.sourceBalance?.balance {
                Text(balanceLabel(balance))
                    .font(Face.row)
                    .foregroundStyle(Ink.text3)
                    .monospacedDigit()
            }
        }
    }

    /// `saldo 1.000,00` today, `saldo al 31 dic 2025 0,00` on another day.
    private func balanceLabel(_ balance: Int64) -> String {
        draft.day == CoreDate.day(Date())
            ? String(localized: "balance \(LedgerMoney.bare(balance))")
            : String(localized: "balance on \(RecurringDayText.full(draft.day)) \(LedgerMoney.bare(balance))")
    }

    // MARK: - The lines

    private func table(_ check: EnvelopeMoveCheck) -> some View {
        VStack(spacing: 0) {
            header
            ForEach(check.rows) { row in
                line(row)
                if let issue = row.issue, issue != .empty {
                    issueLine(row, issue)
                }
            }
            addRow
        }
        .font(Face.row)
        .monospacedDigit()
    }

    private var header: some View {
        SetupHeaderRow {
            Text(String(localized: "To")).tableCell(minWidth: MoveColumn.envelopeMinimum, padding: MoveColumn.padding)
            Text(String(localized: "Balance")).tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            Text(String(localized: "Room")).tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            Text(String(localized: "Amount")).tableCell(width: MoveColumn.amount, alignment: .trailing, padding: MoveColumn.padding)
            Text(String(localized: "Balance after"))
                .tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            // A text, not EmptyView: a frame around nothing takes no room,
            // and the headings would slide off their columns.
            Text(verbatim: "").tableCell(width: MoveColumn.actions, padding: MoveColumn.padding)
        }
    }

    /// One envelope the money goes to. The row under the pointer or with the
    /// cursor is raised, and offers Al tetto when the envelope has room
    /// under a cap; the one being typed in also gets the amber bar.
    private func line(_ row: EnvelopeMoveCheck.Row) -> some View {
        let title = row.envelope.map { store.flowName($0) } ?? TransactionRow.placeholder
        let active = hovered == row.flowId || focused == row.flowId
        let overCap = if case .overCap = row.issue { true } else { false }
        return HStack(spacing: 0) {
            Text(title).foregroundStyle(row.envelope == nil ? Ink.text3 : Ink.text)
                .tableCell(minWidth: MoveColumn.envelopeMinimum, padding: MoveColumn.padding)
            Text(row.balance.map { LedgerMoney.bare($0.balance) } ?? TransactionRow.placeholder)
                .foregroundStyle(Ink.text2)
                .tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            Text(row.room.map(LedgerMoney.bare) ?? TransactionRow.placeholder)
                .foregroundStyle(Ink.text3)
                .tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            amountField(row, title: title).tableCell(width: MoveColumn.amount, padding: MoveColumn.padding)
            Text(row.after.map(LedgerMoney.bare) ?? TransactionRow.placeholder)
                .foregroundStyle(overCap ? Ink.negative : Ink.text)
                .tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            HStack(spacing: 2) {
                if active, let room = row.room, room > 0, row.amount != room {
                    Button(String(localized: "To cap")) { draft.fill(row.flowId, room: room) }
                        .buttonStyle(.chrome(.ghost, small: true))
                }
                SetupIconButton(
                    symbol: "xmark",
                    help: String(localized: "Remove \(title)"),
                    label: String(localized: "Remove \(title)"),
                    symbolSize: 9,
                    symbolWeight: .semibold
                ) {
                    draft.remove(row.flowId)
                }
            }
            .tableCell(width: MoveColumn.actions, alignment: .trailing, padding: MoveColumn.padding)
        }
        .frame(height: MoveColumn.rowHeight)
        .background {
            if active { RoundedRectangle(cornerRadius: 4).fill(Ink.raised) }
        }
        .overlay(alignment: .leading) {
            if focused == row.flowId {
                RoundedRectangle(cornerRadius: 1).fill(Ink.accent).frame(width: 2).padding(.vertical, 4)
            }
        }
        .overlay(alignment: .bottom) {
            if !active && (row.issue == nil || row.issue == .empty) {
                Rectangle().fill(Ink.rowLine).frame(height: 1)
            }
        }
        .hoverId($hovered, row.flowId)
    }

    private func amountField(_ row: EnvelopeMoveCheck.Row, title: String) -> some View {
        let text = Binding(
            get: { draft.lines.first { $0.flowId == row.flowId }?.amount ?? "" },
            set: { draft.setAmount($0, for: row.flowId) }
        )
        let wrong = row.issue != nil && row.issue != .empty
        let border = wrong ? Ink.negative.opacity(0.6) : focused == row.flowId ? Ink.accent.opacity(0.6) : Ink.line2
        return TextField(String(localized: "Amount for \(title)"), text: text, prompt: Text(verbatim: ""))
            .textFieldStyle(.plain)
            .labelsHidden()
            .multilineTextAlignment(.trailing)
            .foregroundStyle(wrong ? Ink.negative : Ink.text)
            .padding(.horizontal, 6)
            .frame(height: 22)
            .background(Ink.sheet, in: RoundedRectangle(cornerRadius: 5))
            .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(border, lineWidth: 1))
            .focused($focused, equals: row.flowId)
            .accessibilityLabel(String(localized: "Amount for \(title)"))
    }

    /// What is wrong with a line, under it: a tag, a sentence, and Al tetto
    /// with the amount that fits when the line is over its cap.
    private func issueLine(_ row: EnvelopeMoveCheck.Row, _ issue: EnvelopeMoveCheck.Issue) -> some View {
        let title = row.envelope.map { store.flowName($0) } ?? TransactionRow.placeholder
        let words = IssueText(issue: issue, title: title, room: row.room)
        return HStack(spacing: 8) {
            RowTag(text: words.tag, tint: Ink.negative, opacity: 0.14)
            Text(words.sentence)
                .font(Face.ui(11.5))
                .foregroundStyle(Ink.text2)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            if case .overCap = issue, let room = row.room, room > 0 {
                Button(String(localized: "To cap: \(LedgerMoney.bare(room))")) { draft.fill(row.flowId, room: room) }
                    .buttonStyle(.chrome(small: true))
                    .fixedSize()
            }
        }
        .padding(.horizontal, MoveColumn.padding + 2)
        .padding(.bottom, 6)
        .frame(minHeight: 24)
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.rowLine).frame(height: 1) }
        .accessibilityElement(children: .combine)
    }

    /// Aggiungi busta…: the envelopes that are neither the source nor a line
    /// yet. Picking one adds its line and puts the cursor in its amount.
    private var addRow: some View {
        let candidates = store.flows.filter { flow in
            flow.id != draft.sourceId && !draft.lines.contains { $0.flowId == flow.id }
        }
        return Menu {
            ForEach(candidates, id: \.id) { flow in
                Button(store.flowName(flow)) {
                    draft.add(flow.id)
                    Task { focused = flow.id }
                }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "plus")
                    .font(.system(size: 9, weight: .semibold))
                    .accessibilityHidden(true)
                Text(String(localized: "Add envelope…"))
            }
            .foregroundStyle(Ink.text3)
            .padding(.horizontal, MoveColumn.padding)
            .frame(maxWidth: .infinity, minHeight: MoveColumn.rowHeight, alignment: .leading)
            .contentShape(Rectangle())
        }
        .plainMenu()
        .disabled(candidates.isEmpty)
    }

    // MARK: - The totals

    private func totals(_ check: EnvelopeMoveCheck) -> some View {
        let sourceName = check.source.map { store.flowName($0) } ?? TransactionRow.placeholder
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 18) {
                AmountFigure(label: String(localized: "Total"), amount: check.total, spacing: 5)
                if let after = check.sourceAfter {
                    AmountFigure(label: String(localized: "Left in \(sourceName)"), amount: after, negative: after < 0, spacing: 5)
                }
                if let lowest = check.sourceLowAfter {
                    AmountFigure(
                        label: String(localized: "Lowest on \(RecurringDayText.full(lowest.on))"),
                        amount: lowest.amount,
                        negative: lowest.amount < 0,
                        spacing: 5
                    )
                }
                if !check.rows.isEmpty {
                    Text(CountText.transfers(check.rows.count))
                        .foregroundStyle(Ink.text3)
                }
            }
            .font(Face.row)
            .monospacedDigit()
            switch check.sourceState {
            case .belowZero(_, nil):
                FormNote(String(localized: "\(sourceName) goes below zero, which it allows."), tone: .warning, indented: false)
            case .belowZero(let shortfall, let day?):
                FormNote(
                    String(localized: "\(sourceName) goes \(LedgerMoney.bare(shortfall)) below zero on \(RecurringDayText.full(day)), which it allows."),
                    tone: .warning,
                    indented: false
                )
            case .short(let shortfall, nil):
                FormNote(
                    String(localized: "\(sourceName) is \(LedgerMoney.bare(shortfall)) short and cannot go below zero."),
                    tone: .negative,
                    indented: false
                )
            case .short(let shortfall, let day?):
                FormNote(
                    String(localized: "\(sourceName) is \(LedgerMoney.bare(shortfall)) short on \(RecurringDayText.full(day)) and cannot go below zero."),
                    tone: .negative,
                    indented: false
                )
            case .fine, .gone:
                EmptyView()
            }
        }
        .padding(.top, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .top) { Rectangle().fill(Ink.line2).frame(height: 1) }
    }

    // MARK: - The footer

    /// What keeps Sposta greyed out, at the footer's left: guidance in grey
    /// while the draft is only incomplete, in red when something is wrong.
    private func reason(_ check: EnvelopeMoveCheck) -> some View {
        let words: (String, Bool)? = switch check.blocker {
        case nil:
            nil
        case .noLines:
            (String(localized: "Add an envelope to move money to"), false)
        case .line(let flowId, .empty):
            (String(localized: "Type an amount for \(name(flowId))"), false)
        case .line(_, .gone):
            (String(localized: "Remove the line that is no longer there"), true)
        case .line(let flowId, _):
            (String(localized: "Fix \(name(flowId)) to move"), true)
        case .sourceShort:
            (String(localized: "Lower the amounts to move"), true)
        case .sourceGone:
            (String(localized: "The envelope to move from is no longer there"), true)
        }
        return Text(words?.0 ?? "")
            .font(Face.ui(11.5))
            .foregroundStyle(words?.1 == true ? Ink.negative : Ink.text3)
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func moveTitle(_ check: EnvelopeMoveCheck) -> String {
        check.total > 0
            ? String(localized: "Move \(LedgerMoney.bare(check.total))")
            : String(localized: "Move")
    }

    private func move() async {
        guard !moving else { return }
        moving = true
        defer { moving = false }
        if await store.moveBetweenEnvelopes(draft) {
            dismiss()
        }
    }

    /// An envelope's name as the window shows it, Unallocated localized.
    private func name(_ flowId: Uuid) -> String {
        store.flows.first { $0.id == flowId }.map { store.flowName($0) } ?? TransactionRow.placeholder
    }
}

/// The words under a line that cannot move: a short tag and a sentence.
private struct IssueText {
    let tag: String
    let sentence: String

    init(issue: EnvelopeMoveCheck.Issue, title: String, room: Int64?) {
        switch issue {
        case .overCap(let excess):
            tag = String(localized: "over the cap")
            sentence = String(localized: "\(title) can take \(LedgerMoney.bare(room ?? 0)) more before its cap: \(LedgerMoney.bare(excess)) too much.")
        case .notAnAmount:
            tag = String(localized: "not an amount")
            sentence = String(localized: "Type an amount above zero, such as 250 or 1.000,50.")
        case .tooLarge:
            tag = String(localized: "too large")
            sentence = String(localized: "No envelope can hold that much.")
        case .gone:
            tag = String(localized: "no longer there")
            sentence = String(localized: "Remove its line to move the rest.")
        case .empty:
            tag = ""
            sentence = ""
        }
    }
}
