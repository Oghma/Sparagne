import SwiftUI
import SparagneCore

/// The Patrimonio's inspector: one loan's terms, where it stands today, its
/// residual debt over time, its events and how its installments reach the
/// ledger. What changes the loan opens a sheet of the window; what links or
/// unlinks it acts at once.
struct LoanInspector: View {
    let store: AppStore
    let loan: LoanView
    /// The core's, for a loan not archived; an archived one's is worked out
    /// from its schedule.
    let overview: LoanOverview?
    let today: NaiveDate

    /// The loan's whole schedule, asked of the core whenever the loan
    /// changes. `nil` until the first answer.
    @State private var schedule: LoanSchedule?
    /// A command in flight: the buttons wait for it.
    @State private var working = false

    /// An archived loan is restored before it changes.
    private var editable: Bool { store.canWrite && !loan.archived }

    /// The template that records the installments, archived or not.
    private var linkedTemplate: RecurringView? {
        loan.recurringId.flatMap { id in store.recurringTemplates.first { $0.id == id } }
    }

    private var figures: LoanOverview? {
        overview ?? schedule.map { LoanFacts.overview(of: loan, schedule: $0, today: today) }
    }

    var body: some View {
        InspectorShell(
            title: loan.name,
            subtitle: WealthText.summary(loan),
            footerDisabled: working
        ) {
            if store.canWrite { moreMenu }
        } content: {
            termsGroup
            todayGroup
            chartGroup
            eventsGroup
            paymentGroup
            if let note = loan.note, !note.isEmpty {
                FormGroup(String(localized: "Note")) {
                    Text(note)
                        .font(Face.row)
                        .foregroundStyle(Ink.text2)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
            }
        } footer: {
            Button {
                LoanSheetRequest.schedule(loan.id).post()
            } label: {
                Text(String(localized: "Amortization schedule\u{2026}")).frame(maxWidth: .infinity)
            }
            .buttonStyle(.chrome())
            if loan.archived, store.canWrite {
                Button(String(localized: "Restore")) {
                    $working.run { await store.setLoanArchived(loan, false) }
                }
                .buttonStyle(.chrome(.ghost, small: true))
            }
        }
        .task(id: loan) {
            schedule = await store.loanSchedule(loan.id)
        }
    }

    // MARK: - Heading

    /// Modifica…, Aggiungi evento… and archive or restore.
    private var moreMenu: some View {
        Menu {
            if !loan.archived {
                Button(String(localized: "Edit\u{2026}")) { LoanSheetRequest.edit(loan.id).post() }
                Button(String(localized: "Add event\u{2026}")) { LoanSheetRequest.addEvent(loan.id).post() }
                Divider()
                Button(String(localized: "Archive"), role: .destructive) {
                    $working.run { await store.setLoanArchived(loan, true) }
                }
            } else {
                Button(String(localized: "Restore")) {
                    $working.run { await store.setLoanArchived(loan, false) }
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

    // MARK: - Termini

    private var termsGroup: some View {
        let terms = loan.terms
        let taeg = figures?.taegPpm ?? schedule?.taegPpm
        let last = figures?.endDate ?? schedule?.rows.last?.dueDate
        return FormGroup(String(localized: "Terms")) {
            VStack(spacing: 0) {
                WealthFact(String(localized: "Amount"), LedgerMoney.bare(terms.principal))
                WealthFact(String(localized: "Current rate"), WealthText.rate(LoanFacts.currentRate(loan, today: today)))
                WealthFact(String(localized: "APR"), taeg.map { WealthText.rate($0) } ?? TransactionRow.placeholder)
                WealthFact(String(localized: "Duration"), WealthText.durationWithCount(months: terms.months))
                WealthFact(String(localized: "First installment"), RecurringDayText.full(terms.firstDue))
                WealthFact(String(localized: "Last installment"), last.map { RecurringDayText.full($0) } ?? TransactionRow.placeholder)
                if terms.upfrontFees > 0 {
                    WealthFact(String(localized: "Upfront fees"), LedgerMoney.bare(terms.upfrontFees))
                }
                if terms.installmentFee > 0 {
                    WealthFact(String(localized: "Fee per installment"), LedgerMoney.bare(terms.installmentFee))
                }
            }
        }
    }

    // MARK: - A oggi

    @ViewBuilder
    private var todayGroup: some View {
        if let figures {
            FormGroup(String(localized: "As of today")) {
                VStack(spacing: 0) {
                    WealthFact(String(localized: "Capital repaid"), LedgerMoney.bare(figures.capitalRepaid))
                    WealthFact(String(localized: "Interest paid"), LedgerMoney.bare(figures.interestPaid))
                    WealthFact(String(localized: "Interest to pay"), LedgerMoney.bare(figures.interestLeft))
                    WealthFact(String(localized: "Total cost"), LedgerMoney.bare(figures.totalCost))
                }
                if let schedule {
                    note(WealthText.costNote(
                        totalCost: schedule.totalCost,
                        totalInterest: schedule.totalInterest,
                        upfrontFees: loan.terms.upfrontFees
                    ))
                }
                ForEach(
                    LoanFacts.issues(figures, firstDue: loan.terms.firstDue, templateStart: linkedTemplate?.schedule.startDate),
                    id: \.self
                ) { issue in
                    FormNote(WealthText.issue(issue), tone: .warning, indented: false)
                }
            }
        }
    }

    // MARK: - Il grafico

    @ViewBuilder
    private var chartGroup: some View {
        if let schedule, !schedule.rows.isEmpty {
            FormGroup(String(localized: "Remaining debt over time")) {
                LoanChartView(chart: LoanChart.build(loan: loan, schedule: schedule, today: today), name: loan.name)
            }
        }
    }

    // MARK: - Eventi

    private var eventsGroup: some View {
        let previous = LoanFacts.previousRates(loan)
        return FormGroup(String(localized: "Events")) {
            if loan.events.isEmpty {
                note(String(localized: "No rate changes or early repayments."))
            }
            VStack(spacing: 0) {
                ForEach(loan.events, id: \.id) { view in
                    eventRow(view, previousRate: previous[view.id])
                }
            }
            if editable {
                Button {
                    LoanSheetRequest.addEvent(loan.id).post()
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "plus")
                            .font(.system(size: 10, weight: .semibold))
                            .accessibilityHidden(true)
                        Text(String(localized: "Add event\u{2026}"))
                    }
                }
                .buttonStyle(.chrome(.ghost, small: true))
                .padding(.leading, -8)
                .disabled(working)
            }
        }
    }

    private func eventRow(_ view: LoanEventView, previousRate: UInt32?) -> some View {
        let kind = WealthText.kind(view.event)
        return HStack(spacing: 6) {
            HStack(spacing: 0) {
                Text(kind).foregroundStyle(Ink.text2)
                Text(verbatim: " \u{00B7} ").foregroundStyle(Ink.text3)
                Text(RecurringDayText.full(view.event.effectiveDate)).foregroundStyle(Ink.text)
                Text(verbatim: " \u{00B7} ").foregroundStyle(Ink.text3)
                Text(WealthText.detail(view.event, previousRate: previousRate))
                    .foregroundStyle(Ink.text)
                    .layoutPriority(-1)
            }
            .lineLimit(1)
            .help(WealthText.event(view.event, previousRate: previousRate))
            Spacer(minLength: 4)
            if editable {
                SetupIconButton(
                    symbol: "xmark",
                    help: String(localized: "Remove \(kind.lowercased())"),
                    label: String(localized: "Remove \(kind.lowercased())"),
                    symbolSize: 9,
                    symbolWeight: .semibold
                ) {
                    $working.run { await store.removeLoanEvent(view.id, from: loan) }
                }
                .disabled(working)
            }
        }
        .font(Face.row)
        .frame(minHeight: 24)
        .overlay(alignment: .bottom) { DashedRule() }
        .accessibilityElement(children: .combine)
    }

    // MARK: - Pagamento

    private var paymentGroup: some View {
        let template = linkedTemplate
        let names = store.names
        return FormGroup(String(localized: "Payment")) {
            VStack(spacing: 0) {
                WealthFact(
                    String(localized: "loan.recurring", defaultValue: "Recurring entry"),
                    template.map { template in
                        [
                            RecurringTitle.of(template),
                            DueRecurringText.envelope(template, names: names),
                            DueRecurringText.wallet(template, names: names),
                        ].joined(separator: " \u{00B7} ")
                    } ?? String(localized: "loan.recurring.none", defaultValue: "none")
                )
                if let template, let figures, let registered = figures.registeredCount {
                    let due = LoanFacts.recordable(figures, firstDue: loan.terms.firstDue, templateStart: template.schedule.startDate)
                    WealthFact(
                        String(localized: "Recorded in the ledger"),
                        String(localized: "\(Int(registered)) of \(Int(due))"),
                        tint: registered < due ? Ink.warning : Ink.text
                    )
                }
            }
            if let template, template.archived {
                FormNote(
                    String(localized: "recurring entry archived \u{2014} unlink it or link another"),
                    tone: .warning,
                    indented: false
                )
            } else if template == nil, loan.recurringId == nil {
                note(String(localized: "Without a recurring entry the installments are yours to record."))
            }
            if editable {
                HStack(spacing: 6) {
                    if loan.recurringId != nil {
                        Button(String(localized: "Unlink")) {
                            $working.run { await store.linkLoan(loan, to: nil) }
                        }
                        .buttonStyle(.chrome(small: true))
                    }
                    linkMenu
                    if loan.recurringId == nil {
                        Button(String(localized: "Create recurring entry\u{2026}")) {
                            LoanSheetRequest.createRecurring(loan.id).post()
                        }
                        .buttonStyle(.chrome(small: true))
                    }
                }
                .padding(.top, 2)
                .disabled(working)
            }
        }
    }

    /// Collega…: the expense templates no other loan links.
    private var linkMenu: some View {
        let candidates = store.linkableRecurring(for: loan).filter { $0.id != loan.recurringId }
        let names = store.names
        return Menu {
            // Linking rewrites the template: said before the click.
            Section(String(localized: "Its amount and end follow the schedule from now on, what was typed in it included.")) {
                ForEach(candidates, id: \.id) { template in
                    Button {
                        $working.run { await store.linkLoan(loan, to: template.id) }
                    } label: {
                        Text(RecurringTitle.of(template))
                        Text(verbatim: linkDetail(template, names: names))
                    }
                }
            }
        } label: {
            Text(String(localized: "Link\u{2026}"))
        }
        .menuStyle(.button)
        .buttonStyle(.chrome(small: true))
        .menuIndicator(.hidden)
        .fixedSize()
        .disabled(candidates.isEmpty)
        .help(candidates.isEmpty ? String(localized: "No expense template to link") : String(localized: "Link\u{2026}"))
    }

    /// `780,00 → 771,88 · Casa · fino al 1 feb 2049`: what linking does to
    /// the template's amount and end (`LoanLink.alignment`), its flow
    /// between them.
    private func linkDetail(_ template: RecurringView, names: NameBook) -> String {
        let envelope = DueRecurringText.envelope(template, names: names)
        guard let schedule else { return "\(RecurringAmount.text(template)) \u{00B7} \(envelope)" }
        let patch = LoanLink.alignment(of: template, to: schedule, today: today)
        let amount = patch.amount.map { "\(RecurringAmount.text(template)) \u{2192} \(LedgerMoney.bare($0))" }
            ?? RecurringAmount.text(template)
        let end = (patch.schedule?.endDate ?? template.schedule.endDate)
            .map { String(localized: "until \(RecurringDayText.full($0))") }
        return ([amount, envelope] + (end.map { [$0] } ?? [])).joined(separator: " \u{00B7} ")
    }

    private func note(_ text: String) -> some View {
        Text(text)
            .font(Face.ui(11.5))
            .foregroundStyle(Ink.text3)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}
