import SwiftUI
import SparagneCore

/// Quick-add field, filter bar, totals line and the transactions table.
struct DetailView: View {
    @Bindable var store: AppStore

    /// Shared with the ⌘⇧V menu item in `SparagneApp`.
    @AppStorage("showVoided") private var showVoided = false
    @FocusState private var quickAddFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            QuickAddBar(store: store, focused: $quickAddFocused)
            Divider()
            filterBar
            Divider()
            TransactionsTable(store: store)
        }
        .onReceive(NotificationCenter.default.publisher(for: .focusQuickAdd)) { _ in
            quickAddFocused = true
        }
        .onAppear { store.showVoided = showVoided }
        .onChange(of: showVoided) { _, newValue in store.showVoided = newValue }
        .task(id: store.searchText) {
            // Debounce: reload only once the field has been quiet for 300 ms.
            guard (try? await Task.sleep(for: .milliseconds(300))) != nil else { return }
            store.reload()
        }
        .inspector(isPresented: Binding(
            get: { store.selection != nil },
            set: { if !$0 { store.selection = nil } }
        )) {
            Group {
                if let row = store.selectedRow {
                    InspectorView(store: store, row: row)
                        .id(row.id)
                } else {
                    ContentUnavailableView(
                        String(localized: "No selection"),
                        systemImage: "sidebar.trailing"
                    )
                }
            }
            .inspectorColumnWidth(min: 280, ideal: 320, max: 420)
        }
    }

    private var filterBar: some View {
        HStack(spacing: 12) {
            HStack(spacing: 4) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField(String(localized: "Search"), text: $store.searchText)
                    .textFieldStyle(.plain)
            }
            .frame(maxWidth: 220)

            Toggle(String(localized: "Voided"), isOn: $showVoided)
            Toggle(String(localized: "Transfers"), isOn: $store.showTransfers)

            Spacer()

            totalsLine

            Picker(String(localized: "Period"), selection: $store.period) {
                ForEach(Period.allCases) { period in
                    Text(period.label).tag(period)
                }
            }
            .pickerStyle(.segmented)
            .frame(maxWidth: 300)
            .labelsHidden()
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    /// Income and expenses net of refunds for the selected period
    /// (docs/v2/DISTILLATO_V1.md §3.5).
    @ViewBuilder
    private var totalsLine: some View {
        if let totals = store.totals {
            HStack(spacing: 10) {
                Label(
                    MoneyFormatter.format(minorUnits: totals.income, currencyCode: store.currencyCode),
                    systemImage: "arrow.down"
                )
                .foregroundStyle(AppTheme.income)

                Label(
                    MoneyFormatter.format(minorUnits: totals.netExpense, currencyCode: store.currencyCode),
                    systemImage: "arrow.up"
                )
                .foregroundStyle(AppTheme.expense)
            }
            .font(.callout)
            .monospacedDigit()
            .labelStyle(.titleAndIcon)
        }
    }
}

// MARK: - Quick add

private struct QuickAddBar: View {
    @Bindable var store: AppStore
    @FocusState.Binding var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            TextField(
                String(localized: "-12.50 pizza #food @cash >groceries"),
                text: $store.quickAddText
            )
            .textFieldStyle(.roundedBorder)
            .focused($focused)
            .disabled(store.currentVault == nil)
            .onSubmit { store.submit(quickAdd: store.quickAddText) }

            Text(preview.text)
                .font(.caption)
                .foregroundStyle(preview.isError ? AppTheme.critical : Color.secondary)
                .lineLimit(1)
        }
        .padding([.horizontal, .top])
        .padding(.bottom, 4)
    }

    private var preview: (text: String, isError: Bool) {
        let trimmed = store.quickAddText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return (" ", false) }
        switch store.preview(quickAdd: trimmed) {
        case .success(let parsed):
            return (QuickAddSummary.describe(parsed, currency: store.currency), false)
        case .failure(let error):
            return (error.message, true)
        }
    }
}

// MARK: - Table

private struct TransactionsTable: View {
    @Bindable var store: AppStore
    @State private var sortOrder = [KeyPathComparator(\TransactionRow.occurredAt, order: .reverse)]

    private var rows: [TransactionRow] {
        store.rows.sorted(using: sortOrder)
    }

    var body: some View {
        VStack(spacing: 0) {
            Table(rows, selection: $store.selection, sortOrder: $sortOrder) {
                TableColumn(String(localized: "Date"), value: \.occurredAt) { row in
                    Text(DateFormatting.relativeDayAndTime(row.occurredAt))
                        .strikethrough(row.voided)
                }
                .width(min: 130, ideal: 170)

                TableColumn(String(localized: "Type")) { row in
                    Image(systemName: AppTheme.symbolName(for: row.kind))
                        .foregroundStyle(AppTheme.amountColor(for: row.kind))
                        .help(kindLabel(row.kind))
                }
                .width(36)

                TableColumn(String(localized: "Amount"), value: \.signedAmount) { row in
                    Text(amountText(row))
                        .monospacedDigit()
                        .foregroundStyle(AppTheme.amountColor(for: row.kind))
                        .strikethrough(row.voided)
                }
                .width(min: 90, ideal: 120)

                TableColumn(String(localized: "Wallet"), value: \.walletDisplay) { row in
                    Text(row.walletDisplay).strikethrough(row.voided)
                }

                TableColumn(String(localized: "Envelope"), value: \.envelopeDisplay) { row in
                    Text(row.envelopeDisplay).strikethrough(row.voided)
                }

                TableColumn(String(localized: "Category"), value: \.category) { row in
                    Text(row.category).strikethrough(row.voided)
                }

                TableColumn(String(localized: "Note"), value: \.note) { row in
                    Text(row.note).strikethrough(row.voided)
                }
            }
            .contextMenu(forSelectionType: TransactionRow.ID.self) { ids in
                if let id = ids.first {
                    Button(String(localized: "Edit")) { store.selection = id }
                    Divider()
                    Button(String(localized: "Void"), role: .destructive) {
                        store.void(transactionId: id)
                    }
                }
            }

            if store.nextCursor != nil {
                Divider()
                HStack {
                    Spacer()
                    Button(String(localized: "Load More")) { store.loadMore() }
                    Spacer()
                }
                .padding(.vertical, 6)
            }
        }
    }

    /// Transfers are neutral: they move money without being income or expense
    /// (docs/v2/DISTILLATO_V1.md §1.2), so they keep an unsigned amount.
    private func amountText(_ row: TransactionRow) -> String {
        row.isTransfer
            ? MoneyFormatter.format(minorUnits: row.absoluteAmount, currencyCode: store.currencyCode)
            : MoneyFormatter.formatSigned(minorUnits: row.signedAmount, currencyCode: store.currencyCode)
    }

    private func kindLabel(_ kind: TransactionKind) -> String {
        switch kind {
        case .income: String(localized: "Income")
        case .expense: String(localized: "Expense")
        case .refund: String(localized: "Refund")
        case .transferWallet: String(localized: "Wallet transfer")
        case .transferFlow: String(localized: "Envelope transfer")
        }
    }
}
