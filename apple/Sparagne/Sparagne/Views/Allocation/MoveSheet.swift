import SwiftUI
import SparagneCore

/// Column geometry of the sheet's table. A takes what is left: the sheet's
/// 640 points of content less the rest leave it about 148.
private enum MoveColumn {
    static let nameMinimum: CGFloat = 100
    static let figure: CGFloat = 96
    static let amount: CGFloat = 112
    static let actions: CGFloat = 92
    static let padding: CGFloat = 6
    static let rowHeight: CGFloat = 30
}

/// "Move between flows" or between wallets: money out of one into several at once, outside the plan.
struct MoveSheet: View {
    let store: AppStore

    @Environment(\.dismiss) private var dismiss
    @State private var draft: MoveDraft
    /// The flows or wallets from the draft's day on, read again when the day
    /// or a balance changes: a move dated in the past can only use what the
    /// source held then and still holds at its lowest afterwards. Empty
    /// until the first answer: the targets as they are now stand in, which
    /// is what a move dated today sees.
    @State private var balances: [Uuid: MoveBalance] = [:]
    @State private var moving = false
    @State private var hovered: Uuid?
    @FocusState private var focused: Uuid?

    init(store: AppStore, side: TargetKind = .flow) {
        self.store = store
        let source = Self.defaultSource(side, store: store)
        _draft = State(initialValue: MoveDraft(side: side, sourceId: source ?? "", day: CoreDate.day(Date())))
    }

    /// Where the money leaves from when the sheet opens or changes side:
    /// Unallocated among the flows, the first wallet among the wallets.
    private static func defaultSource(_ side: TargetKind, store: AppStore) -> Uuid? {
        switch side {
        case .flow: (store.flows.first(where: \.isUnallocated) ?? store.flows.first)?.id
        case .wallet: store.wallets.first?.id
        }
    }

    private var isFlows: Bool { draft.side == .flow }

    var body: some View {
        let targets = store.moveTargets(draft.side)
        let check = draft.check(targets: targets, currency: store.currency, balances: balances)
        FormSheet(
            isFlows ? String(localized: "Move between flows") : String(localized: "Move between wallets"),
            subtitle: isFlows
                ? String(localized: "Once only: the allocation plan and its periods stay as they are.")
                : String(localized: "The money changes wallet, not flow: the flows stay as they are."),
            width: 680
        ) {
            FormGroup {
                FormRow(String(localized: "Between")) {
                    SegmentedStrip(
                        options: [TargetKind.flow, .wallet],
                        selection: Binding(get: { draft.side }, set: { setSide($0) }),
                        label: { $0 == .flow ? String(localized: "Flows") : String(localized: "Wallets") },
                        fixed: true
                    )
                    .disabled(moving)
                }
                FormRow(String(localized: "From")) { source(check, targets: targets) }
                FormRow(String(localized: "Date")) {
                    DayField(label: String(localized: "Date"), day: $draft.day)
                        .frame(width: 240)
                }
                FormRow(String(localized: "Note")) {
                    FormTextField(label: String(localized: "Note"), text: $draft.note, prompt: String(localized: "optional"))
                }
            }
            table(check, targets: targets)
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
        // Opened before the vault's flows or wallets were loaded (a launch
        // option can do that): the default source as soon as there is one,
        // unless a source was chosen meanwhile.
        .onChange(of: Self.defaultSource(draft.side, store: store), initial: true) { _, source in
            if draft.sourceId.isEmpty, let source {
                draft.setSource(source)
            }
        }
        // A sync or a row edited elsewhere changes a balance: the day's
        // figures are read again with it. A failed read keeps the last ones
        // and is on the alert; Sposta checks again before it sends anyway.
        .task(id: BalancesKey(side: draft.side, day: draft.day, targets: targets)) {
            if let loaded = await store.moveBalances(draft.side, on: draft.day) {
                balances = loaded
            }
        }
    }

    private struct BalancesKey: Equatable {
        let side: TargetKind
        let day: NaiveDate
        let targets: [MoveTarget]
    }

    /// The other side: its default source and no lines, the day and the
    /// note kept. The figures read for the old side stand for none of the
    /// new one's targets.
    private func setSide(_ side: TargetKind) {
        guard side != draft.side else { return }
        var next = MoveDraft(side: side, sourceId: Self.defaultSource(side, store: store) ?? "", day: draft.day)
        next.note = draft.note
        draft = next
        balances = [:]
        hovered = nil
    }

    // MARK: - Da

    /// The source, and its balance beside it on the day chosen.
    private func source(_ check: MoveCheck, targets: [MoveTarget]) -> some View {
        HStack(spacing: 10) {
            FormPicker(
                label: String(localized: "From"),
                selection: Binding(get: { draft.sourceId }, set: { draft.setSource($0) }),
                options: targets.map(\.id),
                title: { id in name(id, in: targets) }
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

    private func table(_ check: MoveCheck, targets: [MoveTarget]) -> some View {
        VStack(spacing: 0) {
            header
            ForEach(check.rows) { row in
                line(row)
                if let issue = row.issue, issue != .empty {
                    issueLine(row, issue)
                }
            }
            addRow(targets)
        }
        .font(Face.row)
        .monospacedDigit()
    }

    private var header: some View {
        SetupHeaderRow {
            Text(String(localized: "To")).tableCell(minWidth: MoveColumn.nameMinimum, padding: MoveColumn.padding)
            Text(String(localized: "Balance")).tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            // A wallet has no cap, so no room under one.
            if isFlows {
                Text(String(localized: "Room")).tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            }
            Text(String(localized: "Amount")).tableCell(width: MoveColumn.amount, alignment: .trailing, padding: MoveColumn.padding)
            Text(String(localized: "Balance after"))
                .tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            // A text, not EmptyView: a frame around nothing takes no room,
            // and the headings would slide off their columns.
            Text(verbatim: "").tableCell(width: MoveColumn.actions, padding: MoveColumn.padding)
        }
    }

    /// One flow or wallet the money goes to. The row under the pointer or
    /// with the cursor is raised, and offers Al tetto when the flow has room
    /// under a cap; the one being typed in also gets the amber bar.
    private func line(_ row: MoveCheck.Row) -> some View {
        let title = row.target?.name ?? TransactionRow.placeholder
        let active = hovered == row.targetId || focused == row.targetId
        let overCap = if case .overCap = row.issue { true } else { false }
        return SetupRow(
            highlighted: active,
            editing: focused == row.targetId,
            separator: row.issue == nil || row.issue == .empty,
            height: MoveColumn.rowHeight,
            barInset: 4
        ) {
            Text(title).foregroundStyle(row.target == nil ? Ink.text3 : Ink.text)
                .tableCell(minWidth: MoveColumn.nameMinimum, padding: MoveColumn.padding)
            Text(row.balance.map { LedgerMoney.bare($0.balance) } ?? TransactionRow.placeholder)
                .foregroundStyle(Ink.text2)
                .tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            if isFlows {
                Text(row.room.map(LedgerMoney.bare) ?? TransactionRow.placeholder)
                    .foregroundStyle(Ink.text3)
                    .tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            }
            amountField(row, title: title).tableCell(width: MoveColumn.amount, padding: MoveColumn.padding)
            Text(row.after.map(LedgerMoney.bare) ?? TransactionRow.placeholder)
                .foregroundStyle(overCap ? Ink.negative : Ink.text)
                .tableCell(width: MoveColumn.figure, alignment: .trailing, padding: MoveColumn.padding)
            HStack(spacing: 2) {
                if active, let room = row.room, room > 0, row.amount != room {
                    Button(String(localized: "To cap")) { draft.fill(row.targetId, room: room) }
                        .buttonStyle(.chrome(.ghost, small: true))
                }
                SetupIconButton(
                    symbol: "xmark",
                    help: String(localized: "Remove \(title)"),
                    label: String(localized: "Remove \(title)"),
                    symbolSize: 9,
                    symbolWeight: .semibold
                ) {
                    draft.remove(row.targetId)
                }
            }
            .tableCell(width: MoveColumn.actions, alignment: .trailing, padding: MoveColumn.padding)
        }
        .hoverId($hovered, row.targetId)
    }

    private func amountField(_ row: MoveCheck.Row, title: String) -> some View {
        let text = Binding(
            get: { draft.lines.first { $0.targetId == row.targetId }?.amount ?? "" },
            set: { draft.setAmount($0, for: row.targetId) }
        )
        let wrong = row.issue != nil && row.issue != .empty
        let border = wrong ? Ink.negative.opacity(0.6) : focused == row.targetId ? Ink.accent.opacity(0.6) : Ink.line2
        return TextField(String(localized: "Amount for \(title)"), text: text, prompt: Text(verbatim: ""))
            .textFieldStyle(.plain)
            .labelsHidden()
            .multilineTextAlignment(.trailing)
            .foregroundStyle(wrong ? Ink.negative : Ink.text)
            .padding(.horizontal, 6)
            .frame(height: 22)
            .background(Ink.sheet, in: RoundedRectangle(cornerRadius: 5))
            .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(border, lineWidth: 1))
            .focused($focused, equals: row.targetId)
            .accessibilityLabel(String(localized: "Amount for \(title)"))
    }

    /// What is wrong with a line, under it: a tag, a sentence, and Al tetto
    /// with the amount that fits when the line is over its cap.
    private func issueLine(_ row: MoveCheck.Row, _ issue: MoveCheck.Issue) -> some View {
        let title = row.target?.name ?? TransactionRow.placeholder
        let words = IssueText(issue: issue, title: title, room: row.room, side: draft.side)
        return HStack(spacing: 8) {
            RowTag(text: words.tag, tint: Ink.negative, opacity: 0.14)
            Text(words.sentence)
                .font(Face.ui(11.5))
                .foregroundStyle(Ink.text2)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            if case .overCap = issue, let room = row.room, room > 0 {
                Button(String(localized: "To cap: \(LedgerMoney.bare(room))")) { draft.fill(row.targetId, room: room) }
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

    /// Aggiungi flusso… or wallet…: the targets that are neither the source
    /// nor a line yet. Picking one adds its line and puts the cursor in its
    /// amount.
    private func addRow(_ targets: [MoveTarget]) -> some View {
        let candidates = targets.filter { target in
            target.id != draft.sourceId && !draft.lines.contains { $0.targetId == target.id }
        }
        return Menu {
            ForEach(candidates, id: \.id) { target in
                Button(target.name) {
                    draft.add(target.id)
                    Task { focused = target.id }
                }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "plus")
                    .font(.system(size: 9, weight: .semibold))
                    .accessibilityHidden(true)
                Text(isFlows ? String(localized: "Add flow…") : String(localized: "Add wallet…"))
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

    private func totals(_ check: MoveCheck) -> some View {
        let sourceName = check.source?.name ?? TransactionRow.placeholder
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
    private func reason(_ check: MoveCheck) -> some View {
        let name = { (id: Uuid) in check.rows.first { $0.targetId == id }?.target?.name ?? TransactionRow.placeholder }
        let words: (String, Bool)? = switch check.blocker {
        case nil:
            nil
        case .noLines:
            (isFlows ? String(localized: "Add a flow to move money to") : String(localized: "Add a wallet to move money to"), false)
        case .line(let targetId, .empty):
            (String(localized: "Type an amount for \(name(targetId))"), false)
        case .line(_, .gone):
            (String(localized: "Remove the line that is no longer there"), true)
        case .line(let targetId, _):
            (String(localized: "Fix \(name(targetId)) to move"), true)
        case .sourceShort:
            (String(localized: "Lower the amounts to move"), true)
        case .sourceGone:
            (
                isFlows
                    ? String(localized: "The flow to move from is no longer there")
                    : String(localized: "The wallet to move from is no longer there"),
                true
            )
        }
        return Text(words?.0 ?? "")
            .font(Face.ui(11.5))
            .foregroundStyle(words?.1 == true ? Ink.negative : Ink.text3)
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func moveTitle(_ check: MoveCheck) -> String {
        check.total > 0
            ? String(localized: "Move \(LedgerMoney.bare(check.total))")
            : String(localized: "Move")
    }

    private func move() async {
        guard !moving else { return }
        moving = true
        defer { moving = false }
        if await store.move(draft) {
            dismiss()
        }
    }

    /// A flow's or wallet's name as the window shows it, Unallocated
    /// localized.
    private func name(_ id: Uuid, in targets: [MoveTarget]) -> String {
        targets.first { $0.id == id }?.name ?? TransactionRow.placeholder
    }
}

/// The words under a line that cannot move: a short tag and a sentence.
private struct IssueText {
    let tag: String
    let sentence: String

    init(issue: MoveCheck.Issue, title: String, room: Int64?, side: TargetKind) {
        switch issue {
        case .overCap(let excess):
            tag = String(localized: "over the cap")
            sentence = String(localized: "\(title) can take \(LedgerMoney.bare(room ?? 0)) more before its cap: \(LedgerMoney.bare(excess)) too much.")
        case .notAnAmount:
            tag = String(localized: "not an amount")
            sentence = String(localized: "Type an amount above zero, such as 250 or 1.000,50.")
        case .tooLarge:
            tag = String(localized: "too large")
            sentence = side == .flow
                ? String(localized: "No flow can hold that much.")
                : String(localized: "No wallet can hold that much.")
        case .gone:
            tag = String(localized: "no longer there")
            sentence = String(localized: "Remove its line to move the rest.")
        case .empty:
            tag = ""
            sentence = ""
        }
    }
}
