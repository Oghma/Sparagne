import SwiftUI
import SparagneCore

/// «Nuovo prestito» and «Modifica prestito»: the terms as typed, a live
/// preview of the schedule they make, and for a new loan how its
/// installments reach the ledger. The draft reads every figure; the sheet
/// only shows what it made of them.
struct LoanSheet: View {
    let store: AppStore
    /// `nil` for a new loan.
    let loan: LoanView?

    @Environment(\.dismiss) private var dismiss
    @State private var draft: LoanDraft
    /// The schedule of the terms as typed, asked of the core whenever they
    /// change; `nil` while they make none.
    @State private var preview: LoanSchedule?
    @State private var working = false
    /// The list under the CATEGORY field (`CategoryFormField`).
    @State private var completion = CategoryCompletionModel()

    init(store: AppStore, loan: LoanView? = nil) {
        self.store = store
        self.loan = loan
        if let loan {
            _draft = State(initialValue: Self.editDraft(loan))
        } else {
            var draft = LoanDraft(firstDue: Self.firstOfNextMonth())
            draft.payment = .create
            draft.target = LoanTargetRows.defaults(store)
            _draft = State(initialValue: draft)
        }
    }

    private var isNew: Bool { loan == nil }
    private var terms: LoanTerms? { try? draft.terms().get() }

    var body: some View {
        FormSheet(
            isNew ? String(localized: "New loan") : String(localized: "Edit loan"),
            subtitle: String(localized: "The amortization schedule follows from the terms: the bank charges the installments itself, Sparagne keeps count of the debt."),
            width: 720
        ) {
            FormGroup {
                FormRow(String(localized: "Name")) {
                    FormTextField(label: String(localized: "Name"), text: $draft.name).frame(width: 300)
                }
                FormRow(String(localized: "Note")) {
                    FormTextField(label: String(localized: "Note"), text: $draft.note, prompt: String(localized: "optional"))
                        .frame(width: 300)
                }
                FormRow(String(localized: "Amount")) {
                    UnitField(label: String(localized: "Amount"), text: $draft.principal, unit: "\u{20AC}").frame(width: 160)
                }
                rateRow
                if draft.rateKind == .taeg {
                    FormNote(String(localized: "Estimate: the nominal rate is worked out without fees. If the bank states it, choose Nominal for the exact installment."))
                }
                durationRow
                FormRow(String(localized: "First installment")) {
                    HStack(spacing: 10) {
                        LoanDayField(label: String(localized: "First installment"), date: $draft.firstDue).frame(width: 200)
                        hint(String(localized: "then every month, on day \(Calendar.current.component(.day, from: draft.firstDue))"))
                    }
                }
                installmentRow
                FormRow(String(localized: "Upfront fees")) {
                    HStack(spacing: 10) {
                        UnitField(label: String(localized: "Upfront fees"), text: $draft.upfrontFees, unit: "\u{20AC}", prompt: "0,00")
                            .frame(width: 130)
                        hint(String(localized: "application, survey, substitute tax"))
                    }
                }
                FormRow(String(localized: "Fee per installment")) {
                    HStack(spacing: 10) {
                        UnitField(label: String(localized: "Fee per installment"), text: $draft.installmentFee, unit: "\u{20AC}", prompt: "0,00")
                            .frame(width: 130)
                        hint(String(localized: "collection fee, if the bank charges it"))
                    }
                }
            }
            if isNew { paymentGroup }
            previewGroup
        } footer: {
            footerNote
            FormCancelButton(shortcut: !completion.isOpen) { dismiss() }
                .disabled(working)
            FormPrimaryButton(
                title: isNew ? String(localized: "Create loan") : String(localized: "Save"),
                shortcut: !completion.isOpen
            ) {
                submit()
            }
            .disabled(!canSubmit || working)
        }
        .hangingCategoryList(completion)
        .task(id: PreviewKey(terms: terms, events: loan?.events ?? [])) {
            guard let terms else {
                preview = nil
                return
            }
            preview = await store.previewLoan(terms: terms, events: loan?.events ?? [])
        }
    }

    private struct PreviewKey: Equatable {
        let terms: LoanTerms?
        let events: [LoanEventView]
    }

    private func hint(_ text: String) -> some View {
        Text(text)
            .font(Face.ui(12))
            .foregroundStyle(Ink.text3)
            .lineLimit(1)
    }

    // MARK: - Rate, duration, installment

    /// [TAN | TAEG] and the rate; with the TAEG, the TAN the draft worked out
    /// beside it.
    private var rateRow: some View {
        FormRow(String(localized: "Rate")) {
            HStack(spacing: 10) {
                SegmentedStrip(
                    options: [LoanDraft.RateKind.tan, .taeg],
                    selection: $draft.rateKind,
                    label: { $0 == .tan ? Self.tanLabel : Self.taegLabel },
                    name: String(localized: "Rate"),
                    fixed: true
                )
                UnitField(label: draft.rateKind == .tan ? Self.tanLabel : Self.taegLabel, text: $draft.rate, unit: "%")
                    .frame(width: 100)
                if let terms, draft.rateKind == .taeg || draft.rate.trimmingCharacters(in: .whitespaces).isEmpty {
                    hint("\(Self.tanLabel) \(WealthText.rate(terms.annualRatePpm, decimals: 3))")
                }
            }
        }
    }

    private var durationRow: some View {
        FormRow(String(localized: "Duration")) {
            HStack(spacing: 10) {
                UnitField(label: String(localized: "Duration"), text: $draft.duration).frame(width: 64)
                SegmentedStrip(
                    options: [LoanDraft.DurationUnit.years, .months],
                    selection: $draft.durationUnit,
                    label: { $0 == .years ? String(localized: "years") : String(localized: "months") },
                    name: String(localized: "Duration"),
                    fixed: true
                )
                if let terms {
                    hint(String(localized: "\(Int(terms.months)) monthly installments"))
                }
            }
        }
    }

    /// [Calcolata | Come da contratto]: the installment the preview worked
    /// out, or the contract's as typed.
    private var installmentRow: some View {
        FormRow(String(localized: "Installment")) {
            HStack(spacing: 10) {
                SegmentedStrip(
                    options: [LoanDraft.InstallmentMode.computed, .contract],
                    selection: $draft.installmentMode,
                    label: { $0 == .computed ? String(localized: "Computed") : String(localized: "As in the contract") },
                    name: String(localized: "Installment"),
                    fixed: true
                )
                switch draft.installmentMode {
                case .computed:
                    if let first = preview?.rows.first {
                        Text(WealthText.euro(first.payment))
                            .font(Face.ui(13, .semibold))
                            .foregroundStyle(Ink.text)
                        if draft.rateKind == .taeg {
                            RowTag(text: String(localized: "estimate"), tint: Ink.accent, opacity: 0.14)
                        } else {
                            hint(String(localized: "worked out from the rate"))
                        }
                    }
                case .contract:
                    UnitField(label: String(localized: "Installment"), text: $draft.installment, unit: "\u{20AC}")
                        .frame(width: 130)
                }
            }
        }
    }

    // MARK: - Pagamento

    /// Crea ricorrenza, Collega a una esistente or Nessuna, and what each
    /// needs.
    private var paymentGroup: some View {
        let templates = store.linkableRecurring(for: nil)
        return VStack(alignment: .leading, spacing: 8) {
            FormGroupLabel(String(localized: "Payment"))
            HStack(spacing: 8) {
                ChoiceCard(
                    title: String(localized: "Create recurring entry"),
                    detail: monthlyDetail,
                    selected: draft.payment == .create
                ) { draft.payment = .create }
                ChoiceCard(
                    title: String(localized: "Link to an existing one"),
                    detail: String(localized: "the amount follows the schedule"),
                    selected: draft.payment == .link
                ) { draft.payment = .link }
                ChoiceCard(
                    title: String(localized: "loan.payment.none", defaultValue: "None"),
                    detail: String(localized: "you record the installments"),
                    selected: draft.payment == .none
                ) { draft.payment = .none }
            }
            switch draft.payment {
            case .create:
                FormGroup {
                    LoanTargetRows(store: store, target: $draft.target, completion: completion)
                }
            case .link:
                FormGroup {
                    FormRow(String(localized: "loan.recurring", defaultValue: "Recurring entry")) {
                        FormMenu(
                            label: String(localized: "loan.recurring", defaultValue: "Recurring entry"),
                            value: templates.first { $0.id == draft.linkedRecurringId }.map(RecurringTitle.of)
                                ?? String(localized: "Choose\u{2026}")
                        ) {
                            ForEach(templates, id: \.id) { template in
                                let title = "\(RecurringTitle.of(template)) \u{00B7} \(RecurringAmount.text(template))"
                                Button(title) { draft.linkedRecurringId = template.id }
                            }
                        }
                        .frame(width: 300)
                        .disabled(templates.isEmpty)
                    }
                    if templates.isEmpty {
                        FormNote(String(localized: "No expense template to link"))
                    } else {
                        FormNote(String(localized: "Its amount and end follow the schedule from now on, what was typed in it included."))
                    }
                }
            case .none:
                EmptyView()
            }
        }
        .padding(.top, 10)
        .overlay(alignment: .top) { Hairline() }
    }

    /// «una nuova, 948,42 € al mese»: the template's amount, fee included.
    private var monthlyDetail: String {
        guard let first = preview?.rows.first else { return String(localized: "a new one, monthly") }
        return String(localized: "a new one, \(WealthText.euro(first.payment + first.fee)) a month")
    }

    // MARK: - Anteprima

    @ViewBuilder
    private var previewGroup: some View {
        if let preview, let first = preview.rows.first, let last = preview.rows.last {
            VStack(alignment: .leading, spacing: 8) {
                PreviewBlock(figures: [
                    PreviewFigure(
                        label: String(localized: "Installment"),
                        value: WealthText.euro(first.payment),
                        caption: first.fee > 0 ? String(localized: "plus \(LedgerMoney.bare(first.fee)) of fees") : nil
                    ),
                    PreviewFigure(
                        label: String(localized: "Last installment"),
                        value: WealthText.euro(last.payment),
                        caption: RecurringDayText.full(last.dueDate)
                    ),
                    PreviewFigure(label: String(localized: "Total interest"), value: WealthText.euro(preview.totalInterest)),
                    PreviewFigure(
                        label: String(localized: "APR"),
                        value: preview.taegPpm.map { WealthText.rate($0) } ?? TransactionRow.placeholder,
                        caption: (terms?.upfrontFees ?? 0) > 0 || (terms?.installmentFee ?? 0) > 0
                            ? String(localized: "with the fees")
                            : nil
                    ),
                    PreviewFigure(
                        label: String(localized: "Total cost of credit"),
                        value: WealthText.euro(preview.totalCost),
                        caption: String(localized: "interest and fees")
                    ),
                ])
                ForEach(preview.warnings, id: \.self) { warning in
                    FormNote(WealthText.warning(warning), tone: .warning, indented: false)
                }
            }
        }
    }

    // MARK: - Footer

    /// At the footer's left: what is missing or wrong, else what the button
    /// does beside the loan.
    @ViewBuilder
    private var footerNote: some View {
        let words: (String, Bool)? = if case .failure(let error) = draft.terms() {
            error.message.isEmpty ? nil : (error.message, !isBlank(error.field))
        } else if draft.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            (String(localized: "Give the loan a name"), false)
        } else if case .failure(let error) = draft.paymentRequest() {
            error.message.isEmpty ? nil : (error.message, true)
        } else if isNew, draft.payment == .create {
            (String(localized: "also creates the recurring entry \u{00AB}\(draftName)\u{00BB}"), false)
        } else {
            nil
        }
        Text(words?.0 ?? "")
            .font(Face.ui(11.5))
            .foregroundStyle(words?.1 == true ? Ink.negative : Ink.text3)
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var draftName: String {
        draft.name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// An error on a field still empty is guidance, not a mistake.
    private func isBlank(_ field: LoanDraftError.Field) -> Bool {
        let text: String = switch field {
        case .name: draft.name
        case .principal: draft.principal
        case .rate: draft.rate
        case .duration: draft.duration
        case .installment: draft.installment
        case .upfrontFees: draft.upfrontFees
        case .installmentFee: draft.installmentFee
        case .firstDue, .payment, .date, .amount, .fee: "x"
        }
        return text.trimmingCharacters(in: .whitespaces).isEmpty
    }

    private var canSubmit: Bool {
        guard terms != nil, !draftName.isEmpty else { return false }
        if isNew, case .failure = draft.paymentRequest() { return false }
        return true
    }

    private func submit() {
        guard !working, let terms else { return }
        let name = draftName
        let note = draft.note.trimmingCharacters(in: .whitespacesAndNewlines)
        $working.run {
            let saved: Bool
            if let loan {
                saved = await store.updateLoan(loan, name: name, note: note.isEmpty ? nil : note, terms: terms)
            } else {
                guard case .success(let payment) = draft.paymentRequest() else { return }
                saved = await store.createLoan(name: name, note: note.isEmpty ? nil : note, terms: terms, payment: payment)
            }
            if saved { dismiss() }
        }
    }

    // MARK: - Seeds

    static var tanLabel: String { String(localized: "loan.tan", defaultValue: "Nominal") }
    static var taegLabel: String { String(localized: "loan.taeg", defaultValue: "APR") }

    /// The first day of next month: where a loan signed now usually starts.
    private static func firstOfNextMonth(_ now: Date = Date(), calendar: Calendar = .current) -> Date {
        let start = calendar.dateInterval(of: .month, for: now)?.start ?? now
        return calendar.date(byAdding: .month, value: 1, to: start) ?? now
    }

    /// A saved loan as the form types it, so that `terms()` reads it back
    /// as it is.
    ///
    /// Stand-in for `LoanDraft(loan:)`, which the drafts' own file brings.
    static func editDraft(_ loan: LoanView) -> LoanDraft {
        let terms = loan.terms
        var draft = LoanDraft(firstDue: CoreDate.localDay(terms.firstDue) ?? Date())
        draft.name = loan.name
        draft.note = loan.note ?? ""
        draft.principal = LedgerMoney.bare(terms.principal)
        let ppm = terms.annualRatePpm
        let decimals = ppm % 100 == 0 ? 2 : ppm % 10 == 0 ? 3 : 4
        draft.rate = LedgerMoney.number(Decimal(ppm) / 10_000 as NSDecimalNumber, decimals: decimals)
        draft.rateKind = .tan
        if terms.months % 12 == 0 {
            draft.duration = String(terms.months / 12)
            draft.durationUnit = .years
        } else {
            draft.duration = String(terms.months)
            draft.durationUnit = .months
        }
        if case .contract(let amount) = terms.installment {
            draft.installmentMode = .contract
            draft.installment = LedgerMoney.bare(amount)
        } else {
            draft.installmentMode = .computed
        }
        draft.upfrontFees = terms.upfrontFees == 0 ? "" : LedgerMoney.bare(terms.upfrontFees)
        draft.installmentFee = terms.installmentFee == 0 ? "" : LedgerMoney.bare(terms.installmentFee)
        draft.payment = .none
        draft.linkedRecurringId = loan.recurringId
        return draft
    }
}
