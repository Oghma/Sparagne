import SwiftUI
import SparagneCore

/// Templates the user materializes period by period, never automatically
/// (docs/v2/DISTILLATO_V1.md §2.3): list, create, edit and archive. The
/// banner of periods still waiting for a decision lives in `DetailView`
/// (`RecurringBanner`), since it needs to be visible without opening this
/// sheet (team-lead task 4).
struct RecurringPanel: View {
    let store: AppStore
    @Environment(\.dismiss) private var dismiss
    @State private var sheet: TemplateSheet?

    private enum TemplateSheet: Identifiable {
        case new
        case edit(RecurringView)

        var id: String {
            switch self {
            case .new: "new"
            case .edit(let template): "edit-\(template.id)"
            }
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(String(localized: "Recurring")).font(.headline)
                Spacer()
                Button(String(localized: "New…")) { sheet = .new }
                Button(String(localized: "Done")) { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
            .padding()

            Divider()

            if store.recurringTemplates.isEmpty {
                ContentUnavailableView(
                    String(localized: "No recurring templates"),
                    systemImage: "arrow.triangle.2.circlepath"
                )
                .frame(maxHeight: .infinity)
            } else {
                List(store.recurringTemplates, id: \.id) { template in
                    RecurringRow(template: template, store: store)
                        .contentShape(Rectangle())
                        .onTapGesture { sheet = .edit(template) }
                        .contextMenu {
                            Button(String(localized: "Edit…")) { sheet = .edit(template) }
                            if !template.archived {
                                Button(String(localized: "Archive"), role: .destructive) {
                                    store.archiveRecurring(template.id)
                                }
                            }
                        }
                }
            }
        }
        .frame(width: 480, height: 420)
        .onAppear { store.loadRecurringTemplates() }
        .sheet(item: $sheet) { kind in
            switch kind {
            case .new:
                RecurringTemplateSheet(store: store, template: nil)
            case .edit(let template):
                RecurringTemplateSheet(store: store, template: template)
            }
        }
    }
}

private struct RecurringRow: View {
    let template: RecurringView
    let store: AppStore

    var body: some View {
        HStack {
            Image(systemName: template.kind == .income ? "arrow.down.circle" : "arrow.up.circle")
                .foregroundStyle(template.kind == .income ? Ink.positive : Ink.negative)
            VStack(alignment: .leading, spacing: 2) {
                Text(
                    MoneyFormatter.format(minorUnits: template.amount, currencyCode: store.currencyCode)
                        + " · " + ScheduleFormatting.describe(template.schedule)
                )
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if !template.enabled {
                Text(String(localized: "Disabled")).font(.caption2).foregroundStyle(.secondary)
            }
            if template.archived {
                Text(String(localized: "Archived")).font(.caption2).foregroundStyle(.secondary)
            }
        }
    }

    private var subtitle: String {
        var parts: [String] = []
        if let note = template.note, !note.isEmpty { parts.append(note) }
        if let category = template.category, !category.isEmpty { parts.append("#" + category) }
        return parts.isEmpty ? TransactionRow.placeholder : parts.joined(separator: " · ")
    }
}

/// Create or edit one template. Editing sends only the fields that changed,
/// the same convention as `InspectorView`'s `TransactionPatch`
/// (`RecurringPatch`, `docs/v2/ARCH.md` §4).
struct RecurringTemplateSheet: View {
    let store: AppStore
    /// `nil` = create.
    let template: RecurringView?

    @Environment(\.dismiss) private var dismiss

    @State private var kind: TransactionKind
    @State private var amountText: String
    @State private var walletId: Uuid?
    @State private var flowId: Uuid?
    @State private var category: String
    @State private var note: String
    @State private var frequencyKind: FrequencyKind
    @State private var weekday: Int
    @State private var monthDay: Int
    @State private var yearMonth: Int
    @State private var yearDay: Int
    @State private var interval: Int
    @State private var startDate: Date
    @State private var hasEndDate: Bool
    @State private var endDate: Date
    @State private var enabled: Bool

    /// `Frequency` as a flat picker choice, same idea as `NewEnvelopeSheet
    /// .CapKind` (docs/v2/DISTILLATO_V1.md §2.2: "mode as data").
    private enum FrequencyKind: String, CaseIterable, Identifiable {
        case daily, weekly, monthly, yearly

        var id: String { rawValue }

        var label: String {
            switch self {
            case .daily: String(localized: "Daily")
            case .weekly: String(localized: "Weekly")
            case .monthly: String(localized: "Monthly")
            case .yearly: String(localized: "Yearly")
            }
        }
    }

    init(store: AppStore, template: RecurringView?) {
        self.store = store
        self.template = template
        let schedule = template?.schedule

        _kind = State(initialValue: template?.kind ?? .expense)
        _amountText = State(initialValue: template.map { MoneyFormatter.editable(minorUnits: $0.amount) } ?? "")
        _walletId = State(initialValue: template?.walletId)
        _flowId = State(initialValue: template?.flowId)
        _category = State(initialValue: template?.category ?? "")
        _note = State(initialValue: template?.note ?? "")
        _enabled = State(initialValue: template?.enabled ?? true)

        var weekday = 1
        var monthDay = 1
        var yearMonth = 1
        var yearDay = 1
        var frequencyKind = FrequencyKind.monthly
        switch schedule?.frequency {
        case .none, .daily:
            frequencyKind = .daily
        case .weekly(let day):
            frequencyKind = .weekly
            weekday = Int(day)
        case .monthly(let day):
            frequencyKind = .monthly
            monthDay = Int(day)
        case .yearly(let month, let day):
            frequencyKind = .yearly
            yearMonth = Int(month)
            yearDay = Int(day)
        }
        // A brand-new template defaults to monthly, not daily, as the more
        // common recurring bill.
        _frequencyKind = State(initialValue: template == nil ? .monthly : frequencyKind)
        _weekday = State(initialValue: weekday)
        _monthDay = State(initialValue: monthDay)
        _yearMonth = State(initialValue: yearMonth)
        _yearDay = State(initialValue: yearDay)
        _interval = State(initialValue: Int(schedule?.interval ?? 1))
        _startDate = State(initialValue: schedule.flatMap { CoreDate.localDay($0.startDate) } ?? Date())
        _hasEndDate = State(initialValue: schedule?.endDate != nil)
        _endDate = State(initialValue: schedule?.endDate.flatMap { CoreDate.localDay($0) } ?? Date())
    }

    private var frequency: Frequency {
        switch frequencyKind {
        case .daily: .daily
        case .weekly: .weekly(weekday: UInt8(weekday))
        case .monthly: .monthly(day: UInt8(monthDay))
        case .yearly: .yearly(month: UInt8(yearMonth), day: UInt8(yearDay))
        }
    }

    private var schedule: Schedule {
        Schedule(
            frequency: frequency,
            interval: UInt32(max(interval, 1)),
            startDate: CoreDate.day(startDate),
            endDate: hasEndDate ? CoreDate.day(endDate) : nil
        )
    }

    private var amountMinor: Int64? {
        try? parseMoney(text: amountText, currency: store.currency)
    }

    private var canSave: Bool { (amountMinor.map { $0 > 0 }) ?? false }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(template == nil ? String(localized: "New Recurring") : String(localized: "Edit Recurring"))
                .font(.headline)

            Form {
                Section {
                    Picker(String(localized: "Kind"), selection: $kind) {
                        Text(String(localized: "Income")).tag(TransactionKind.income)
                        Text(String(localized: "Expense")).tag(TransactionKind.expense)
                    }
                    TextField(String(localized: "Amount"), text: $amountText)
                        .monospacedDigit()
                }

                Section {
                    Picker(String(localized: "Wallet"), selection: $walletId) {
                        Text(String(localized: "Any active wallet")).tag(Optional<Uuid>.none)
                        ForEach(store.wallets, id: \.id) { wallet in
                            Text(wallet.name).tag(Optional(wallet.id))
                        }
                    }
                    Picker(String(localized: "Envelope"), selection: $flowId) {
                        Text(String(localized: "Unallocated")).tag(Optional<Uuid>.none)
                        ForEach(store.flows.filter { !$0.isUnallocated }, id: \.id) { flow in
                            Text(flow.name).tag(Optional(flow.id))
                        }
                    }
                    TextField(String(localized: "Category"), text: $category)
                    TextField(String(localized: "Note"), text: $note)
                }

                Section {
                    Picker(String(localized: "Frequency"), selection: $frequencyKind) {
                        ForEach(FrequencyKind.allCases) { kind in
                            Text(kind.label).tag(kind)
                        }
                    }
                    switch frequencyKind {
                    case .daily:
                        EmptyView()
                    case .weekly:
                        Picker(String(localized: "Weekday"), selection: $weekday) {
                            ForEach(1...7, id: \.self) { day in
                                Text(ScheduleFormatting.weekdayName(UInt8(day))).tag(day)
                            }
                        }
                    case .monthly:
                        Stepper(String(localized: "Day") + " \(monthDay)", value: $monthDay, in: 1...31)
                    case .yearly:
                        Picker(String(localized: "Month"), selection: $yearMonth) {
                            ForEach(1...12, id: \.self) { month in
                                Text(ScheduleFormatting.monthName(UInt8(month))).tag(month)
                            }
                        }
                        Stepper(String(localized: "Day") + " \(yearDay)", value: $yearDay, in: 1...31)
                    }
                    Stepper(String(localized: "Every") + " \(interval)", value: $interval, in: 1...365)

                    DatePicker(String(localized: "Start date"), selection: $startDate, displayedComponents: .date)
                    Toggle(String(localized: "End date"), isOn: $hasEndDate)
                    if hasEndDate {
                        DatePicker(String(localized: "End date"), selection: $endDate, displayedComponents: .date)
                    }
                    // Meaningless on creation: a new template is always
                    // enabled (`.createRecurring` has no `enabled` field).
                    if template != nil {
                        Toggle(String(localized: "Enabled"), isOn: $enabled)
                    }
                }
            }
            .formStyle(.grouped)

            HStack {
                Spacer()
                Button(String(localized: "Cancel"), role: .cancel) { dismiss() }
                Button(String(localized: "Save"), action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canSave)
            }
        }
        .padding(20)
        .frame(width: 440)
    }

    private func save() {
        guard let amount = amountMinor else { return }
        let categoryValue = category.trimmingCharacters(in: .whitespaces)
        let noteValue = note.trimmingCharacters(in: .whitespaces)

        if let template {
            var patch = RecurringPatch()
            if amount != template.amount { patch.amount = amount }
            if walletId != template.walletId { patch.walletId = walletId }
            if flowId != template.flowId { patch.flowId = flowId }
            if categoryValue != (template.category ?? "") { patch.category = categoryValue }
            if noteValue != (template.note ?? "") { patch.note = noteValue }
            if schedule != template.schedule { patch.schedule = schedule }
            if enabled != template.enabled { patch.enabled = enabled }
            store.updateRecurring(template.id, patch: patch)
        } else {
            store.createRecurring(
                kind: kind,
                amount: amount,
                walletId: walletId,
                flowId: flowId,
                category: categoryValue.isEmpty ? nil : categoryValue,
                note: noteValue.isEmpty ? nil : noteValue,
                schedule: schedule
            )
        }
        dismiss()
    }
}
