import SwiftUI
import SparagneCore

/// The Ricorrenze tab (`docs/v2/UI.md` §2.5). On the left, what is waiting
/// for a decision ("Da confermare"), what is coming ("Prossimi 30 giorni")
/// and every template ("Modelli"); on the right, the inspector of the
/// template picked in the table, or of a new one.
///
/// It replaces the old modal sheets: the due periods are recorded from the
/// first card, and the templates are created and edited in the inspector.
/// Nuova ricorrenza… in the Vault menu and the palette opens it in create
/// mode (`requestCreate`).
struct RecurringTab: View {
    let store: AppStore

    /// The user's pick in the table. `nil` until there is one, and then the
    /// inspector shows the first template (`shown`), so the layout never
    /// jumps between a tab with an inspector and one without.
    @State private var picked: RecurringSelection?
    /// The day everything on the tab counts from. Taken again when the
    /// calendar day turns and whenever the due list changes, so a tab left
    /// open past midnight does not keep counting from yesterday.
    @State private var today = CoreDate.day(Date())
    /// The table's Prossima per template (`AppStore.nextRecurring`), worked
    /// out with the agenda rather than by each row as it draws.
    @State private var next: [Uuid: RecurringNext] = [:]
    /// What "Duplicate" in the inspector's menu hands to create mode: the
    /// new template's first draft. `nil` for a blank one.
    @State private var seed: RecurringDraft?

    static let inspectorWidth: CGFloat = 340

    var body: some View {
        HStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    DueRecurringCard(store: store, today: today)
                    RecurringAgendaCard(store: store)
                    RecurringTemplateTable(store: store, today: today, next: next, selection: shown) {
                        seed = nil
                        picked = $0
                    }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .topLeading)
            }
            Rectangle().fill(Ink.line).frame(width: 1)
            inspector
                .frame(width: Self.inspectorWidth)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task(id: store.currentVault?.id) {
            refreshToday()
            await store.loadRecurringTemplates()
        }
        // The agenda and the table's next dates are worked out again
        // whenever the templates, the due list or the day change: a save, a
        // period recorded, a sync, midnight.
        .task(id: AgendaInputs(templates: store.recurringTemplates, pending: store.pendingRecurringItems, today: today)) {
            await store.loadUpcomingRecurring(today: today)
            next = store.nextRecurring(today: today)
        }
        // The core works the due list out from its own clock: when it moves,
        // the day may have too.
        .onChange(of: store.pendingRecurringItems) { _, _ in refreshToday() }
        // The notification promises no thread, and `today` is the main
        // actor's.
        .onReceive(
            NotificationCenter.default.publisher(for: .NSCalendarDayChanged).receive(on: DispatchQueue.main)
        ) { _ in
            refreshToday()
        }
        .onAppear(perform: takeCreateRequest)
        .onReceive(NotificationCenter.default.publisher(for: .createRecurringOnTab)) { _ in
            takeCreateRequest()
        }
        // Another vault: the pick named a template of the vault that is gone.
        .onChange(of: store.currentVault?.id) { _, _ in picked = nil }
    }

    private func refreshToday() {
        today = CoreDate.day(Date())
    }

    // MARK: - Selection

    /// What the inspector shows: the pick while it still exists, else the
    /// first template, else a new one (or nothing, for a vault the account
    /// only reads).
    private var shown: RecurringSelection? {
        let templates = store.recurringTemplatesInTableOrder
        switch picked {
        case .template(let id) where templates.contains(where: { $0.id == id }):
            return picked
        case .new where store.canWrite:
            return .new
        default:
            if let first = templates.first { return .template(first.id) }
            return store.canWrite ? .new : nil
        }
    }

    @ViewBuilder
    private var inspector: some View {
        switch shown {
        case .template(let id):
            if let template = store.recurringTemplates.first(where: { $0.id == id }) {
                RecurringInspector(
                    store: store,
                    template: template,
                    today: today,
                    duplicate: { draft in
                        seed = draft
                        picked = .new
                    }
                ) { _ in }
                .id(RecurringSelection.template(id))
            }
        case .new:
            RecurringInspector(store: store, template: nil, seed: seed, today: today) { created in
                seed = nil
                picked = created.map(RecurringSelection.template)
            }
            .id(RecurringSelection.new)
        case nil:
            Text(String(localized: "No recurring templates"))
                .font(Face.ui(12))
                .foregroundStyle(Ink.text3)
                .padding(16)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .background(Ink.bg)
        }
    }

    // MARK: - Nuova ricorrenza…

    /// Set by `requestCreate`, read once by whichever comes first: the tab
    /// appearing, or the notification that follows the request.
    @MainActor private static var createRequested = false

    /// The Vault menu's and the palette's Nuova ricorrenza…: switches to
    /// this tab and opens the inspector on a new template.
    ///
    /// The tab may not be on screen yet, so the request is left where the
    /// tab finds it when it appears, and also announced once the switch has
    /// had a turn to lay the tab out, for a tab that was already there.
    @MainActor static func requestCreate(store: AppStore) {
        guard store.canWrite else { return }
        createRequested = true
        store.tab = .recurring
        Task { @MainActor in
            await Task.yield()
            NotificationCenter.default.post(name: .createRecurringOnTab, object: nil)
        }
    }

    private func takeCreateRequest() {
        guard Self.createRequested else { return }
        Self.createRequested = false
        if store.canWrite {
            seed = nil
            picked = .new
        }
    }
}

/// What the agenda is built from, as one value for `.task(id:)`.
private struct AgendaInputs: Equatable {
    let templates: [RecurringView]
    let pending: [PendingRecurring]
    let today: NaiveDate
}

extension Notification.Name {
    /// Posted by `RecurringTab.requestCreate` once the tab is on screen: the
    /// inspector opens on a new template.
    static let createRecurringOnTab = Notification.Name("it.oghma.sparagne.createRecurringOnTab")
}
