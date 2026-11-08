import SwiftUI
import SparagneCore

/// Column geometry of the wallet table, as the canvas
/// has it: NOME takes what is left, SALDO 140 and the action column 36.
private enum WalletColumn {
    static let nameMinimum: CGFloat = 100
    static let balance: CGFloat = 140
}

/// The vault's wallets as a table edited by rows (`editsRows`); SALDO is typed only as an opening balance.
struct WalletTable: View {
    @Bindable var store: AppStore

    @State private var rows = RowEditing(WalletDraft())
    @State private var hovered: Uuid?
    @FocusState private var focus: WalletCell?

    var body: some View {
        SetupCard(title: String(localized: "Wallets")) {
            SetupTableBody {
                header
                ForEach(store.wallets.filter { matchesFilter($0.name) }, id: \.id) { wallet in
                    activeRow(wallet)
                }
                // A viewer reads the wallets; nothing to add.
                if store.canWrite { newLineRow }
                archived
                totalRow
            }
            if store.canWrite { footnote }
        }
        .editsRows($rows, focus: $focus, hovered: hovered, store: store) { commit($0) }
    }

    /// The top bar's search; the empty line and the total stay.
    private func matchesFilter(_ name: String) -> Bool {
        TableFilter.matches(store.tabFilter, [name])
    }

    // MARK: - Chrome

    /// Where an opening balance goes, which is what ties the wallets to the
    /// envelopes: Σ wallets = Σ envelopes.
    private var footnote: some View {
        Text(String(localized: "The opening balance of a new wallet goes to Unallocated"))
            .font(Face.ui(11.5))
            .foregroundStyle(Ink.text3)
            .padding(.top, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var header: some View {
        SetupHeaderRow {
            GridCell { Text(String(localized: "Name")) }
                .frame(minWidth: WalletColumn.nameMinimum)
            GridCell(width: WalletColumn.balance, alignment: .trailing) {
                Text(String(localized: "Balance"))
            }
            GridCell(width: SetupColumn.action) { EmptyView() }
        }
    }

    /// What all the wallets hold together, which is also what the envelopes
    /// add up to.
    private var totalRow: some View {
        SetupRow(separator: false, topRule: true) {
            GridCell {
                Text(String(localized: "Total")).font(Face.row.weight(.semibold)).foregroundStyle(Ink.text)
            }
            .frame(minWidth: WalletColumn.nameMinimum)
            GridCell(width: WalletColumn.balance, alignment: .trailing) {
                Text(LedgerMoney.amount(SetupFigures.total(of: store.wallets)))
                    .font(Face.row.weight(.semibold))
                    .foregroundStyle(Ink.text)
            }
            GridCell(width: SetupColumn.action) { EmptyView() }
        }
    }

    // MARK: - An active wallet

    @ViewBuilder
    private func activeRow(_ wallet: WalletView) -> some View {
        let isEditing = rows.row == wallet.id
        SetupRow(highlighted: isEditing || hovered == wallet.id, editing: isEditing) {
            if isEditing {
                GridCell {
                    SetupTextField(
                        placeholder: "", text: $rows.draft.name,
                        focus: $focus, key: WalletCell(row: wallet.id, field: .name), submit: commitFocusedLine
                    )
                }
                .frame(minWidth: WalletColumn.nameMinimum)
                balanceCell(wallet)
                GridCell(width: SetupColumn.action) { EmptyView() }
            } else {
                nameCell(wallet)
                    .onTapGesture { open(wallet) }
                balanceCell(wallet)
                    .onTapGesture { open(wallet) }
                // The core refuses a wallet that still holds money, and the
                // refusal comes through the store's alert.
                SetupArchiveCell(name: wallet.name, shown: hovered == wallet.id && store.canWrite) {
                    Task { await store.archiveWallet(wallet.id) }
                }
            }
        }
        .hoverId($hovered, wallet.id)
        .onEscape(when: isEditing) { rows.cancel($focus) }
        .contextMenu {
            if store.canWrite {
                Button(String(localized: "Archive"), role: .destructive) {
                    Task { await store.archiveWallet(wallet.id) }
                }
            }
        }
    }

    /// An archived wallet's name is `dimmed` and struck through.
    private func nameCell(_ wallet: WalletView, dimmed: Bool = false) -> some View {
        GridCell {
            Text(wallet.name).font(Face.row).foregroundStyle(dimmed ? Ink.text3 : Ink.text).strikethrough(dimmed)
        }
        .frame(minWidth: WalletColumn.nameMinimum)
    }

    /// A wallet may go negative (a card), and then it reads like any other
    /// negative amount; a zero reads as background, and so does every
    /// `dimmed` balance.
    private func balanceCell(_ wallet: WalletView, dimmed: Bool = false) -> some View {
        GridCell(width: WalletColumn.balance, alignment: .trailing) {
            Text(LedgerMoney.amount(wallet.balance))
                .font(Face.row)
                .foregroundStyle(
                    dimmed || wallet.balance == 0 ? Ink.text3 : wallet.balance < 0 ? Ink.negative : Ink.text
                )
        }
    }

    private var archived: some View {
        ForEach(store.archivedWallets.filter { matchesFilter($0.name) }, id: \.id) { wallet in
            SetupArchivedRow(name: wallet.name, id: wallet.id, hovered: $hovered, canWrite: store.canWrite) {
                Task { await store.restoreWallet(wallet.id) }
            } cells: {
                nameCell(wallet, dimmed: true)
                balanceCell(wallet, dimmed: true)
            }
        }
    }

    // MARK: - The empty line

    /// Always under the active wallets: a name, an opening balance (negative
    /// for a card that starts in debt) and ↩.
    private var newLineRow: some View {
        SetupRow(highlighted: focus?.row == nil && focus != nil, editing: focus?.row == nil && focus != nil) {
            GridCell {
                SetupTextField(
                    placeholder: String(localized: "New wallet…"), text: $rows.newLine.name,
                    focus: $focus, key: WalletCell(row: nil, field: .name), submit: commitFocusedLine
                )
            }
            .frame(minWidth: WalletColumn.nameMinimum)
            GridCell(width: WalletColumn.balance, alignment: .trailing) {
                SetupTextField(
                    placeholder: String(localized: "opening balance"), text: $rows.newLine.opening, alignment: .trailing,
                    focus: $focus, key: WalletCell(row: nil, field: .opening), submit: commitFocusedLine
                )
            }
            GridCell(width: SetupColumn.action) { EmptyView() }
        }
        .onEscape { rows.cancelNewLine($focus) }
    }

    // MARK: - Actions

    /// Opens `wallet` with the caret on NOME, saving whatever row was open.
    private func open(_ wallet: WalletView) {
        guard store.canWrite else { return }
        if let other = rows.other(than: wallet.id) { commit(other) }
        rows.open(wallet.id) { WalletDraft(wallet: wallet) }
        focus = WalletCell(row: wallet.id, field: .name)
    }

    private func commitFocusedLine() {
        rows.submit($focus, commit: commit) { Task { await commitNewLine() } }
    }

    /// Closes the row and sends the new name, if there is one; a row always
    /// closes. A name the core refuses (a duplicate) comes back through the
    /// store's alert.
    @discardableResult
    private func commit(_ walletId: Uuid) -> Bool {
        guard rows.row == walletId else { return true }
        rows.row = nil
        guard let wallet = store.wallets.first(where: { $0.id == walletId }),
              let name = rows.draft.rename(against: wallet)
        else { return true }
        Task { await store.renameWallet(walletId, name: name) }
        return true
    }

    /// ↩ on the empty line. The line is cleared only when a wallet actually
    /// appeared, so a refused name keeps its text and can be fixed.
    private func commitNewLine() async {
        do {
            guard let entry = try rows.newLine.wallet(currency: store.currency) else { return }
            if await store.createWallet(name: entry.name, openingBalance: entry.opening) {
                rows.clearNewLine()
            }
            focus = WalletCell(row: nil, field: .name)
        } catch {
            // Only SALDO is parsed, so a refusal is always about it.
            store.report(error)
            focus = WalletCell(row: nil, field: .opening)
        }
    }
}

/// A text cell of the wallet table. `opening` is the empty line's SALDO and
/// exists nowhere else.
typealias WalletCell = SetupFocus<WalletField>

enum WalletField: Hashable {
    case name
    case opening
}

// MARK: - The draft behind an edited line

/// The text in the cells of the wallet line being edited.
struct WalletDraft: Equatable {
    var name = ""
    /// Only the empty line has one: the opening balance.
    var opening = ""

    init() {}

    init(wallet: WalletView) {
        name = wallet.name
    }

    /// The new name, or `nil` when NOME is unchanged or emptied: clearing the
    /// cell is not a way to rename a wallet to nothing.
    func rename(against wallet: WalletView) -> String? {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty || trimmed == wallet.name ? nil : trimmed
    }

    /// The fields of a new wallet, parsed. `nil` when NOME is still blank: ↩ on
    /// an empty line is not an error, it is nothing. An empty SALDO is zero.
    func wallet(currency: Currency) throws -> (name: String, opening: Int64)? {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return (name: trimmed, opening: try LedgerMoney.cell(opening, currency: currency) ?? 0)
    }
}
