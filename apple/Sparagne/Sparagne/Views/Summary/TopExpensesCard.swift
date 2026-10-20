import SparagneCore
import SwiftUI

/// The year's heaviest expenses, largest first; a line is a button that
/// reveals the row in the Mastro (`AppStore.reveal`), on its month, picked
/// and scrolled into view.
struct TopExpensesCard: View {
    let expenses: [TopExpense]
    let store: AppStore

    /// The line under the pointer, lifted as the grid lifts its rows.
    @State private var hovered: Uuid?

    private static let dateWidth: CGFloat = 48
    private static let categoryWidth: CGFloat = 104
    private static let personWidth: CGFloat = 64
    private static let amountWidth: CGFloat = 84

    /// Who spent it, only when there is somebody else to tell apart: not
    /// with one person, nor under the PERSONA filter.
    private var showsPerson: Bool {
        (store.year?.people.count ?? 0) > 1 && store.year?.person == nil
    }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                Text(String(localized: "Heaviest expenses \u{00B7} year"))
                    .font(Face.label)
                    .foregroundStyle(Ink.text2)
                    .accessibilityAddTraits(.isHeader)
                if expenses.isEmpty {
                    Text(String(localized: "No expenses this year"))
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                } else {
                    VStack(spacing: 0) {
                        ForEach(expenses, id: \.transactionId) { line($0) }
                    }
                }
            }
            // As tall as the categories beside it.
            .frame(maxHeight: .infinity, alignment: .top)
        }
    }

    private func line(_ expense: TopExpense) -> some View {
        let date = CoreDate.date(expense.occurredAt)
        let category = SummaryText.categoryName(expense.category, isSystem: expense.categoryIsSystem)
        let note = Self.note(expense)
        return Button {
            store.reveal(transactionId: expense.transactionId, occurredAt: expense.occurredAt)
        } label: {
            HStack(spacing: 8) {
                Text(date.map { LedgerDate.day($0) } ?? "")
                    .foregroundStyle(Ink.text2)
                    .frame(width: Self.dateWidth, alignment: .leading)
                Text(note ?? category)
                    .foregroundStyle(Ink.text)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                // A row without a note is titled by its category, which is
                // then not repeated beside it.
                Text(note == nil ? "" : category)
                    .foregroundStyle(Ink.text2)
                    .truncationMode(.tail)
                    .frame(width: Self.categoryWidth, alignment: .leading)
                if showsPerson {
                    Text(expense.person)
                        .foregroundStyle(Ink.text3)
                        .truncationMode(.tail)
                        .frame(width: Self.personWidth, alignment: .leading)
                }
                Text(LedgerMoney.bare(expense.amount))
                    .foregroundStyle(Ink.text)
                    .frame(width: Self.amountWidth, alignment: .trailing)
            }
            .font(Face.row)
            .lineLimit(1)
            .padding(.horizontal, 6)
            .frame(height: Metrics.rowHeight)
            .background {
                if hovered == expense.transactionId {
                    RoundedRectangle(cornerRadius: 4).fill(Ink.raised)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .hoverId($hovered, expense.transactionId)
        .help(String(localized: "Show in the ledger"))
        // One button per line, read in the order the eye scans it:
        // "12 mar, Dentista, Salute, €480.00".
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(spoken(expense, date: date, note: note, category: category))
        .accessibilityHint(String(localized: "Show in the ledger"))
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { store.reveal(transactionId: expense.transactionId, occurredAt: expense.occurredAt) }
    }

    /// The description, `nil` without one: the line then names the row by
    /// its category, as the Riparto names an income without a note.
    private static func note(_ expense: TopExpense) -> String? {
        guard let note = expense.note?.trimmingCharacters(in: .whitespacesAndNewlines), !note.isEmpty else {
            return nil
        }
        return note
    }

    private func spoken(_ expense: TopExpense, date: Date?, note: String?, category: String) -> String {
        var parts: [String] = []
        if let date { parts.append(LedgerDate.day(date)) }
        if let note { parts.append(note) }
        parts.append(category)
        if showsPerson { parts.append(expense.person) }
        parts.append(LedgerMoney.amount(expense.amount))
        return parts.joined(separator: ", ")
    }
}
