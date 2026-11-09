import SwiftUI
import SparagneCore

/// The Patrimonio tab: the wallets' money against what the loans and
/// mortgages still owe, the loans in a table, and the selected one in an
/// inspector. Everything is as of today: the tab ignores the month and the
/// person on screen.
struct WealthTab: View {
    let store: AppStore

    /// The user's pick in the table. `nil` until there is one, and then the
    /// inspector shows the first loan (`shown`), so the layout never jumps.
    @State private var picked: Uuid?
    /// The day the figures count from, taken again at midnight.
    @State private var today = CoreDate.day(Date())
    /// The schedules of the archived loans, which the core's overview leaves
    /// out: their rows still say how they ended.
    @State private var archivedSchedules: [Uuid: LoanSchedule] = [:]

    static let inspectorWidth: CGFloat = 340

    var body: some View {
        let rows = LoanRow.rows(
            loans: store.loans,
            overview: store.loanOverview,
            schedules: archivedSchedules,
            templates: store.recurringTemplates,
            today: today,
            occurrences: CoreSchedule.occurrences
        )
        HStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    WealthCards(
                        figures: WealthFigures.build(
                            wallets: store.snapshot?.wallets ?? [],
                            loans: store.loans,
                            overview: store.loanOverview
                        )
                    )
                    LoanTable(store: store, rows: rows, today: today, selection: shown(rows)) { picked = $0 }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .topLeading)
            }
            Rectangle().fill(Ink.line).frame(width: 1)
            inspector(shown(rows))
                .frame(width: Self.inspectorWidth)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task(id: store.loans.filter(\.archived)) {
            var loaded: [Uuid: LoanSchedule] = [:]
            for loan in store.loans where loan.archived {
                loaded[loan.id] = await store.loanSchedule(loan.id)
            }
            archivedSchedules = loaded
        }
        .task {
            for await _ in NotificationCenter.default.messages(of: Calendar.self, for: .calendarDayChanged) {
                today = CoreDate.day(Date())
            }
        }
        .onAppear { today = CoreDate.day(Date()) }
        // Another vault: the pick named a loan of the vault that is gone.
        .onChange(of: store.currentVault?.id) { _, _ in picked = nil }
    }

    /// The pick while it still exists, else the first loan of the table.
    private func shown(_ rows: [LoanRow]) -> Uuid? {
        if let picked, rows.contains(where: { $0.id == picked }) { return picked }
        return rows.first?.id
    }

    @ViewBuilder
    private func inspector(_ id: Uuid?) -> some View {
        if let id, let loan = store.loans.first(where: { $0.id == id }) {
            LoanInspector(
                store: store,
                loan: loan,
                overview: store.loanOverview.first { $0.loanId == id },
                today: today
            )
            .id(id)
        } else {
            VStack(alignment: .leading, spacing: 8) {
                Text(String(localized: "No loans"))
                    .font(Face.ui(16, .semibold))
                    .foregroundStyle(Ink.text)
                Text(String(localized: "Add a loan or a mortgage to follow its debt: the bank charges the installments, Sparagne keeps count of what is left."))
                    .font(Face.row)
                    .foregroundStyle(Ink.text3)
                    .fixedSize(horizontal: false, vertical: true)
                if store.canWrite {
                    Button(String(localized: "New loan\u{2026}")) {
                        NotificationCenter.default.post(name: .newLoan, object: nil)
                    }
                    .buttonStyle(.chrome())
                    .padding(.top, 4)
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .background(Ink.bg)
        }
    }
}

// MARK: - The cards

/// Liquidità, Debito residuo and Patrimonio netto, three across.
struct WealthCards: View {
    let figures: WealthFigures

    var body: some View {
        HStack(spacing: 8) {
            WealthCard(
                title: String(localized: "Liquidity"),
                value: LedgerMoney.bare(figures.liquidity),
                caption: figures.wallets.isEmpty ? nil : WealthText.wallets(figures.wallets)
            )
            WealthCard(
                title: String(localized: "Remaining debt"),
                value: LedgerMoney.bare(figures.debt),
                caption: String(localized: "\(figures.activeLoans) active loans")
            )
            WealthCard(
                title: String(localized: "Net worth"),
                value: LedgerMoney.bare(figures.netWorth),
                tint: figures.netWorth < 0 ? Ink.negative : Ink.text,
                caption: String(localized: "liquidity \u{2212} debt")
            )
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(String(localized: "Wealth as of today"))
    }
}

/// One of the three figures: its title, the number, a line under it.
private struct WealthCard: View {
    let title: String
    let value: String
    var tint: Color = Ink.text
    var caption: String?

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(Face.footnote)
                    .foregroundStyle(Ink.text3)
                Text(value)
                    .font(Face.display)
                    .foregroundStyle(tint)
                Text(caption ?? " ")
                    .font(Face.row)
                    .foregroundStyle(Ink.text2)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(AccessibilityText.card(title: title, value: value, caption: caption))
        .help(ifAny: caption)
    }
}
