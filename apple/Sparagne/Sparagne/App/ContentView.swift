import SwiftUI
import SparagneCore

/// Opens the database, then hands the window over to `MainWindow`. Owns the
/// store through a binding so `SparagneApp` can share the same instance
/// with the Categories `Window` scene.
struct ContentView: View {
    @Binding var store: AppStore?
    @Binding var engine: SyncEngine?
    @Binding var launchFailure: String?

    var body: some View {
        Group {
            if let store {
                MainWindow(store: store, engine: engine)
            } else if let launchFailure {
                ContentUnavailableView(
                    String(localized: "Sparagne could not open its database"),
                    systemImage: "externaldrive.badge.exclamationmark",
                    description: Text(launchFailure)
                )
            } else {
                ProgressView()
            }
        }
        .task { await open() }
    }

    private func open() async {
        guard store == nil, launchFailure == nil else { return }
        do {
            if let database = LaunchOptions.database {
                // Another database, with no engine: nothing in it is pushed,
                // and the window has no account or sharing to offer. New rows
                // are signed with the name chosen for rows written offline.
                let core = try CoreActor.onDisk(named: database, author: AccountStore.loggedOutAuthor())
                let opened = AppStore(core: core, defaultCategories: DefaultCategories.forAppLanguage())
                store = opened
                await opened.bootstrap()
                return
            }
            let core = try CoreActor.onDisk()
            let opened = AppStore(core: core, defaultCategories: DefaultCategories.forAppLanguage())
            // The engine adopts the account's username as the author and
            // starts the first sync right after bootstrap
            // (`docs/v2/SYNC.md` §5). It prepares before the bootstrap so the
            // first command is already signed with the account's name.
            let sync = SyncEngine(core: core, store: opened, account: AccountStore())
            store = opened
            engine = sync
            await sync.prepare()
            await opened.bootstrap()
            sync.start()
        } catch {
            launchFailure = error.localizedDescription
        }
    }
}

/// The single window: the ledger and its two summary views, plus the sheets,
/// the alerts and the undo toast (`docs/v2/UI.md` §2).
struct MainWindow: View {
    @Bindable var store: AppStore
    /// `nil` only before the database is open.
    let engine: SyncEngine?
    @State private var sheet: SheetKind?

    /// The sheets the window can present. Associated values seed the sheet
    /// with the entity being renamed or edited.
    enum SheetKind: Identifiable {
        case vault
        case renameVault(VaultView)
        case deleteVault(VaultView)
        case wallet
        case envelope
        case renameWallet(WalletView)
        case renameEnvelope(FlowView)
        case editEnvelope(FlowView)
        case recurring
        case newRecurring
        case dueRecurring
        case share(VaultView)
        case leaveVault(VaultView)
        case rejected
        case manage
        case importStatement

        var id: String {
            switch self {
            case .vault: "vault"
            case .renameVault(let vault): "renameVault-\(vault.id)"
            case .deleteVault(let vault): "deleteVault-\(vault.id)"
            case .wallet: "wallet"
            case .envelope: "envelope"
            case .renameWallet(let wallet): "renameWallet-\(wallet.id)"
            case .renameEnvelope(let flow): "renameEnvelope-\(flow.id)"
            case .editEnvelope(let flow): "editEnvelope-\(flow.id)"
            case .recurring: "recurring"
            case .newRecurring: "newRecurring"
            case .dueRecurring: "dueRecurring"
            case .share(let vault): "share-\(vault.id)"
            case .leaveVault(let vault): "leaveVault-\(vault.id)"
            case .rejected: "rejected"
            case .manage: "manage"
            case .importStatement: "importStatement"
            }
        }
    }

    var body: some View {
        LedgerWindow(store: store, engine: engine, sheet: $sheet)
            .frame(minWidth: 1176, minHeight: 640)
            .preferredColorScheme(.dark)
            .navigationTitle(title)
            // The file's name when it is not the real database, so rows made
            // up for a try are never mistaken for the ledger.
            .navigationSubtitle(Text(verbatim: LaunchOptions.database ?? ""))
            .toolbar { toolbar }
            .toolbarBackground(Ink.bg, for: .windowToolbar)
            .overlay(alignment: .bottom) {
                if let pending = store.pendingUndo {
                    UndoToast(pending: pending) { store.undo() }
                        .padding(.bottom, 48)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .animation(.default, value: store.pendingUndo)
            .onChange(of: store.needsOnboarding) { _, needed in
                if needed { sheet = .vault }
            }
            .task {
                if store.needsOnboarding { sheet = .vault }
            }
            .onReceive(NotificationCenter.default.publisher(for: .openManagement)) { _ in
                sheet = .manage
            }
            // The Vault menu and the palette act on the vault on screen; the
            // management sheet seeds the same sheets with the vault it shows.
            .onReceive(NotificationCenter.default.publisher(for: .newVault)) { _ in
                sheet = .vault
            }
            .onReceive(NotificationCenter.default.publisher(for: .renameVault)) { _ in
                if let vault = store.currentVault { sheet = .renameVault(vault) }
            }
            .onReceive(NotificationCenter.default.publisher(for: .deleteVault)) { _ in
                if let vault = store.currentVault { sheet = .deleteVault(vault) }
            }
            .onReceive(NotificationCenter.default.publisher(for: .leaveVault)) { _ in
                if let vault = store.currentVault { sheet = .leaveVault(vault) }
            }
            .onReceive(NotificationCenter.default.publisher(for: .importStatement)) { _ in
                if store.currentVault != nil { sheet = .importStatement }
            }
            .onReceive(NotificationCenter.default.publisher(for: .reviewDueRecurring)) { _ in
                sheet = .dueRecurring
            }
            .onReceive(NotificationCenter.default.publisher(for: .newRecurring)) { _ in
                if store.canWrite { sheet = .newRecurring }
            }
            // Back Up Database and Export All Transactions: file panels owned
            // by the exporter (`Support/VaultExporter.swift`).
            .modifier(VaultExportHandlers(store: store))
            .sheet(item: $sheet) { kind in sheetBody(kind) }
            .alert(
                String(localized: "Some changes were refused"),
                isPresented: Binding(
                    get: { engine?.showsRejectedAlert ?? false },
                    set: { engine?.showsRejectedAlert = $0 }
                )
            ) {
                Button(String(localized: "Review")) {
                    engine?.showsRejectedAlert = false
                    sheet = .rejected
                }
                Button(String(localized: "Later"), role: .cancel) { engine?.showsRejectedAlert = false }
            } message: {
                Text(String(localized: "The server did not accept them, so they are not in your balances."))
            }
            .alert(
                store.presentedError.map { ErrorMessages.summary(for: $0.code) } ?? String(localized: "Something went wrong"),
                isPresented: Binding(
                    get: { store.presentedError != nil },
                    set: { if !$0 { store.presentedError = nil } }
                ),
                presenting: store.presentedError
            ) { error in
                // ambiguous_name: one button per candidate name, rewriting the
                // marker in quickAddText and resubmitting.
                if error.candidates.isEmpty {
                    Button(String(localized: "OK"), role: .cancel) { store.presentedError = nil }
                } else {
                    ForEach(error.candidates, id: \.self) { candidate in
                        Button(candidate) { Task { await store.resolveAmbiguous(choosing: candidate) } }
                    }
                    Button(String(localized: "Cancel"), role: .cancel) { store.presentedError = nil }
                }
            } message: { error in
                Text(error.message)
            }
    }

    /// `Sparagne — libro mastro — 2026`, as in the mockups.
    private var title: String {
        let vault = store.currentVault?.name ?? String(localized: "Sparagne")
        return "\(vault) \u{2014} \(store.tab.label.lowercased()) \u{2014} \(store.month.year)"
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        if #available(macOS 26.0, *) {
            // macOS 26 wraps every toolbar item in a glass capsule; the strip
            // is square by design (`docs/v2/UI.md` §2) and sits badly in one.
            ToolbarItem(placement: .principal) { switcher }
                .sharedBackgroundVisibility(.hidden)
        } else {
            ToolbarItem(placement: .principal) { switcher }
        }
        if let engine {
            ToolbarItem(placement: .primaryAction) {
                SyncStatusButton(engine: engine) { sheet = .rejected }
            }
        }
    }

    private var switcher: some View {
        SegmentedStrip(
            options: LedgerTab.allCases,
            selection: $store.tab,
            label: { $0.label.uppercased() }
        )
        // `SegmentedStrip` (`Views/Ledger/LedgerHeader.swift`) draws its own
        // buttons and is owned by another package right now, so its own
        // accessibility can't be touched from here; this substitutes an
        // equivalent tree — one real button per tab, the one on screen
        // marked selected — for VoiceOver.
        .accessibilityRepresentation {
            HStack(spacing: 0) {
                ForEach(LedgerTab.allCases) { tab in
                    Button(tab.label) { store.tab = tab }
                        .accessibilityAddTraits(tab == store.tab ? .isSelected : [])
                }
            }
        }
    }

    @ViewBuilder
    private func sheetBody(_ kind: SheetKind) -> some View {
        switch kind {
        case .vault:
            OnboardingSheet(
                isFirstRun: store.needsOnboarding,
                takenNames: VaultNaming.siblingNames(owner: store.currentAuthor, in: store.vaults)
            ) { name, wallet, opening in
                Task { await store.createVault(name: name, walletName: wallet, openingBalance: opening) }
            }
            .interactiveDismissDisabled(store.needsOnboarding)
        case .renameVault(let vault):
            RenameSheet(
                title: String(localized: "Rename Vault"),
                name: vault.name,
                takenNames: VaultNaming.siblingNames(owner: vault.owner, excluding: vault.id, in: store.vaults)
            ) { name in
                Task { await store.renameVault(vault.id, name: name) }
            }
        case .deleteVault(let vault):
            DeleteVaultSheet(vault: vault) {
                Task { await store.deleteVault(vault.id) }
            }
        case .wallet:
            NewWalletSheet(currency: store.currency) { name, opening in
                Task { await store.createWallet(name: name, openingBalance: opening) }
            }
        case .envelope:
            NewEnvelopeSheet(currency: store.currency) { name, mode, allowNegative, allocation in
                Task {
                    await store.createEnvelope(
                        name: name,
                        mode: mode,
                        allowNegative: allowNegative,
                        openingAllocation: allocation
                    )
                }
            }
        case .renameWallet(let wallet):
            RenameSheet(title: String(localized: "Rename Wallet"), name: wallet.name) { name in
                Task { await store.renameWallet(wallet.id, name: name) }
            }
        case .renameEnvelope(let flow):
            RenameSheet(title: String(localized: "Rename Envelope"), name: flow.name) { name in
                Task { await store.updateEnvelope(flow.id, name: name) }
            }
        case .editEnvelope(let flow):
            EditEnvelopeSheet(flow: flow, currency: store.currency) { mode, allowNegative in
                Task { await store.updateEnvelope(flow.id, mode: mode, allowNegative: allowNegative) }
            }
        case .recurring:
            RecurringPanel(store: store)
        case .newRecurring:
            RecurringTemplateSheet(store: store, template: nil)
        case .dueRecurring:
            DueRecurringSheet(store: store)
        case .share(let vault):
            if let engine {
                ShareVaultSheet(engine: engine, vault: vault)
            }
        case .leaveVault(let vault):
            if let engine {
                LeaveVaultSheet(engine: engine, vault: vault)
            }
        case .rejected:
            if let engine {
                RejectedChangesSheet(engine: engine)
            }
        case .manage:
            ManagementSheet(store: store, engine: engine, present: { sheet = $0 })
        case .importStatement:
            StatementImportSheet(store: store)
        }
    }
}
