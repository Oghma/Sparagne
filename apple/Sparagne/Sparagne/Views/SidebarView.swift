import SwiftUI
import SparagneCore

/// Vault picker, wallet balances and envelope balances.
///
/// Archived wallets and envelopes are hidden; capped envelopes show
/// `balance / cap` with the 70%/90% tinted bar (docs/v2/DISTILLATO_V1.md §3.4).
struct SidebarView: View {
    let store: AppStore
    let onNewVault: () -> Void
    let onNewWallet: () -> Void
    let onNewEnvelope: () -> Void

    var body: some View {
        List {
            Section {
                Menu {
                    ForEach(store.vaults, id: \.id) { vault in
                        Button(vault.name) { store.select(vault) }
                    }
                    Divider()
                    Button(String(localized: "New Vault…"), action: onNewVault)
                } label: {
                    Label(
                        store.currentVault?.name ?? String(localized: "No vault"),
                        systemImage: "chevron.up.chevron.down"
                    )
                }
                .menuStyle(.borderlessButton)
            }

            Section(String(localized: "Wallets")) {
                ForEach(store.wallets, id: \.id) { wallet in
                    WalletSidebarRow(wallet: wallet, currencyCode: store.currencyCode)
                }
                Button(String(localized: "New Wallet…"), action: onNewWallet)
                    .buttonStyle(.link)
                    .disabled(store.currentVault == nil)
            }

            Section(String(localized: "Envelopes")) {
                ForEach(store.flows, id: \.id) { flow in
                    EnvelopeSidebarRow(
                        flow: flow,
                        name: store.flowName(flow),
                        currencyCode: store.currencyCode
                    )
                }
                Button(String(localized: "New Envelope…"), action: onNewEnvelope)
                    .buttonStyle(.link)
                    .disabled(store.currentVault == nil)
            }
        }
        .listStyle(.sidebar)
        .frame(minWidth: 220)
    }
}

private struct WalletSidebarRow: View {
    let wallet: WalletView
    let currencyCode: String

    var body: some View {
        HStack {
            Text(wallet.name)
            Spacer()
            Text(MoneyFormatter.format(minorUnits: wallet.balance, currencyCode: currencyCode))
                .monospacedDigit()
                .foregroundStyle(.secondary)
        }
    }
}

private struct EnvelopeSidebarRow: View {
    let flow: FlowView
    let name: String
    let currencyCode: String

    /// The cap, when the mode has one.
    private var cap: Int64? {
        switch flow.mode {
        case .unlimited: nil
        case .netCapped(let cap), .incomeCapped(let cap): cap
        }
    }

    /// What fills the bar: the balance for a net cap, the cumulative income
    /// for an income cap (docs/v2/DISTILLATO_V1.md §2.2).
    private var filled: Int64 {
        switch flow.mode {
        case .incomeCapped: flow.incomeTotal ?? flow.balance
        case .unlimited, .netCapped: flow.balance
        }
    }

    private var progress: Double? {
        guard let cap, cap > 0 else { return nil }
        return min(max(Double(filled) / Double(cap), 0), 1)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(name)
                Spacer()
                if let cap {
                    Text(
                        "\(MoneyFormatter.format(minorUnits: flow.balance, currencyCode: currencyCode)) / \(MoneyFormatter.format(minorUnits: cap, currencyCode: currencyCode))"
                    )
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                } else {
                    Text(MoneyFormatter.format(minorUnits: flow.balance, currencyCode: currencyCode))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
            }
            if let progress {
                ProgressView(value: progress)
                    .tint(AppTheme.progressTint(progress))
            }
        }
    }
}
