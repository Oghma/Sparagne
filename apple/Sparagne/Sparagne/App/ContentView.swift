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
        .task { open() }
    }

    private func open() {
        guard store == nil, launchFailure == nil else { return }
        do {
            let client = try CoreClient.onDisk()
            let opened = AppStore(client: client)
            opened.bootstrap()
            // The engine adopts the account's username as the author and
            // starts the first sync right after bootstrap
            // (`docs/v2/SYNC.md` §5).
            let sync = SyncEngine(client: client, store: opened, account: AccountStore())
            store = opened
            engine = sync
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
        case wallet
        case envelope
        case renameWallet(WalletView)
        case renameEnvelope(FlowView)
        case editEnvelope(FlowView)
        case recurring
        case share(VaultView)
        case rejected
        case manage

        var id: String {
            switch self {
            case .vault: "vault"
            case .wallet: "wallet"
            case .envelope: "envelope"
            case .renameWallet(let wallet): "renameWallet-\(wallet.id)"
            case .renameEnvelope(let flow): "renameEnvelope-\(flow.id)"
            case .editEnvelope(let flow): "editEnvelope-\(flow.id)"
            case .recurring: "recurring"
            case .share(let vault): "share-\(vault.id)"
            case .rejected: "rejected"
            case .manage: "manage"
            }
        }
    }

    var body: some View {
        LedgerWindow(store: store, engine: engine, sheet: $sheet)
            .frame(minWidth: 1080, minHeight: 640)
            .preferredColorScheme(.dark)
            .navigationTitle(title)
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
                        Button(candidate) { store.resolveAmbiguous(choosing: candidate) }
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
    }

    @ViewBuilder
    private func sheetBody(_ kind: SheetKind) -> some View {
        switch kind {
        case .vault:
            OnboardingSheet(isFirstRun: store.needsOnboarding) { name, wallet, opening in
                store.createVault(name: name, walletName: wallet, openingBalance: opening)
            }
            .interactiveDismissDisabled(store.needsOnboarding)
        case .wallet:
            NewWalletSheet(currency: store.currency) { name, opening in
                store.createWallet(name: name, openingBalance: opening)
            }
        case .envelope:
            NewEnvelopeSheet(currency: store.currency) { name, mode, allowNegative, allocation in
                store.createEnvelope(
                    name: name,
                    mode: mode,
                    allowNegative: allowNegative,
                    openingAllocation: allocation
                )
            }
        case .renameWallet(let wallet):
            RenameSheet(title: String(localized: "Rename Wallet"), name: wallet.name) { name in
                store.renameWallet(wallet.id, name: name)
            }
        case .renameEnvelope(let flow):
            RenameSheet(title: String(localized: "Rename Envelope"), name: flow.name) { name in
                store.updateEnvelope(flow.id, name: name)
            }
        case .editEnvelope(let flow):
            EditEnvelopeSheet(flow: flow, currency: store.currency) { mode, allowNegative in
                store.updateEnvelope(flow.id, mode: mode, allowNegative: allowNegative)
            }
        case .recurring:
            RecurringPanel(store: store)
        case .share(let vault):
            if let engine {
                ShareVaultSheet(engine: engine, vault: vault)
            }
        case .rejected:
            if let engine {
                RejectedChangesSheet(engine: engine)
            }
        case .manage:
            ManagementSheet(store: store, engine: engine, present: { sheet = $0 })
        }
    }
}
