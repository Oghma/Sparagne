import SwiftUI
import SparagneCore

/// The Ricorrenze tab's inspector (`docs/v2/UI.md` §2.5): one template's
/// fields, edited in place, or a new template written from scratch. The
/// fields are a `RecurringDraft`; Salva sends only what changed, Annulla
/// puts the saved values back, and the next four dates follow every
/// keystroke, asked of the core.
///
/// What `RecurringPatch` cannot say is not offered: the kind is fixed once a
/// template exists, and "any wallet" or Unallocated are greyed out once it
/// has a wallet or an envelope. A vault the account only reads shows the
/// fields without letting them change.
struct RecurringInspector: View {
    let store: AppStore
    /// `nil` = a new template.
    let template: RecurringView?
    let today: NaiveDate
    /// A new template was created and should be selected, or the user gave
    /// up on one (`nil`).
    let finishedCreating: (Uuid?) -> Void
    /// "Duplicate": the tab opens create mode with the draft this inspector
    /// made from the template.
    var duplicate: (RecurringDraft) -> Void = { _ in }

    @State private var draft: RecurringDraft
    /// A command in flight: Salva, Crea and the rest wait for it.
    @State private var working = false
    /// The next four dates (`nextDates`), asked of the core when what they
    /// depend on changes (`PreviewInputs`) rather than on every render: the
    /// inspector draws again on every keystroke, in the amount and the note
    /// too. `nil` until the first answer.
    @State private var preview: Result<[RecurringNext.Preview], Error>?

    init(
        store: AppStore,
        template: RecurringView?,
        seed: RecurringDraft? = nil,
        today: NaiveDate,
        duplicate: @escaping (RecurringDraft) -> Void = { _ in },
        finishedCreating: @escaping (Uuid?) -> Void
    ) {
        self.store = store
        self.template = template
        self.today = today
        self.duplicate = duplicate
        self.finishedCreating = finishedCreating
        _draft = State(
            initialValue: template.map(RecurringDraft.init(template:))
                ?? seed
                ?? RecurringDraft(today: today, owner: store.currentAuthor)
        )
    }

    private var currency: Currency { store.currency }
    private var isNew: Bool { template == nil }
    /// Archived templates are restored before they are edited.
    private var editable: Bool { store.canWrite && !(template?.archived ?? false) && !working }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    title
                    amountAndKind
                    whereGroup
                    whenGroup
                    nextDates
                }
                .disabled(!editable)
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            footer
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Ink.bg)
        // The saved template moved under the open draft (a save, the table's
        // switch, a sync): an untouched draft follows it, an edited one keeps
        // the user's edits and takes the switch only.
        .onChange(of: template) { old, new in
            guard let new else { return }
            let untouched = old.map { !draft.isDirty(against: $0, currency: currency) } ?? true
            if untouched || !draft.isDirty(against: new, currency: currency) {
                draft = RecurringDraft(template: new)
            } else {
                draft.enabled = new.enabled
            }
        }
        .onChange(of: previewInputs, initial: true) { _, inputs in
            preview = Self.previewDates(inputs)
        }
    }

    // MARK: - Heading

    private var title: some View {
        let cadence = ScheduleFormatting.describe(draft.schedule)
        return VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 8) {
                Text(template.map(RecurringTitle.of) ?? String(localized: "New recurring entry"))
                    .font(Face.ui(16, .semibold))
                    .foregroundStyle(Ink.text)
                    .lineLimit(1)
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: 0)
                if let template, store.canWrite { moreMenu(template) }
            }
            Text(String(localized: "\(cadence) \u{00B7} from \(RecurringDayText.full(draft.startDate))"))
            .font(Face.ui(12))
            .foregroundStyle(Ink.text2)
            .lineLimit(2)
        }
    }

    /// The "…" beside the title: duplicating, and the footer's archive or
    /// restore. Not for a vault that is only read, and not in create mode,
    /// where there is nothing to copy yet. Outside the `.disabled` of the
    /// fields, so an archived template can still be restored from it.
    private func moreMenu(_ template: RecurringView) -> some View {
        Menu {
            Button(String(localized: "Duplicate")) {
                duplicate(RecurringDraft(duplicating: template, today: today))
            }
            if template.archived {
                Button(String(localized: "Restore")) {
                    run { await store.restoreRecurring(template.id) }
                }
            } else {
                Button(String(localized: "Archive"), role: .destructive) {
                    run { await store.archiveRecurring(template.id) }
                }
            }
        } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Ink.text2)
                .frame(width: 26, height: 22)
                .contentShape(Rectangle())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .disabled(working)
        .accessibilityLabel(String(localized: "More actions"))
    }

    // MARK: - Amount and kind

    private var amountAndKind: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                TextField(String(localized: "Amount"), text: $draft.amountText, prompt: Text(verbatim: "0,00"))
                    .textFieldStyle(.plain)
                    .labelsHidden()
                    .font(Face.ui(22, .semibold))
                    .foregroundStyle(Ink.text)
                    .accessibilityLabel(String(localized: "Amount"))
                Text(verbatim: "€")
                    .font(Face.ui(13))
                    .foregroundStyle(Ink.text3)
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, 12)
            .frame(height: 40)
            .background(Ink.card, in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Ink.accent, lineWidth: 1))
            .background(RoundedRectangle(cornerRadius: 11).fill(Ink.accent.opacity(0.14)).padding(-3))
            .padding(3)

            RecurringSegments(
                options: [TransactionKind.expense, .income],
                selection: draft.kind,
                label: Self.kindLabel,
                select: { draft.kind = $0 },
                name: String(localized: "Kind")
            )
            // A patch has no kind: once the template exists, it is what it is.
            .disabled(!isNew)
        }
    }

    /// Uscita / Entrata. The income key is not "Income", whose Italian is
    /// the plural "Entrate" of the summary's columns.
    private static func kindLabel(_ kind: TransactionKind) -> String {
        kind == .income
            ? String(localized: "recurring.kind.income", defaultValue: "Income")
            : String(localized: "Expense")
    }

    // MARK: - Dove

    private var whereGroup: some View {
        let names = NameBook(snapshot: store.snapshot)
        return FormGroup(String(localized: "Where")) {
            FormRow(String(localized: "Wallet")) {
                FormMenu(
                    label: String(localized: "Wallet"),
                    value: draft.walletId.map { names.wallet($0) ?? TransactionRow.placeholder }
                        ?? String(localized: "Any wallet")
                ) {
                    Button(String(localized: "Any wallet")) { draft.setWallet(nil, template: template) }
                        .disabled(!RecurringDraft.mayClearWallet(template))
                    Divider()
                    ForEach(store.wallets, id: \.id) { wallet in
                        Button(wallet.name) { draft.setWallet(wallet.id, template: template) }
                    }
                }
            }
            FormRow(String(localized: "Envelope")) {
                FormMenu(
                    label: String(localized: "Envelope"),
                    value: draft.flowId.map { names.flow($0) ?? TransactionRow.placeholder }
                        ?? NameBook.unallocatedLabel
                ) {
                    Button(NameBook.unallocatedLabel) { draft.setFlow(nil, template: template) }
                        .disabled(!RecurringDraft.mayClearFlow(template))
                    Divider()
                    ForEach(store.flows.filter { !$0.isUnallocated }, id: \.id) { flow in
                        Button(flow.name) { draft.setFlow(flow.id, template: template) }
                    }
                }
            }
            FormRow(String(localized: "Category")) {
                FormTextField(label: String(localized: "Category"), text: $draft.category)
            }
            FormRow(String(localized: "Note")) {
                FormTextField(label: String(localized: "Note"), text: $draft.note)
            }
            FormRow(Self.ownerLabel) {
                FormPicker(label: Self.ownerLabel, selection: $draft.owner, options: owners, title: { $0 })
            }
        }
    }

    /// "Titolare": whose the template is. Not the "Owner" of a vault
    /// (Proprietario), hence a key of its own.
    static var ownerLabel: String {
        String(localized: "recurring.owner", defaultValue: "Owner")
    }

    /// Who the template may belong to: the people a row may be for, and the
    /// owner it has now even when that is no longer one of them (a member
    /// who left), so the picker never shows a choice it does not list.
    private var owners: [String] {
        AppStore.distinct(store.assignablePeople + [draft.owner, template?.owner ?? ""])
    }

    // MARK: - Quando

    private var whenGroup: some View {
        FormGroup(String(localized: "When")) {
            RecurringSegments(
                options: RecurringDraft.Cadence.allCases,
                selection: draft.cadence,
                label: Self.cadenceLabel,
                select: { draft.cadence = $0 },
                name: String(localized: "Frequency")
            )
            .padding(.bottom, 2)
            FormRow(String(localized: "Repeat"), alignment: .firstTextBaseline) {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 4) {
                        every
                        on
                    }
                    VStack(alignment: .leading, spacing: 5) {
                        every
                        on
                    }
                }
            }
            FormRow(String(localized: "Start")) {
                DayField(label: String(localized: "Start"), day: $draft.startDate)
            }
            FormRow(String(localized: "End")) {
                HStack(spacing: 8) {
                    RecurringSegments(
                        options: [false, true],
                        selection: draft.hasEndDate,
                        label: { $0 ? String(localized: "On a day\u{2026}") : String(localized: "Never") },
                        select: { draft.hasEndDate = $0 },
                        name: String(localized: "End")
                    )
                    if draft.hasEndDate {
                        DayField(label: String(localized: "End"), day: $draft.endDate)
                    }
                }
            }
        }
    }

    private static func cadenceLabel(_ cadence: RecurringDraft.Cadence) -> String {
        switch cadence {
        case .daily: String(localized: "Day")
        case .weekly: String(localized: "Week")
        case .monthly: String(localized: "Month")
        case .yearly: String(localized: "Year")
        }
    }

    /// "ogni [1] mese,". The spacing is tight on purpose: in English, with
    /// "month," and "on day", the row only just fits the inspector's 340 pt on
    /// one line, and it wraps (`ViewThatFits`) at any looser gap.
    private var every: some View {
        HStack(spacing: 4) {
            Text(String(localized: "every"))
            MiniNumberField(label: String(localized: "Interval"), value: $draft.interval)
            Text(unitWord + (draft.cadence == .daily ? "" : ","))
        }
        .font(Face.ui(12))
        .foregroundStyle(Ink.text)
        .fixedSize()
    }

    /// The unit after the interval, singular for one. Two plain words rather
    /// than a plural key: the number is in the box before it, not in the
    /// sentence.
    private var unitWord: String {
        let one = draft.interval == 1
        switch draft.cadence {
        case .daily: return one ? String(localized: "day") : String(localized: "days")
        case .weekly: return one ? String(localized: "week") : String(localized: "weeks")
        case .monthly: return one ? String(localized: "month") : String(localized: "months")
        case .yearly: return one ? String(localized: "year") : String(localized: "years")
        }
    }

    /// The day the cadence falls on: a weekday, a day of the month, or a
    /// day and a month.
    @ViewBuilder
    private var on: some View {
        HStack(spacing: 4) {
            switch draft.cadence {
            case .daily:
                EmptyView()
            case .weekly:
                Text(String(localized: "on"))
                compactMenu(
                    label: String(localized: "Weekday"),
                    value: ScheduleFormatting.weekdayName(UInt8(clamping: draft.weekday))
                ) {
                    ForEach(1...7, id: \.self) { day in
                        Button(ScheduleFormatting.weekdayName(UInt8(day))) { draft.weekday = day }
                    }
                }
            case .monthly:
                Text(String(localized: "on day"))
                MiniNumberField(label: String(localized: "Day of the month"), value: $draft.monthDay)
            case .yearly:
                Text(String(localized: "on the"))
                MiniNumberField(label: String(localized: "Day of the month"), value: $draft.yearDay)
                compactMenu(
                    label: String(localized: "Month"),
                    value: ScheduleFormatting.monthName(UInt8(clamping: draft.yearMonth))
                ) {
                    ForEach(1...12, id: \.self) { month in
                        Button(ScheduleFormatting.monthName(UInt8(month))) { draft.yearMonth = month }
                    }
                }
            }
        }
        .font(Face.ui(12))
        .foregroundStyle(Ink.text)
        .fixedSize()
    }

    private func compactMenu<Items: View>(label: String, value: String, @ViewBuilder items: () -> Items) -> some View {
        FormMenu(label: label, value: value) { items() }
            .fixedSize()
    }

    // MARK: - Prossime date

    private var nextDates: some View {
        FormGroup(String(localized: "Next dates")) {
            switch preview {
            case nil:
                EmptyView()
            case .success(let dates):
                VStack(spacing: 0) {
                    ForEach(dates, id: \.self) { line in
                        previewLine(line)
                    }
                }
                if dates.isEmpty {
                    Text(String(localized: "No dates left: the end date has passed"))
                        .font(Face.ui(12))
                        .foregroundStyle(Ink.text3)
                }
            case .failure:
                Text(String(localized: "These settings do not make a schedule"))
                    .font(Face.ui(12))
                    .foregroundStyle(Ink.negative)
            }
        }
    }

    /// What the preview is worked out from. Cheap to read on every render,
    /// unlike the answer: the schedule on screen, saved or not; the due
    /// periods, which lead while the schedule is the saved one; and whether
    /// today, for a running template, counts as decided when it is not among
    /// them (`RecurringNext.of`).
    private var previewInputs: PreviewInputs {
        let schedule = draft.schedule
        let unchanged = template.map { $0.schedule == schedule } ?? false
        return PreviewInputs(
            schedule: schedule,
            due: unchanged ? template.map { store.dueDates(of: $0.id) } ?? [] : [],
            today: today,
            running: template.map { $0.enabled && !$0.archived } ?? false
        )
    }

    /// The next four dates of `inputs`, asked of the core.
    private static func previewDates(_ inputs: PreviewInputs) -> Result<[RecurringNext.Preview], Error> {
        let from = inputs.running ? (NaiveDay.adding(1, to: inputs.today) ?? inputs.today) : inputs.today
        return Result {
            try RecurringNext.preview(
                schedule: inputs.schedule,
                due: inputs.due,
                from: from,
                count: 4,
                occurrences: CoreSchedule.occurrences
            )
        }
    }

    private func previewLine(_ line: RecurringNext.Preview) -> some View {
        HStack {
            Text(RecurringDayText.weekdayWithYear(line.date))
                .foregroundStyle(Ink.text)
            Spacer(minLength: 8)
            if line.isDue {
                Text(String(localized: "to confirm"))
                    .font(Face.ui(10, .semibold))
                    .foregroundStyle(Ink.accent)
                    .padding(.horizontal, 5)
                    .frame(height: 16)
                    .background(Ink.accent.opacity(0.14), in: RoundedRectangle(cornerRadius: 4))
            } else if let amount = draft.amount(currency: currency) ?? template?.amount {
                Text(LedgerMoney.bare(amount)).foregroundStyle(Ink.text3)
            }
        }
        .font(Face.ui(12))
        .frame(height: 22)
        .overlay(alignment: .bottom) {
            Line()
                .stroke(Ink.line, style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
                .frame(height: 1)
        }
        .accessibilityElement(children: .combine)
    }

    // MARK: - Footer

    private var footer: some View {
        HStack(spacing: 8) {
            if let template {
                if store.canWrite {
                    FormSwitch(label: String(localized: "Enabled"), isOn: draft.enabled && !template.archived) { on in
                        run { await store.setRecurringEnabled(template.id, on) }
                    }
                    .disabled(template.archived || working)
                    // The switch speaks for itself; the word beside it would
                    // be a second stop saying the same.
                    Text(String(localized: "Enabled"))
                        .font(Face.ui(12))
                        .foregroundStyle(Ink.text2)
                        .accessibilityHidden(true)
                }
                Spacer(minLength: 8)
                if store.canWrite {
                    if template.archived {
                        Button(String(localized: "Restore")) {
                            run { await store.restoreRecurring(template.id) }
                        }
                        .buttonStyle(.chrome(.ghost, small: true))
                    } else if !isDirty {
                        Button {
                            run { await store.archiveRecurring(template.id) }
                        } label: {
                            Text(String(localized: "Archive")).foregroundStyle(Ink.negative)
                        }
                        .buttonStyle(.chrome(.ghost, small: true))
                    }
                    if isDirty {
                        Button(String(localized: "Cancel")) { draft = RecurringDraft(template: template) }
                            .buttonStyle(.chrome(.ghost, small: true))
                            .keyboardShortcut(.cancelAction)
                        if draft.isValid(currency: currency), isPreviewValid {
                            Button(String(localized: "Save")) { save(template) }
                                .buttonStyle(.chrome(.primary, small: true))
                                .keyboardShortcut(.defaultAction)
                        }
                    }
                }
            } else {
                Spacer(minLength: 8)
                Button(String(localized: "Cancel")) { finishedCreating(nil) }
                    .buttonStyle(.chrome(.ghost, small: true))
                    .keyboardShortcut(.cancelAction)
                Button(String(localized: "Create")) { create() }
                    .buttonStyle(.chrome(.primary, small: true))
                    .keyboardShortcut(.defaultAction)
                    .disabled(!draft.isValid(currency: currency) || !isPreviewValid)
            }
        }
        .disabled(working)
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .frame(minHeight: 44)
        .overlay(alignment: .top) { Hairline() }
        .background(Ink.bg)
        .accessibilityElement(children: .contain)
    }

    private var isDirty: Bool {
        guard let template else { return false }
        return draft.isDirty(against: template, currency: currency)
    }

    private var isPreviewValid: Bool {
        if case .success = preview { return true }
        return false
    }

    // MARK: - Actions

    private func save(_ template: RecurringView) {
        let patch = draft.patch(against: template, currency: currency)
        run { await store.updateRecurring(template.id, patch: patch) }
    }

    /// Crea, then the new template selected in the table. A refused one (an
    /// envelope gone since) keeps the draft, so it can be fixed.
    private func create() {
        guard let creation = draft.creation(currency: currency) else { return }
        run {
            let created = await store.createRecurring(
                kind: creation.kind,
                amount: creation.amount,
                walletId: creation.walletId,
                flowId: creation.flowId,
                category: creation.category,
                note: creation.note,
                schedule: creation.schedule,
                owner: creation.owner
            )
            if let created { finishedCreating(created) }
        }
    }

    private func run(_ work: @escaping @MainActor () async -> Void) {
        working = true
        Task {
            await work()
            working = false
        }
    }
}

/// What the inspector's next dates are worked out from, as one value for
/// `.onChange`.
private struct PreviewInputs: Equatable {
    let schedule: Schedule
    let due: [NaiveDate]
    let today: NaiveDate
    let running: Bool
}

/// A horizontal line through the middle of its frame, for a dashed rule.
private struct Line: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.midY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
        return path
    }
}
