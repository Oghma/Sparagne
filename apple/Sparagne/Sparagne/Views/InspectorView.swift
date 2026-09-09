import SwiftUI
import SparagneCore

/// Edits the selected transaction. Save sends only the fields that changed
/// (`UpdateTransaction` is a partial update, docs/v2/ARCH.md §4); Cancel puts
/// the draft back to what the row says.
struct InspectorView: View {
    let store: AppStore
    let row: TransactionRow

    @State private var draft: Draft

    init(store: AppStore, row: TransactionRow) {
        self.store = store
        self.row = row
        _draft = State(initialValue: Draft(row: row))
    }

    /// The editable shape of a row.
    private struct Draft: Equatable {
        var amountText: String
        var date: Date
        var category: String
        var note: String
        var walletId: Uuid?
        var flowId: Uuid?
        var destinationId: Uuid?

        init(row: TransactionRow) {
            amountText = MoneyFormatter.editable(minorUnits: row.absoluteAmount)
            date = row.occurredAt
            category = row.category
            note = row.note
            walletId = row.walletId
            flowId = row.flowId
            destinationId = row.destinationId
        }
    }

    private var isDirty: Bool { draft != Draft(row: row) }

    var body: some View {
        Form {
            Section {
                TextField(String(localized: "Amount"), text: $draft.amountText)
                    .monospacedDigit()
                DatePicker(
                    String(localized: "Date"),
                    selection: $draft.date,
                    displayedComponents: [.date, .hourAndMinute]
                )
            }

            switch row.kind {
            case .income, .expense, .refund:
                Section {
                    Picker(String(localized: "Wallet"), selection: $draft.walletId) {
                        ForEach(store.wallets, id: \.id) { wallet in
                            Text(wallet.name).tag(Optional(wallet.id))
                        }
                    }
                    Picker(String(localized: "Envelope"), selection: $draft.flowId) {
                        ForEach(store.flows, id: \.id) { flow in
                            Text(store.flowName(flow)).tag(Optional(flow.id))
                        }
                    }
                    categoryField
                }
            case .transferWallet:
                Section {
                    Picker(String(localized: "From"), selection: $draft.walletId) {
                        ForEach(store.wallets, id: \.id) { wallet in
                            Text(wallet.name).tag(Optional(wallet.id))
                        }
                    }
                    Picker(String(localized: "To"), selection: $draft.destinationId) {
                        ForEach(store.wallets, id: \.id) { wallet in
                            Text(wallet.name).tag(Optional(wallet.id))
                        }
                    }
                }
            case .transferFlow:
                Section {
                    Picker(String(localized: "From"), selection: $draft.flowId) {
                        ForEach(store.flows, id: \.id) { flow in
                            Text(store.flowName(flow)).tag(Optional(flow.id))
                        }
                    }
                    Picker(String(localized: "To"), selection: $draft.destinationId) {
                        ForEach(store.flows, id: \.id) { flow in
                            Text(store.flowName(flow)).tag(Optional(flow.id))
                        }
                    }
                }
            }

            Section {
                TextField(String(localized: "Note"), text: $draft.note, axis: .vertical)
                    .lineLimit(1...4)
            }

            Section {
                HStack {
                    Button(String(localized: "Cancel")) { draft = Draft(row: row) }
                        .disabled(!isDirty)
                    Spacer()
                    Button(String(localized: "Save")) { save() }
                        .keyboardShortcut(.defaultAction)
                        .disabled(!isDirty)
                }
            }
        }
        .formStyle(.grouped)
        .frame(minWidth: 260)
    }

    /// Free text, with the vault's existing categories as suggestions.
    private var categoryField: some View {
        HStack {
            TextField(String(localized: "Category"), text: $draft.category)
            Menu {
                ForEach(store.categories, id: \.id) { category in
                    Button(category.name) { draft.category = category.name }
                }
            } label: {
                Image(systemName: "chevron.down")
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .disabled(store.categories.isEmpty)
        }
    }

    private func save() {
        var patch = TransactionPatch()

        if let amount = try? parseMoney(text: draft.amountText, currency: store.currency),
           amount != row.absoluteAmount {
            patch.amount = amount
        }
        if draft.date != row.occurredAt {
            patch.occurredAt = draft.date
        }
        if draft.note != row.note {
            patch.note = draft.note
        }

        switch row.kind {
        case .income, .expense, .refund:
            if draft.category != row.category { patch.category = draft.category }
            if draft.walletId != row.walletId { patch.walletId = draft.walletId }
            if draft.flowId != row.flowId { patch.flowId = draft.flowId }
        case .transferWallet:
            if draft.walletId != row.walletId { patch.fromId = draft.walletId }
            if draft.destinationId != row.destinationId { patch.toId = draft.destinationId }
        case .transferFlow:
            if draft.flowId != row.flowId { patch.fromId = draft.flowId }
            if draft.destinationId != row.destinationId { patch.toId = draft.destinationId }
        }

        store.update(transactionId: row.id, patch: patch)
    }
}
