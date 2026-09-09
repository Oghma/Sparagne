import SwiftUI
import SparagneCore

/// Opens the database, then hands the window over to `MainWindow`.
struct ContentView: View {
    @State private var store: AppStore?
    @State private var launchFailure: String?

    var body: some View {
        Group {
            if let store {
                MainWindow(store: store)
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
            let opened = AppStore(client: try CoreClient.onDisk())
            opened.bootstrap()
            store = opened
        } catch {
            launchFailure = error.localizedDescription
        }
    }
}

/// The sidebar/detail split, plus the sheets, the alert and the undo toast.
struct MainWindow: View {
    let store: AppStore
    @State private var sheet: SheetKind?

    /// The three small sheets the window can present.
    enum SheetKind: String, Identifiable {
        case vault
        case wallet
        case envelope

        var id: String { rawValue }
    }

    var body: some View {
        NavigationSplitView {
            SidebarView(
                store: store,
                onNewVault: { sheet = .vault },
                onNewWallet: { sheet = .wallet },
                onNewEnvelope: { sheet = .envelope }
            )
        } detail: {
            DetailView(store: store)
        }
        .navigationTitle(store.currentVault?.name ?? String(localized: "Sparagne"))
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
            }
        }
        .alert(
            String(localized: "Something went wrong"),
            isPresented: Binding(
                get: { store.presentedError != nil },
                set: { if !$0 { store.presentedError = nil } }
            ),
            presenting: store.presentedError
        ) { _ in
            Button(String(localized: "OK"), role: .cancel) { store.presentedError = nil }
        } message: { error in
            Text(error.message)
        }
    }
}
