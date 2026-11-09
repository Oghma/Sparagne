import SwiftUI
import SparagneCore

/// Column geometry of «Prestiti e mutui», as the canvas has it: Nome takes
/// what is left. Beside the inspector the narrowest window leaves Nome
/// about 160 points.
private enum LoanColumn {
    static let nameMinimum: CGFloat = 150
    static let installment: CGFloat = 96
    static let rate: CGFloat = 64
    static let progress: CGFloat = 150
    static let residual: CGFloat = 120
    static let end: CGFloat = 84
    static let warning: CGFloat = 28
    static let rowHeight: CGFloat = 32
    static let meter: CGFloat = 110
}

/// «Prestiti e mutui»: every loan as a row, the running ones first and the
/// archived ones under them, and «Nuovo prestito…» last. A click shows the
/// loan in the inspector.
struct LoanTable: View {
    let store: AppStore
    let rows: [LoanRow]
    let today: NaiveDate
    /// What the inspector shows, as the tab resolved it.
    let selection: Uuid?
    let select: (Uuid) -> Void

    @State private var hovered: Uuid?

    var body: some View {
        SetupCard(
            title: String(localized: "Loans and mortgages"),
            hint: String(localized: "figures as of today, \(RecurringDayText.full(today)) \u{00B7} the tab ignores the month and the person")
        ) {
            SetupTableBody {
                header
                ForEach(rows) { row in
                    loanRow(row)
                }
                // A viewer reads the loans; nothing to add.
                if store.canWrite { newRow }
            }
        }
    }

    private var header: some View {
        SetupHeaderRow(accessibilityHeader: true) {
            Text(String(localized: "Name")).tableCell(minWidth: LoanColumn.nameMinimum)
            Text(String(localized: "Installment")).tableCell(width: LoanColumn.installment, alignment: .trailing)
            Text(String(localized: "loan.column.rate", defaultValue: "Rate"))
                .tableCell(width: LoanColumn.rate, alignment: .trailing)
            Text(String(localized: "Progress")).tableCell(width: LoanColumn.progress)
            Text(String(localized: "Remaining debt")).tableCell(width: LoanColumn.residual, alignment: .trailing)
            Text(String(localized: "End")).tableCell(width: LoanColumn.end)
            Text(verbatim: "").tableCell(width: LoanColumn.warning)
            Text(verbatim: "").tableCell(width: SetupColumn.action)
        }
    }

    // MARK: - A loan

    private func loanRow(_ row: LoanRow) -> some View {
        let isSelected = selection == row.id
        // Archived reads as background, as in the Setup tables.
        let ink = row.archived ? Ink.text3 : Ink.text
        let side = row.archived ? Ink.text3 : Ink.text2
        return SetupRow(
            highlighted: isSelected || hovered == row.id,
            editing: isSelected,
            height: LoanColumn.rowHeight,
            barInset: 4
        ) {
            nameCell(row, ink: ink).tableCell(minWidth: LoanColumn.nameMinimum, hitTestable: true)
            Text(row.installment.map(LedgerMoney.bare) ?? TransactionRow.placeholder)
                .foregroundStyle(ink)
                .tableCell(width: LoanColumn.installment, alignment: .trailing, hitTestable: true)
            Text(WealthText.rate(row.rate))
                .foregroundStyle(side)
                .tableCell(width: LoanColumn.rate, alignment: .trailing, hitTestable: true)
            progressCell(row).tableCell(width: LoanColumn.progress, hitTestable: true)
            Text(row.residual.map(LedgerMoney.bare) ?? TransactionRow.placeholder)
                .foregroundStyle(row.residual == 0 ? Ink.text3 : ink)
                .tableCell(width: LoanColumn.residual, alignment: .trailing, hitTestable: true)
            Text(row.end.map { WealthText.monthYear($0) } ?? TransactionRow.placeholder)
                .foregroundStyle(side)
                .tableCell(width: LoanColumn.end, hitTestable: true)
            warningCell(row).tableCell(width: LoanColumn.warning, alignment: .center)
            SetupArchiveCell(name: row.name, archived: row.archived, shown: hovered == row.id && store.canWrite) {
                Task { await setArchived(row) }
            }
        }
        .font(Face.row)
        .onTapGesture { select(row.id) }
        .hoverId($hovered, row.id)
        .contextMenu { menu(row) }
        // One stop per loan, its figures in one sentence; the icons'
        // actions come as the row's own.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.name)
        .accessibilityValue(spoken(row))
        .accessibilityAddTraits(isSelected ? [.isButton, .isSelected] : .isButton)
        .accessibilityAction { select(row.id) }
        .accessibilityActions {
            if store.canWrite {
                if !row.archived {
                    Button(String(localized: "Edit\u{2026}")) { LoanSheetRequest.edit(row.id).post() }
                }
                Button(row.archived ? String(localized: "Restore") : String(localized: "Archive")) {
                    Task { await setArchived(row) }
                }
            }
        }
    }

    /// The name, then «variabile», «estinto» and «archiviato».
    private func nameCell(_ row: LoanRow, ink: Color) -> some View {
        HStack(spacing: 6) {
            Text(row.name).foregroundStyle(ink).lineLimit(1)
            ForEach(tags(row), id: \.self) { SetupTag(text: $0) }
        }
    }

    private func tags(_ row: LoanRow) -> [String] {
        var tags: [String] = []
        if row.variable { tags.append(String(localized: "variable")) }
        if row.paidOff { tags.append(String(localized: "paid off")) }
        if row.archived { tags.append(String(localized: "loan.archived", defaultValue: "archived")) }
        return tags
    }

    /// `33/300` over its meter.
    @ViewBuilder
    private func progressCell(_ row: LoanRow) -> some View {
        if let paid = row.paid, let total = row.total {
            VStack(alignment: .leading, spacing: 3) {
                Text(verbatim: "\(paid)/\(total)")
                    .font(Face.ui(11.5))
                    .foregroundStyle(row.archived ? Ink.text3 : Ink.text2)
                MeterBar(
                    fraction: row.progress ?? 0,
                    tint: row.archived ? Ink.text3.opacity(0.5) : Ink.chartIncome,
                    track: Ink.track,
                    height: 3,
                    rounded: true
                )
                .frame(width: LoanColumn.meter)
            }
        } else {
            Text(TransactionRow.placeholder).foregroundStyle(Ink.text3)
        }
    }

    /// The triangle, its sentences on hover, when the schedule or the ledger
    /// has something to say. A text, not nothing, otherwise: a frame around
    /// nothing takes no room.
    @ViewBuilder
    private func warningCell(_ row: LoanRow) -> some View {
        if row.issues.isEmpty {
            Text(verbatim: "")
        } else {
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(Ink.warning)
                .help(row.issues.map(WealthText.issue).joined(separator: "\n"))
        }
    }

    @ViewBuilder
    private func menu(_ row: LoanRow) -> some View {
        if store.canWrite, !row.archived {
            Button(String(localized: "Edit\u{2026}")) { LoanSheetRequest.edit(row.id).post() }
            Button(String(localized: "Add event\u{2026}")) { LoanSheetRequest.addEvent(row.id).post() }
        }
        Button(String(localized: "Amortization schedule\u{2026}")) { LoanSheetRequest.schedule(row.id).post() }
        if store.canWrite {
            Divider()
            if row.archived {
                Button(String(localized: "Restore")) { Task { await setArchived(row) } }
            } else {
                Button(String(localized: "Archive"), role: .destructive) { Task { await setArchived(row) } }
            }
        }
    }

    private func setArchived(_ row: LoanRow) async {
        guard let loan = store.loans.first(where: { $0.id == row.id }) else { return }
        await store.setLoanArchived(loan, !loan.archived)
    }

    /// "variabile, Rata 771,88, TAN 2,85%, 33 di 300, Debito residuo
    /// 152.490,96, Fine feb 2049", then the warnings.
    private func spoken(_ row: LoanRow) -> String {
        var parts = tags(row)
        if let installment = row.installment {
            parts.append(AccessibilityText.figure(String(localized: "Installment"), amount: LedgerMoney.amount(installment)))
        }
        parts.append(AccessibilityText.figure(String(localized: "loan.column.rate", defaultValue: "Rate"), amount: WealthText.rate(row.rate)))
        if let paid = row.paid, let total = row.total {
            parts.append(AccessibilityText.figure(
                String(localized: "Progress"),
                amount: String(localized: "\(Int(paid)) of \(Int(total))")
            ))
        }
        if let residual = row.residual {
            parts.append(AccessibilityText.figure(String(localized: "Remaining debt"), amount: LedgerMoney.amount(residual)))
        }
        if let end = row.end {
            parts.append(AccessibilityText.figure(String(localized: "End"), amount: WealthText.monthYear(end)))
        }
        return (parts + row.issues.map(WealthText.issue)).joined(separator: ", ")
    }

    // MARK: - The last row

    /// «Nuovo prestito…»: the sheet, as the top bar's button opens it.
    private var newRow: some View {
        HStack(spacing: 6) {
            Image(systemName: "plus")
                .font(.system(size: 10, weight: .bold))
                .accessibilityHidden(true)
            Text(String(localized: "New loan\u{2026}"))
        }
        .font(Face.row)
        .foregroundStyle(hovered == Self.newRowId ? Ink.text2 : Ink.text3)
        .padding(.horizontal, Metrics.cellPad)
        .frame(maxWidth: .infinity, minHeight: 28, alignment: .leading)
        .background(hovered == Self.newRowId ? Ink.raised : .clear, in: RoundedRectangle(cornerRadius: 4))
        .contentShape(Rectangle())
        .onTapGesture { NotificationCenter.default.post(name: .newLoan, object: nil) }
        .hoverId($hovered, Self.newRowId)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(String(localized: "New loan\u{2026}"))
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { NotificationCenter.default.post(name: .newLoan, object: nil) }
    }

    /// The hover key of «Nuovo prestito…», which no loan's id can be.
    private static let newRowId: Uuid = "new"
}
