import SwiftUI
import SparagneCore

/// «Aggiungi evento»: a rate change or an early repayment, with what it
/// would do to the schedule beside what it says now, and for a repayment the
/// expense that records it in the ledger.
struct LoanEventSheet: View {
    let store: AppStore
    let loan: LoanView

    @Environment(\.dismiss) private var dismiss
    @State private var draft: LoanEventDraft
    /// The saved schedule, asked of the core once.
    @State private var current: LoanSchedule?
    /// The schedule with the event as typed, and with the other mode of a
    /// repayment; `nil` while the event makes none.
    @State private var next: LoanSchedule?
    @State private var other: LoanSchedule?
    /// The debt left on the day chosen, the most a repayment can take
    /// (`AppStore.residualDebt`); `nil` until the core answers.
    @State private var residual: Int64?
    @State private var working = false
    /// The list under the CATEGORY field (`CategoryFormField`).
    @State private var completion = CategoryCompletionModel()

    init(store: AppStore, loan: LoanView) {
        self.store = store
        self.loan = loan
        // On the next installment's day, where a bank takes a repayment.
        let next = store.loanOverview.first { $0.loanId == loan.id }?.nextDue
        var draft = LoanEventDraft(date: next.flatMap { CoreDate.localDay($0) } ?? Date())
        draft.expense = Self.expenseTarget(loan, store: store)
        _draft = State(initialValue: draft)
    }

    private var event: LoanEvent? { try? checked.get() }

    /// The event as typed, held to the loan's dates and, for a repayment,
    /// to the debt left on its day.
    private var checked: Result<LoanEvent, LoanDraftError> {
        draft.event(currency: store.currency, terms: loan.terms, residual: residual)
    }
    private var day: NaiveDate { CoreDate.day(draft.date) }
    private var template: RecurringView? {
        loan.recurringId.flatMap { id in store.recurringTemplates.first { $0.id == id } }
    }

    var body: some View {
        FormSheet(String(localized: "Add event"), subtitle: subtitle, width: 600) {
            SegmentedStrip(
                options: [LoanEventDraft.Kind.rateChange, .prepayment],
                selection: $draft.kind,
                label: { $0 == .rateChange ? String(localized: "Rate change") : String(localized: "Early repayment") },
                name: String(localized: "Event"),
                fixed: true
            )
            FormGroup {
                FormRow(String(localized: "Date")) {
                    LoanDayField(label: String(localized: "Date"), date: $draft.date).frame(width: 200)
                }
                switch draft.kind {
                case .rateChange: rateRows
                case .prepayment: prepaymentRows
                }
            }
            effectGroup
            if draft.kind == .prepayment { expenseGroup }
        } footer: {
            footerNote
            FormCancelButton(shortcut: !completion.isOpen) { dismiss() }
                .disabled(working)
            FormPrimaryButton(title: String(localized: "Add event"), shortcut: !completion.isOpen) { submit() }
                .disabled(event == nil || working)
        }
        .hangingCategoryList(completion)
        .task(id: loan) {
            current = await store.loanSchedule(loan.id)
        }
        .task(id: ResidualKey(loan: loan, day: day)) {
            residual = await store.residualDebt(of: loan, on: day)
        }
        .task(id: event) {
            guard let event else {
                next = nil
                other = nil
                return
            }
            next = await store.previewLoan(terms: loan.terms, events: events(with: event))
            if let flipped = LoanEventEffect.otherMode(event) {
                other = await store.previewLoan(terms: loan.terms, events: events(with: flipped))
            } else {
                other = nil
            }
        }
    }

    private struct ResidualKey: Equatable {
        let loan: LoanView
        let day: NaiveDate
    }

    private func events(with event: LoanEvent) -> [LoanEventView] {
        loan.events + [LoanEventView(id: LoanEventEffect.previewId, event: event)]
    }

    /// «Mutuo casa · debito residuo 152.081,25 al 1 dic 2026, dopo la rata.»
    private var subtitle: String {
        guard let residual else { return loan.name }
        let amount = LedgerMoney.bare(residual)
        let date = RecurringDayText.full(day)
        let afterInstallment = current?.rows.contains { $0.dueDate == day } ?? false
        return afterInstallment
            ? String(localized: "\(loan.name) \u{00B7} remaining debt \(amount) on \(date), after the installment.")
            : String(localized: "\(loan.name) \u{00B7} remaining debt \(amount) on \(date).")
    }

    // MARK: - The fields

    @ViewBuilder
    private var rateRows: some View {
        FormRow(String(localized: "New rate")) {
            UnitField(label: String(localized: "New rate"), text: $draft.rate, unit: "%").frame(width: 100)
        }
        FormRow(String(localized: "Installment")) {
            HStack(spacing: 10) {
                SegmentedStrip(
                    options: [LoanDraft.InstallmentMode.computed, .contract],
                    selection: $draft.installmentMode,
                    label: { $0 == .computed ? String(localized: "Computed") : String(localized: "As in the bank's letter") },
                    name: String(localized: "Installment"),
                    fixed: true
                )
                if draft.installmentMode == .contract {
                    UnitField(label: String(localized: "Installment"), text: $draft.installment, unit: "\u{20AC}")
                        .frame(width: 130)
                    // The loan's own fee per installment comes on top.
                    Text(String(localized: "without the fee per installment"))
                        .font(Face.ui(12))
                        .foregroundStyle(Ink.text3)
                        .lineLimit(1)
                }
            }
        }
    }

    @ViewBuilder
    private var prepaymentRows: some View {
        FormRow(String(localized: "Amount")) {
            HStack(spacing: 10) {
                UnitField(label: String(localized: "Amount"), text: $draft.amount, unit: "\u{20AC}").frame(width: 140)
                // The whole debt left: the loan paid off, its penalty on top.
                if let residual {
                    if residual > 0 {
                        Button(String(localized: "All of it: \(LedgerMoney.bare(residual))")) {
                            draft.amount = LedgerMoney.bare(residual)
                        }
                        .buttonStyle(.chrome(.ghost, small: true))
                    }
                }
            }
        }
        FormRow(String(localized: "loan.mode", defaultValue: "Mode")) {
            SegmentedStrip(
                options: [PrepaymentMode.shorten, .lower],
                selection: $draft.mode,
                label: WealthText.mode,
                name: String(localized: "loan.mode", defaultValue: "Mode"),
                fixed: true
            )
        }
        FormRow(String(localized: "Penalty")) {
            HStack(spacing: 10) {
                UnitField(label: String(localized: "Penalty"), text: $draft.fee, unit: "\u{20AC}", prompt: "0,00")
                    .frame(width: 140)
                if case .prepayment(_, let amount, _, let fee)? = event, fee > 0,
                   let share = LedgerMoney.percent(fee, of: amount) {
                    Text(String(localized: "\(share) of the amount"))
                        .font(Face.ui(12))
                        .foregroundStyle(Ink.text3)
                }
            }
        }
    }

    // MARK: - The preview

    @ViewBuilder
    private var effectGroup: some View {
        if let current, let next, let event {
            let effect = LoanEventEffect.of(current: current, next: next, date: day)
            VStack(alignment: .leading, spacing: 8) {
                PreviewBlock(figures: figures(effect), columns: 3)
                if let other, let otherMode = LoanEventEffect.otherMode(event) {
                    FormNote(
                        alternative(LoanEventEffect.of(current: current, next: other, date: day), mode: otherMode),
                        indented: false
                    )
                }
                ForEach(effect.newWarnings, id: \.self) { warning in
                    FormNote(WealthText.warning(warning), tone: .warning, indented: false)
                }
                if let template, draft.kind == .rateChange {
                    FormNote(templateNote(template, effect: effect), indented: false)
                }
            }
        }
    }

    private func figures(_ effect: LoanEventEffect) -> [PreviewFigure] {
        let unchanged = String(localized: "unchanged")
        return [
            PreviewFigure(
                label: String(localized: "New installment"),
                value: effect.installment.map(WealthText.euro) ?? TransactionRow.placeholder,
                caption: effect.installmentChanged
                    ? effect.previousInstallment.map { String(localized: "before: \(WealthText.euro($0))") }
                    : unchanged
            ),
            PreviewFigure(
                label: String(localized: "New end"),
                value: effect.end.map { RecurringDayText.full($0) } ?? TransactionRow.placeholder,
                caption: effect.endChanged
                    ? effect.previousEnd.map { String(localized: "before: \(RecurringDayText.full($0))") }
                    : unchanged
            ),
            effect.interestSaved >= 0
                ? PreviewFigure(
                    label: String(localized: "Interest saved"),
                    value: WealthText.euro(effect.interestSaved),
                    tint: effect.interestSaved > 0 ? Ink.positive : Ink.text
                )
                : PreviewFigure(
                    label: String(localized: "Extra interest"),
                    value: WealthText.euro(-effect.interestSaved),
                    tint: Ink.negative
                ),
        ]
    }

    /// «Con «Riduci la rata» la fine resta 1 feb 2049, la rata scende a
    /// 721,12 € e gli interessi risparmiati sono 3.500,20 €.»
    private func alternative(_ effect: LoanEventEffect, mode: LoanEvent) -> String {
        guard case .prepayment(_, _, let other, _) = mode else { return "" }
        let name = WealthText.mode(other)
        let installment = effect.installment.map(WealthText.euro) ?? TransactionRow.placeholder
        let end = effect.end.map { RecurringDayText.full($0) } ?? TransactionRow.placeholder
        let saved = WealthText.euro(effect.interestSaved)
        switch other {
        case .lower:
            return String(localized: "With \u{00AB}\(name)\u{00BB} the end stays \(end), the installment goes down to \(installment) and the interest saved is \(saved).")
        case .shorten:
            return String(localized: "With \u{00AB}\(name)\u{00BB} the installment stays \(installment), the loan ends on \(end) and the interest saved is \(saved).")
        }
    }

    /// «La ricorrenza «Mutuo» resta a 771,88 € e finisce il 1 mar 2047.»: the
    /// linked template follows the schedule.
    private func templateNote(_ template: RecurringView, effect: LoanEventEffect) -> String {
        let name = RecurringTitle.of(template)
        let amount = effect.installment.map { WealthText.euro($0 + effect.fee) } ?? TransactionRow.placeholder
        let end = effect.end.map { RecurringDayText.full($0) } ?? TransactionRow.placeholder
        return effect.installmentChanged
            ? String(localized: "The recurring entry \u{00AB}\(name)\u{00BB} goes to \(amount) and ends on \(end).")
            : String(localized: "The recurring entry \u{00AB}\(name)\u{00BB} stays at \(amount) and ends on \(end).")
    }

    // MARK: - The expense

    private var expenseGroup: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                WealthCheckbox(label: String(localized: "Also record the expense in the ledger"), isOn: $draft.recordsExpense)
                Spacer(minLength: 8)
                if case .prepayment(_, let amount, _, let fee)? = event {
                    Text(String(localized: "\(WealthText.euro(amount + fee)) \u{00B7} amount and penalty"))
                        .font(Face.ui(12))
                        .foregroundStyle(Ink.text3)
                }
            }
            if draft.recordsExpense {
                FormGroup {
                    LoanTargetRows(store: store, target: $draft.expense, completion: completion, showsOwner: false)
                }
            }
            if let template, let current, let next {
                FormNote(templateNote(template, effect: LoanEventEffect.of(current: current, next: next, date: day)), indented: false)
            }
        }
        .padding(.top, 10)
        .overlay(alignment: .top) { Hairline() }
    }

    // MARK: - Footer

    /// What keeps «Aggiungi evento» off, at the footer's left.
    private var footerNote: some View {
        let words: (String, Bool)? = if case .failure(let error) = checked, !error.message.isEmpty {
            (error.message, !isBlank(error.field))
        } else {
            nil
        }
        return Text(words?.0 ?? "")
            .font(Face.ui(11.5))
            .foregroundStyle(words?.1 == true ? Ink.negative : Ink.text3)
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func isBlank(_ field: LoanDraftError.Field) -> Bool {
        let text: String = switch field {
        case .rate: draft.rate
        case .installment: draft.installment
        case .amount: draft.amount
        case .fee: draft.fee
        default: "x"
        }
        return text.trimmingCharacters(in: .whitespaces).isEmpty
    }

    private func submit() {
        guard !working, let event else { return }
        let expense = draft.kind == .prepayment && draft.recordsExpense ? draft.expense : nil
        $working.run {
            if await store.addLoanEvent(event, to: loan, expense: expense) { dismiss() }
        }
    }

    /// A repayment's expense goes where the installments go: the linked
    /// template's wallet, flow and category, else the new loan's defaults.
    private static func expenseTarget(_ loan: LoanView, store: AppStore) -> LoanLedgerTarget {
        let template = loan.recurringId.flatMap { id in store.recurringTemplates.first { $0.id == id } }
        guard let template else { return LoanTargetRows.defaults(store) }
        return LoanLedgerTarget(
            walletId: template.walletId ?? store.defaultWalletId,
            flowId: template.flowId,
            category: template.category ?? "",
            owner: nil
        )
    }
}
