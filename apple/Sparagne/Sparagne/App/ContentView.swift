import SwiftUI
import SparagneCore

/// Opens the database, then hands the window over to `MainWindow`. Owns the
/// store through a binding so `SparagneApp` can share the same instance
/// with the Categories `Window` scene (team-lead task 3).
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

/// The sidebar/detail split, plus the sheets, the alert and the undo toast.
struct MainWindow: View {
    let store: AppStore
    /// `nil` only before the database is open.
    let engine: SyncEngine?
    @State private var sheet: SheetKind?

    /// The sheets the window can present. Associated values seed the sheet
    /// with the entity being renamed or edited (team-lead task 2).
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
            }
        }
    }

    var body: some View {
        NavigationSplitView {
            SidebarView(store: store, engine: engine, present: { sheet = $0 })
        } detail: {
            DetailView(store: store)
        }
        .navigationTitle(store.currentVault?.name ?? String(localized: "Sparagne"))
        .toolbar {
            if let engine {
                ToolbarItem(placement: .primaryAction) {
                    SyncStatusButton(engine: engine) { sheet = .rejected }
                }
            }
        }
        .overlay(alignment: .bottom) {
            if let pending = store.pendingUndo {
                UndoToast(pending: pending) { store.undo() }
                    .padding(.bottom, 24)
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
        .sheet(item: $sheet) { kind in
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
            }
        }
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
            // marker in quickAddText and resubmitting (task 1).
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
}
