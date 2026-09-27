import SwiftUI
import SparagneCore

/// The recurring periods waiting for a decision (`DISTILLATO_V1.md` §2.3):
/// every due template with its dates, oldest first, and Execute or Skip for
/// each. A template never writes a transaction by itself; this sheet is where
/// the user does it, opened from the ledger's banner or the palette.
///
/// Execute All sends every period listed as one batch
/// (`AppStore.executeAllDueRecurring`), so a backlog lands whole or not at
/// all. System styling, like the Recurring panel it complements: a modal
/// sheet is a dialog, not part of the spreadsheet surface.
struct DueRecurringSheet: View {
    let store: AppStore
    @Environment(\.dismiss) private var dismiss
    /// A command in flight. The buttons wait for it: a double click would
    /// execute the same period twice, and the core would refuse the second.
    @State private var working = false

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(String(localized: "Due Recurring Entries")).font(.headline)
                Spacer()
            }
            .padding()

            Divider()

            if store.pendingRecurringItems.isEmpty {
                ContentUnavailableView(String(localized: "Nothing is due"), systemImage: "checkmark.circle")
                    .frame(maxHeight: .infinity)
            } else {
                List {
                    ForEach(store.pendingRecurringItems, id: \.template.id) { item in
                        Section {
                            ForEach(item.due.sorted(), id: \.self) { date in
                                periodRow(item.template, date: date)
                            }
                        } header: {
                            DueTemplateHeader(template: item.template, store: store)
                        }
                    }
                }
            }

            Divider()

            HStack {
                Button(String(localized: "Execute All")) {
                    run { await store.executeAllDueRecurring() }
                }
                .disabled(working || store.isReadOnly || store.pendingRecurringItems.isEmpty)
                Spacer()
                Button(String(localized: "Done")) { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
            .padding()
        }
        .frame(width: 480, height: 440)
    }

    private func periodRow(_ template: RecurringView, date: NaiveDate) -> some View {
        HStack {
            Text(CoreDate.localDay(date).map { DateFormatting.relativeDay($0) } ?? date)
                .monospacedDigit()
            Spacer()
            Button(String(localized: "Skip")) {
                run { await store.skipRecurring(template.id, periodDate: date) }
            }
            Button(String(localized: "Execute")) {
                run { await store.executeRecurring(template.id, periodDate: date) }
            }
        }
        .disabled(working || store.isReadOnly)
    }

    private func run(_ work: @escaping @MainActor () async -> Void) {
        working = true
        Task {
            await work()
            working = false
        }
    }
}

/// What a template writes each time: the amount and its kind, what it is
/// for, and the wallet and envelope it lands on.
private struct DueTemplateHeader: View {
    let template: RecurringView
    let store: AppStore

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                Text(MoneyFormatter.format(minorUnits: template.amount, currencyCode: store.currencyCode))
                    .foregroundStyle(template.kind == .expense ? Ink.negative : Ink.positive)
                Text(DueRecurringText.kind(template.kind))
                    .foregroundStyle(.secondary)
                Text(DueRecurringText.subject(template))
            }
            Text(DueRecurringText.places(template, names: NameBook(snapshot: store.snapshot)))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .textCase(nil)
    }
}

/// The words of a template's header, shared with the Recurring panel's rows.
enum DueRecurringText {
    static func kind(_ kind: TransactionKind) -> String {
        switch kind {
        case .expense: String(localized: "expense")
        case .income: String(localized: "income")
        case .refund: String(localized: "refund")
        case .transferWallet, .transferFlow: String(localized: "transfer")
        }
    }

    /// The note, then the category, as the Recurring panel shows them.
    static func subject(_ template: RecurringView) -> String {
        var parts: [String] = []
        if let note = template.note, !note.isEmpty { parts.append(note) }
        if let category = template.category, !category.isEmpty { parts.append("#" + category) }
        return parts.isEmpty ? TransactionRow.placeholder : parts.joined(separator: " · ")
    }

    /// `Cash · Casa`. A template without a wallet takes the only active one
    /// when it runs, and one without an envelope takes Unallocated
    /// (`core/src/engine/recurring.rs`), which is what the line says.
    static func places(_ template: RecurringView, names: NameBook) -> String {
        let wallet = template.walletId.map { names.wallet($0) ?? TransactionRow.placeholder }
            ?? String(localized: "Any active wallet")
        let envelope = template.flowId.map { names.flow($0) ?? TransactionRow.placeholder }
            ?? NameBook.unallocatedLabel
        return "\(wallet) · \(envelope)"
    }
}
