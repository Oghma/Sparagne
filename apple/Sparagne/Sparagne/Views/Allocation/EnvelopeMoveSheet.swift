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

/// "Move between envelopes": money out of one envelope into several in one
/// go, once, outside the allocation plan. Da, Data and Nota hold for every
/// line; each line is an envelope the money goes to, with what it holds, the
/// room under its cap, the amount and what it will hold. Under the table the
/// total and what the source is left with.
///
/// A line over its cap, an amount that is not one, or a source that would go
/// below zero when it may not keeps Sposta greyed out, and the footer says
/// which line to fix. Unallocated below zero is only pointed out, as the
/// Riparto's preview does. The rules are `EnvelopeMoveCheck`'s; the store
/// sends the lines as one batch (`AppStore.moveBetweenEnvelopes`).
struct EnvelopeMoveSheet: View {
    let store: AppStore

    @Environment(\.dismiss) private var dismiss
    @State private var draft: EnvelopeMoveDraft
    @State private var moving = false
    @State private var hovered: Uuid?
    @FocusState private var focused: Uuid?

    init(store: AppStore) {
        self.store = store
        let source = store.flows.first(where: \.isUnallocated) ?? store.flows.first
        _draft = State(initialValue: EnvelopeMoveDraft(sourceId: source?.id ?? "", day: CoreDate.day(Date())))
    }

    var body: some View {
        let check = draft.check(flows: store.flows, currency: store.currency)
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
    }

    // MARK: - Da

    /// The source, and its balance beside it.
    private func source(_ check: EnvelopeMoveCheck) -> some View {
        HStack(spacing: 10) {
            FormPicker(
                label: String(localized: "From"),
                selection: Binding(get: { draft.sourceId }, set: { draft.setSource($0) }),
                options: store.flows.map(\.id),
                title: name
            )
            .frame(width: 240)
            if let balance = check.source?.balance {
                Text(String(localized: "balance \(LedgerMoney.bare(balance))"))
                    .font(Face.ui(12))
                    .foregroundStyle(Ink.text3)
                    .monospacedDigit()
            }
        }
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
        .font(Face.ui(12))
        .monospacedDigit()
    }

    private var header: some View {
        HStack(spacing: 0) {
            cell(minWidth: MoveColumn.envelopeMinimum) { Text(String(localized: "To")) }
            cell(width: MoveColumn.figure, alignment: .trailing) { Text(String(localized: "Balance")) }
            cell(width: MoveColumn.figure, alignment: .trailing) { Text(String(localized: "Room")) }
            cell(width: MoveColumn.amount, alignment: .trailing) { Text(String(localized: "Amount")) }
            cell(width: MoveColumn.figure, alignment: .trailing) { Text(String(localized: "Balance after")) }
            // A text, not EmptyView: a frame around nothing takes no room,
            // and the headings would slide off their columns.
            cell(width: MoveColumn.actions) { Text(verbatim: "") }
        }
        .font(Face.label)
        .foregroundStyle(Ink.text3)
        .frame(height: Metrics.headerHeight)
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
    }

    /// One envelope the money goes to. The row under the pointer or with the
    /// cursor is raised, and offers Al tetto when the envelope has room
    /// under a cap; the one being typed in also gets the amber bar.
    private func line(_ row: EnvelopeMoveCheck.Row) -> some View {
        let title = row.envelope.map { store.flowName($0) } ?? TransactionRow.placeholder
        let active = hovered == row.flowId || focused == row.flowId
        let overCap = if case .overCap = row.issue { true } else { false }
        return HStack(spacing: 0) {
            cell(minWidth: MoveColumn.envelopeMinimum) {
                Text(title).foregroundStyle(row.envelope == nil ? Ink.text3 : Ink.text)
            }
            cell(width: MoveColumn.figure, alignment: .trailing) {
                Text(row.envelope.map { LedgerMoney.bare($0.balance) } ?? TransactionRow.placeholder)
                    .foregroundStyle(Ink.text2)
            }
            cell(width: MoveColumn.figure, alignment: .trailing) {
                Text(row.room.map(LedgerMoney.bare) ?? TransactionRow.placeholder)
                    .foregroundStyle(Ink.text3)
            }
            cell(width: MoveColumn.amount) { amountField(row, title: title) }
            cell(width: MoveColumn.figure, alignment: .trailing) {
                Text(row.after.map(LedgerMoney.bare) ?? TransactionRow.placeholder)
                    .foregroundStyle(overCap ? Ink.negative : Ink.text)
            }
            cell(width: MoveColumn.actions, alignment: .trailing) {
                HStack(spacing: 2) {
                    if active, let room = row.room, room > 0, row.amount != room {
                        Button(String(localized: "To cap")) { draft.fill(row.flowId, room: room) }
                            .buttonStyle(.chrome(.ghost, small: true))
                    }
                    Button {
                        draft.remove(row.flowId)
                    } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 9, weight: .semibold))
                            .foregroundStyle(Ink.text3)
                            .frame(width: 20, height: 20)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help(String(localized: "Remove \(title)"))
                    .accessibilityLabel(String(localized: "Remove \(title)"))
                }
            }
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
        .onHover { inside in
            if inside {
                hovered = row.flowId
            } else if hovered == row.flowId {
                hovered = nil
            }
        }
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
            Text(words.tag)
                .font(Face.ui(10, .semibold))
                .foregroundStyle(Ink.negative)
                .padding(.horizontal, 5)
                .frame(height: 16)
                .background(Ink.negative.opacity(0.14), in: RoundedRectangle(cornerRadius: 4))
                .fixedSize()
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
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .disabled(candidates.isEmpty)
    }

    // MARK: - The totals

    private func totals(_ check: EnvelopeMoveCheck) -> some View {
        let sourceName = check.source.map { store.flowName($0) } ?? TransactionRow.placeholder
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 18) {
                figure(String(localized: "Total"), check.total)
                if let after = check.sourceAfter {
                    figure(String(localized: "Left in \(sourceName)"), after, negative: after < 0)
                }
                if !check.rows.isEmpty {
                    Text(CountText.transfers(check.rows.count))
                        .foregroundStyle(Ink.text3)
                }
            }
            .font(Face.ui(12))
            .monospacedDigit()
            switch check.sourceState {
            case .belowZero:
                FormNote(String(localized: "\(sourceName) goes below zero, which it allows."), tone: .warning, indented: false)
            case .short(let shortfall):
                FormNote(
                    String(localized: "\(sourceName) is \(LedgerMoney.bare(shortfall)) short and cannot go below zero."),
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

    private func figure(_ label: String, _ amount: Int64, negative: Bool = false) -> some View {
        HStack(spacing: 5) {
            Text(label).foregroundStyle(Ink.text3)
            Text(LedgerMoney.bare(amount))
                .fontWeight(.semibold)
                .foregroundStyle(negative ? Ink.negative : Ink.text)
        }
        .accessibilityElement(children: .combine)
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

    // MARK: - Pieces

    /// An envelope's name as the window shows it, Unallocated localized.
    private func name(_ flowId: Uuid) -> String {
        store.flows.first { $0.id == flowId }.map { store.flowName($0) } ?? TransactionRow.placeholder
    }

    private func cell<Content: View>(
        width: CGFloat? = nil,
        minWidth: CGFloat? = nil,
        alignment: Alignment = .leading,
        @ViewBuilder content: () -> Content
    ) -> some View {
        content()
            .lineLimit(1)
            .truncationMode(.tail)
            .padding(.horizontal, MoveColumn.padding)
            .frame(
                minWidth: width ?? minWidth,
                idealWidth: width,
                maxWidth: width ?? .infinity,
                alignment: alignment
            )
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
