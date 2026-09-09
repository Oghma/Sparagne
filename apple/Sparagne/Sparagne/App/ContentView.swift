import SwiftUI
import Observation

struct ContentView: View {
    @State private var model = AppModel()

    var body: some View {
        NavigationSplitView {
            SidebarView(model: model)
        } detail: {
            DetailView(model: model)
        }
    }
}

/// Placeholder view-state for the window. The app has no domain state of
/// its own (docs/v2/ARCH.md §2.2: "ogni vista è una query sul core"); once
/// the core package exists this becomes the thing that issues queries and
/// commands instead of reading `SampleData`.
@Observable
@MainActor
final class AppModel {
    var quickAddText = ""
    var searchText = ""
    var showTransfers = true
    var period: Period = .thisMonth
    var selection: TransactionRow.ID?
    var selectedVault: VaultRow = SampleData.vaults[0]

    enum Period: String, CaseIterable, Identifiable {
        case thisMonth
        case last30Days
        case all

        var id: String { rawValue }

        var label: String {
            switch self {
            case .thisMonth: String(localized: "This month")
            case .last30Days: String(localized: "Last 30 days")
            case .all: String(localized: "All")
            }
        }
    }
}

// MARK: - Sidebar

private struct SidebarView: View {
    @Bindable var model: AppModel

    var body: some View {
        List {
            Section {
                Menu {
                    ForEach(SampleData.vaults) { vault in
                        Button(vault.name) { model.selectedVault = vault }
                    }
                } label: {
                    Label(model.selectedVault.name, systemImage: "chevron.up.chevron.down")
                }
                .menuStyle(.borderlessButton)
            }

            Section(String(localized: "Wallets")) {
                ForEach(SampleData.wallets) { wallet in
                    WalletSidebarRow(wallet: wallet)
                }
            }

            Section(String(localized: "Envelopes")) {
                ForEach(SampleData.envelopes) { envelope in
                    EnvelopeSidebarRow(envelope: envelope)
                }
            }
        }
        .listStyle(.sidebar)
        .frame(minWidth: 220)
    }
}

private struct WalletSidebarRow: View {
    let wallet: WalletRow

    var body: some View {
        HStack {
            Text(wallet.name)
            Spacer()
            Text(MoneyFormatter.format(minorUnits: wallet.balanceMinor, currencyCode: SampleData.currencyCode))
                .monospacedDigit()
                .foregroundStyle(.secondary)
        }
    }
}

private struct EnvelopeSidebarRow: View {
    let envelope: EnvelopeRow

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(envelope.name)
                Spacer()
                if let capMinor = envelope.capMinor {
                    Text("\(MoneyFormatter.format(minorUnits: envelope.balanceMinor, currencyCode: SampleData.currencyCode)) / \(MoneyFormatter.format(minorUnits: capMinor, currencyCode: SampleData.currencyCode))")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Text(MoneyFormatter.format(minorUnits: envelope.balanceMinor, currencyCode: SampleData.currencyCode))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
            }
            if let progress = envelope.progress {
                ProgressView(value: progress)
                    .tint(AppTheme.progressTint(progress))
            }
        }
    }
}

// MARK: - Detail

private struct DetailView: View {
    @Bindable var model: AppModel
    @AppStorage("showVoided") private var showVoided = false
    @FocusState private var quickAddFocused: Bool
    @State private var isInspectorPresented = false

    private var filteredRows: [TransactionRow] {
        SampleData.transactions.filter { row in
            if row.isVoided, !showVoided { return false }
            if !model.showTransfers, row.kind == .transferWallet || row.kind == .transferFlow { return false }
            if !model.searchText.isEmpty {
                let query = model.searchText.lowercased()
                let haystack = [row.note, row.category ?? "", row.wallet, row.envelope]
                    .joined(separator: " ")
                    .lowercased()
                if !haystack.contains(query) { return false }
            }
            return true
        }
    }

    private var selectedRow: TransactionRow? {
        filteredRows.first { $0.id == model.selection }
    }

    var body: some View {
        VStack(spacing: 0) {
            quickAddBar
            Divider()
            filterBar
            Divider()
            TransactionsTableView(rows: filteredRows, selection: $model.selection)
        }
        .navigationTitle(model.selectedVault.name)
        .onReceive(NotificationCenter.default.publisher(for: .focusQuickAdd)) { _ in
            quickAddFocused = true
        }
        .onChange(of: model.selection) { _, newValue in
            isInspectorPresented = newValue != nil
        }
        .inspector(isPresented: $isInspectorPresented) {
            InspectorView(row: selectedRow)
        }
    }

    private var quickAddBar: some View {
        VStack(alignment: .leading, spacing: 4) {
            TextField(String(localized: "-12.50 pizza #food @cash >groceries"), text: $model.quickAddText)
                .textFieldStyle(.roundedBorder)
                .focused($quickAddFocused)
                .onSubmit { model.quickAddText = "" }

            Text(QuickAddPreview.describe(model.quickAddText) ?? " ")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding([.horizontal, .top])
        .padding(.bottom, 4)
    }

    private var filterBar: some View {
        HStack(spacing: 12) {
            HStack {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField(String(localized: "Search"), text: $model.searchText)
                    .textFieldStyle(.plain)
            }
            .frame(maxWidth: 220)

            Toggle(String(localized: "Voided"), isOn: $showVoided)
            Toggle(String(localized: "Transfers"), isOn: $model.showTransfers)

            Spacer()

            Picker(String(localized: "Period"), selection: $model.period) {
                ForEach(AppModel.Period.allCases) { period in
                    Text(period.label).tag(period)
                }
            }
            .pickerStyle(.segmented)
            .frame(maxWidth: 320)
            .labelsHidden()
        }
        .padding(.horizontal)
        .padding(.bottom, 8)
    }
}

// MARK: - Transactions table

private struct TransactionsTableView: View {
    let rows: [TransactionRow]
    @Binding var selection: TransactionRow.ID?
    @State private var sortOrder = [KeyPathComparator(\TransactionRow.occurredAt, order: .reverse)]

    private var sortedRows: [TransactionRow] {
        rows.sorted(using: sortOrder)
    }

    var body: some View {
        Table(sortedRows, selection: $selection, sortOrder: $sortOrder) {
            TableColumn(String(localized: "Date"), value: \.occurredAt) { row in
                Text(DateFormatting.relativeDayAndTime(row.occurredAt))
                    .strikethrough(row.isVoided)
            }
            .width(min: 130, ideal: 170)

            TableColumn(String(localized: "Type")) { row in
                Image(systemName: AppTheme.symbolName(for: row.kind))
                    .foregroundStyle(AppTheme.amountColor(for: row.kind))
            }
            .width(36)

            TableColumn(String(localized: "Amount"), value: \.amountMinor) { row in
                Text(MoneyFormatter.formatSigned(minorUnits: row.amountMinor, currencyCode: SampleData.currencyCode))
                    .monospacedDigit()
                    .foregroundStyle(AppTheme.amountColor(for: row.kind))
                    .strikethrough(row.isVoided)
            }
            .width(min: 90, ideal: 110)

            TableColumn(String(localized: "Wallet")) { row in
                Text(row.walletDisplay).strikethrough(row.isVoided)
            }

            TableColumn(String(localized: "Envelope")) { row in
                Text(row.envelopeDisplay).strikethrough(row.isVoided)
            }

            TableColumn(String(localized: "Category")) { row in
                Text(row.category ?? "—")
                    .foregroundStyle(row.category == nil ? .secondary : .primary)
                    .strikethrough(row.isVoided)
            }

            TableColumn(String(localized: "Note")) { row in
                Text(row.note).strikethrough(row.isVoided)
            }
        }
        .contextMenu(forSelectionType: TransactionRow.ID.self) { _ in
            Button(String(localized: "Void")) {}
            Button(String(localized: "Edit")) {}
        }
    }
}

// MARK: - Inspector

private struct InspectorView: View {
    let row: TransactionRow?

    var body: some View {
        Group {
            if let row {
                Form {
                    LabeledContent(String(localized: "Date"), value: DateFormatting.relativeDayAndTime(row.occurredAt))
                    LabeledContent(String(localized: "Amount"), value: MoneyFormatter.formatSigned(minorUnits: row.amountMinor, currencyCode: SampleData.currencyCode))
                    LabeledContent(String(localized: "Wallet"), value: row.walletDisplay)
                    LabeledContent(String(localized: "Envelope"), value: row.envelopeDisplay)
                    LabeledContent(String(localized: "Category"), value: row.category ?? "—")
                    LabeledContent(String(localized: "Note"), value: row.note)
                }
                .formStyle(.grouped)
            } else {
                ContentUnavailableView(String(localized: "No selection"), systemImage: "sidebar.trailing")
            }
        }
        .frame(minWidth: 240)
    }
}

#Preview {
    ContentView()
}
