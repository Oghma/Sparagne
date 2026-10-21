import SwiftUI
import SparagneCore

/// The Ricorrenze tab's inspector: a template's fields edited in place, or a new template's.
struct RecurringInspector: View {
    let store: AppStore
    /// `nil` = a new template.
    let template: RecurringView?
    let today: NaiveDate
    /// A template was created and should be selected, a new one or the copy
    /// a kind change left in the place of the open one, or the user gave up
    /// on a new one (`nil`).
    let finishedCreating: (Uuid?) -> Void
    /// "Duplicate": the tab opens create mode with the draft this inspector
    /// made from the template.
    var duplicate: (RecurringDraft) -> Void = { _ in }

    @State private var draft: RecurringDraft
    /// A command in flight: Salva, Crea and the rest wait for it.
    @State private var working = false
    /// The next four dates, asked of the core when what they depend on
    /// changes (`RecurringPreview.Inputs`) rather than on every render: the
    /// inspector draws again on every keystroke, in the amount and the note
    /// too. `nil` until the first answer.
    @State private var preview: Result<RecurringPreview, Error>?
    /// The periods of the open template already recorded or skipped, left out
    /// of the due periods of a schedule being edited. Empty for a new
    /// template.
    @State private var handled: Set<NaiveDate> = []
    /// The periods an archived template missed, which Ripristina would put
    /// back to confirm, over a copy that may have taken them already.
    @State private var missed = 0
    /// Where the copy of the other kind would start, or why the template
    /// cannot change kind now; `nil` until the core answers, and the item
    /// stays off meanwhile.
    @State private var kindChange: Result<NaiveDate, RecurringKindChange.Refusal>?
    /// The day "Change to…" asks about, while its alert is up.
    @State private var confirmingKind: NaiveDate?
    /// The list under the CATEGORY field (`CategoryFormField`).
    @State private var completion = CategoryCompletionModel()

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
                ?? RecurringDraft(today: today, owner: store.currentAuthor, walletId: store.defaultWalletId)
        )
    }

    private var currency: Currency { store.currency }
    private var isNew: Bool { template == nil }
    /// Archived templates are restored before they are edited.
    private var editable: Bool { store.canWrite && !(template?.archived ?? false) && !working }

    var body: some View {
        InspectorShell(
            title: template.map(RecurringTitle.of) ?? String(localized: "New recurring entry"),
            subtitle: String(localized: "\(ScheduleFormatting.describe(draft.schedule)) \u{00B7} from \(RecurringDayText.full(draft.when.startDate))"),
            fieldsDisabled: !editable,
            footerDisabled: working
        ) {
            if let template, store.canWrite { moreMenu(template) }
        } content: {
            amountAndKind
            whereGroup
            ScheduleEditor(draft: $draft.when)
            nextDates
            if let template, template.archived, missed > 0, store.canWrite {
                Text(String(localized: "Restoring it puts \(missed) missed periods back to confirm"))
                    .font(Face.footnote)
                    .foregroundStyle(Ink.text2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        } footer: {
            footer
        }
        .hangingCategoryList(completion)
        .alert(
            template.map { Self.kindQuestion(RecurringKindChange.otherKind(of: $0.kind)) } ?? "",
            isPresented: Binding(get: { confirmingKind != nil }, set: { if !$0 { confirmingKind = nil } }),
            presenting: confirmingKind
        ) { _ in
            Button(String(localized: "Change Kind")) { changeKind() }
            Button(String(localized: "Cancel"), role: .cancel) {}
        } message: { start in
            Text(String(localized: "The template is archived and a new one starts on \(RecurringDayText.short(start, today: today)). This cannot be undone."))
        }
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
            preview = RecurringPreview.of(inputs, occurrences: CoreSchedule.occurrences)
        }
        // A period recorded or skipped from the due sheet changes the saved
        // template's due list, and with it what has been handled; archiving
        // it and the day turning change what Ripristina would bring back.
        .task(id: HandledKey(
            templateId: template?.id,
            archived: template?.archived ?? false,
            due: template.map { store.dueDates(of: $0.id) } ?? [],
            today: today
        )) {
            handled = if let template { await store.handledPeriods(of: template.id) } else { [] }
            missed = template.map { Self.missedPeriods($0, handled: handled, today: today) } ?? 0
        }
        // "Change to…" asks the core again whenever its answer may move: the
        // template itself (archived, paused, edited), its due list, who is a
        // member, whether the vault may be written, the day.
        .task(id: KindChangeKey(
            template: template,
            due: template.map { store.dueDates(of: $0.id) } ?? [],
            members: store.currentMembers,
            canWrite: store.canWrite,
            today: today
        )) {
            kindChange = if let template { await store.recurringKindStart(template.id) } else { nil }
        }
    }

    // MARK: - Heading

    /// The "…" beside the title: duplicating, changing the kind, and the
    /// footer's archive or restore. Not for a vault that is only read, and
    /// not in create mode, where there is nothing to copy yet. Inside the
    /// fields' `.disabled`, as the heading has always been: an archived
    /// template is restored from the footer.
    private func moreMenu(_ template: RecurringView) -> some View {
        Menu {
            Button(String(localized: "Duplicate")) {
                duplicate(RecurringDraft(duplicating: template, today: today, defaultWalletId: store.defaultWalletId))
            }
            kindItem(template)
            if template.archived {
                Button(String(localized: "Restore")) {
                    $working.run { await store.restoreRecurring(template.id) }
                }
            } else {
                Button(String(localized: "Archive"), role: .destructive) {
                    $working.run { await store.archiveRecurring(template.id) }
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

    /// "Change to Income…" or "Change to Expense…", off with the reason
    /// under it while the template cannot change kind, or has edits not
    /// saved, which the copy, made from the saved template, would drop.
    private func kindItem(_ template: RecurringView) -> some View {
        let reason: String? = if isDirty {
            String(localized: "Save or cancel the changes first")
        } else if case .failure(let refusal) = kindChange {
            refusal.reason
        } else {
            nil
        }
        return Button {
            if case .success(let start) = kindChange { confirmingKind = start }
        } label: {
            Text(Self.kindItemTitle(RecurringKindChange.otherKind(of: template.kind)))
            if let reason { Text(reason) }
        }
        .disabled(reason != nil || kindChange == nil)
    }

    private static func kindItemTitle(_ kind: TransactionKind) -> String {
        kind == .income
            ? String(localized: "Change to Income\u{2026}")
            : String(localized: "Change to Expense\u{2026}")
    }

    /// The alert's question before the change, which no undo takes back.
    private static func kindQuestion(_ kind: TransactionKind) -> String {
        kind == .income
            ? String(localized: "Change to an income?")
            : String(localized: "Change to an expense?")
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
                selection: $draft.kind,
                label: Self.kindLabel,
                name: String(localized: "Kind")
            )
            // A patch has no kind: once the template exists, the "…" menu
            // changes it, into a copy.
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
        let wallets = store.wallets.count
        return FormGroup(String(localized: "Where")) {
            FormRow(String(localized: "Wallet")) {
                FormMenu(
                    label: String(localized: "Wallet"),
                    value: draft.walletId.map { names.wallet($0) ?? TransactionRow.placeholder }
                        ?? String(localized: "Any wallet")
                ) {
                    if RecurringDraft.offersAnyWallet(wallets: wallets) {
                        Button(String(localized: "Any wallet")) { draft.setWallet(nil, template: template, wallets: wallets) }
                            .disabled(!RecurringDraft.mayClearWallet(template, wallets: wallets))
                        Divider()
                    }
                    ForEach(store.wallets, id: \.id) { wallet in
                        Button(wallet.name) { draft.setWallet(wallet.id, template: template, wallets: wallets) }
                    }
                }
            }
            // Saved without a wallet in a vault of several: why recording a
            // period fails, until a wallet is picked and saved.
            if let template, store.lacksWallet(template), draft.walletId == nil {
                Text(AppStore.walletNeededExplanation)
                    .font(Face.footnote)
                    .foregroundStyle(Ink.negative)
                    .fixedSize(horizontal: false, vertical: true)
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
                CategoryFormField(label: String(localized: "Category"), text: $draft.category, store: store, completion: completion)
            }
            FormRow(String(localized: "Note")) {
                FormTextField(label: String(localized: "Note"), text: $draft.note)
            }
            FormRow(Self.ownerLabel) {
                FormPicker(label: Self.ownerLabel, selection: $draft.owner, options: owners, title: ownerTitle)
            }
            // The saved owner left the vault and the draft still has them:
            // why Registra is off, until another owner is picked and saved.
            if let template, store.ownerHasLeft(template), draft.owner == template.owner {
                Text(AppStore.ownerLeftExplanation)
                    .font(Face.footnote)
                    .foregroundStyle(Ink.negative)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// A name in the owner picker, with a note when it is someone who left
    /// the vault: the members are what it offers, the former owner is there
    /// only so the picker shows what the template has.
    private func ownerTitle(_ name: String) -> String {
        guard AppStore.ownerHasLeft(name, members: store.currentMembers, author: store.currentAuthor) else { return name }
        return String(localized: "\(name) (no longer a member)")
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

    // MARK: - Prossime date

    private var nextDates: some View {
        FormGroup(String(localized: "Next dates")) {
            switch preview {
            case nil:
                EmptyView()
            case .success(let dates):
                VStack(spacing: 0) {
                    ForEach(dates.lines, id: \.self) { line in
                        previewLine(line)
                    }
                }
                if dates.moreDue > 0 {
                    Text(String(localized: "and \(dates.moreDue) more to confirm"))
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                }
                if dates.lines.isEmpty {
                    Text(String(localized: "No dates left: the end date has passed"))
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                }
            case .failure:
                Text(String(localized: "These settings do not make a schedule"))
                    .font(Face.row)
                    .foregroundStyle(Ink.negative)
            }
        }
    }

    /// Cheap to read on every render, unlike the answer.
    private var previewInputs: RecurringPreview.Inputs {
        RecurringPreview.Inputs(
            schedule: draft.schedule,
            template: template,
            savedDue: store.dueDates(of:),
            handled: handled,
            today: today
        )
    }

    private func previewLine(_ line: RecurringNext.Preview) -> some View {
        HStack {
            Text(RecurringDayText.weekdayWithYear(line.date))
                .foregroundStyle(Ink.text)
            Spacer(minLength: 8)
            if line.isDue {
                RowTag(text: String(localized: "to confirm"), tint: Ink.accent, opacity: 0.14)
            } else if let amount = draft.amount(currency: currency) ?? template?.amount {
                Text(LedgerMoney.bare(amount)).foregroundStyle(Ink.text3)
            }
        }
        .font(Face.row)
        .frame(height: 22)
        .overlay(alignment: .bottom) { DashedRule() }
        .accessibilityElement(children: .combine)
    }

    // MARK: - Footer

    @ViewBuilder
    private var footer: some View {
        if let template {
            if store.canWrite {
                InspectorSwitch(
                    label: String(localized: "Enabled"),
                    isOn: draft.enabled && !template.archived,
                    disabled: template.archived || working
                ) { on in
                    $working.run { await store.setRecurringEnabled(template.id, on) }
                }
            }
            Spacer(minLength: 8)
            if store.canWrite {
                if template.archived {
                    Button(String(localized: "Restore")) {
                        $working.run { await store.restoreRecurring(template.id) }
                    }
                    .buttonStyle(.chrome(.ghost, small: true))
                } else if !isDirty {
                    Button {
                        $working.run { await store.archiveRecurring(template.id) }
                    } label: {
                        Text(String(localized: "Archive")).foregroundStyle(Ink.negative)
                    }
                    .buttonStyle(.chrome(.ghost, small: true))
                }
                if isDirty {
                    Button(String(localized: "Cancel")) { draft = RecurringDraft(template: template) }
                        .buttonStyle(.chrome(.ghost, small: true))
                        .keyboardShortcut(shortcut(.cancelAction))
                    if draft.isValid(currency: currency), isPreviewValid {
                        Button(String(localized: "Save")) { save(template) }
                            .buttonStyle(.chrome(.primary, small: true))
                            .keyboardShortcut(shortcut(.defaultAction))
                    }
                }
            }
        } else {
            Spacer(minLength: 8)
            Button(String(localized: "Cancel")) { finishedCreating(nil) }
                .buttonStyle(.chrome(.ghost, small: true))
                .keyboardShortcut(shortcut(.cancelAction))
            Button(String(localized: "Create")) { create() }
                .buttonStyle(.chrome(.primary, small: true))
                .keyboardShortcut(shortcut(.defaultAction))
                .disabled(!draft.isValid(currency: currency) || !isPreviewValid)
        }
    }

    private var isDirty: Bool {
        guard let template else { return false }
        return draft.isDirty(against: template, currency: currency)
    }

    /// ↩ and esc for the footer's buttons, except while the CATEGORY list is
    /// open: they are the list's then (`completing`), as the riparto
    /// inspector leaves them to an open row of the plan. A shortcut would
    /// take the key before the field saw it, and ↩ would pick and save at
    /// once.
    private func shortcut(_ key: KeyboardShortcut) -> KeyboardShortcut? {
        completion.isOpen ? nil : key
    }

    private var isPreviewValid: Bool {
        if case .success = preview { return true }
        return false
    }

    // MARK: - Actions

    private func save(_ template: RecurringView) {
        let patch = draft.patch(against: template, currency: currency)
        $working.run { await store.updateRecurring(template.id, patch: patch) }
    }

    /// Crea, then the new template selected in the table. A refused one (an
    /// envelope gone since) keeps the draft, so it can be fixed.
    private func create() {
        guard let creation = draft.creation(currency: currency) else { return }
        $working.run {
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

    /// The template archived and its copy of the other kind written, then
    /// the copy selected in its place. A refusal (a wallet archived since)
    /// keeps this one open, with the alert up.
    private func changeKind() {
        guard let template else { return }
        $working.run {
            if let copy = await store.changeRecurringKind(template.id) { finishedCreating(copy) }
        }
    }

    /// The periods of an archived template that Ripristina would put back to
    /// confirm: those up to today it never decided. None for a paused one,
    /// which comes back paused.
    private static func missedPeriods(_ template: RecurringView, handled: Set<NaiveDate>, today: NaiveDate) -> Int {
        guard template.archived, template.enabled else { return 0 }
        let missed = try? RecurringNext.due(
            schedule: template.schedule,
            today: today,
            handled: handled,
            occurrences: CoreSchedule.occurrences
        )
        return missed?.count ?? 0
    }
}

/// When the inspector asks the core again for the periods already handled:
/// another template, a change to its due list, the template archived, or
/// another day.
private struct HandledKey: Equatable {
    let templateId: Uuid?
    let archived: Bool
    let due: [NaiveDate]
    let today: NaiveDate
}

/// When "Change to…" asks the core again whether the template may change
/// kind, and from which day.
private struct KindChangeKey: Equatable {
    let template: RecurringView?
    let due: [NaiveDate]
    let members: [String]?
    let canWrite: Bool
    let today: NaiveDate
}
