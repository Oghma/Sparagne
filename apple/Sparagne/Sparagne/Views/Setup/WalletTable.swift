import SwiftUI
import SparagneCore

/// Column geometry of the wallet table, as the canvas
/// has it: NOME takes what is left, SALDO 140 and the action column 36.
private enum WalletColumn {
    static let nameMinimum: CGFloat = 100
    static let balance: CGFloat = 140
}

/// The wallets of the vault as an editable table. The
/// wallets say where the money is and the envelopes below say what it is for.
/// NOME edits in place (`RenameWallet`). SALDO comes from the transactions, so
/// it is only typed on the empty line, where it is the opening balance.
///
/// Editing works as in `EnvelopeTable`: clicking a row opens it, ↩ commits,
/// esc throws the draft away and leaving the row saves it (`editsRows`).
struct WalletTable: View {
    @Bindable var store: AppStore

    /// Which wallet is open for editing; the empty line is `nil` and lives in
    /// `newLine` instead, so the two drafts never overwrite each other.
    @State private var editing: Uuid?
    @State private var draft = WalletDraft()
    @State private var newLine = WalletDraft()
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
        .editsRows(editing: $editing, focus: $focus, hovered: hovered, store: store) {
            newLine = WalletDraft()
        } commit: { commit($0) }
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
            SetupCell { Text(String(localized: "Name")) }
                .frame(minWidth: WalletColumn.nameMinimum)
            SetupCell(width: WalletColumn.balance, alignment: .trailing) {
                Text(String(localized: "Balance"))
            }
            SetupCell(width: SetupColumn.action) { EmptyView() }
        }
    }

    /// What all the wallets hold together, which is also what the envelopes
    /// add up to.
    private var totalRow: some View {
        SetupRow(separator: false, topRule: true) {
            SetupCell {
                Text(String(localized: "Total")).font(Face.row.weight(.semibold)).foregroundStyle(Ink.text)
            }
            .frame(minWidth: WalletColumn.nameMinimum)
            SetupCell(width: WalletColumn.balance, alignment: .trailing) {
                Text(LedgerMoney.amount(SetupFigures.total(of: store.wallets)))
                    .font(Face.row.weight(.semibold))
                    .foregroundStyle(Ink.text)
            }
            SetupCell(width: SetupColumn.action) { EmptyView() }
        }
    }

    // MARK: - An active wallet

    @ViewBuilder
    private func activeRow(_ wallet: WalletView) -> some View {
        let isEditing = editing == wallet.id
        SetupRow(highlighted: isEditing || hovered == wallet.id, editing: isEditing) {
            if isEditing {
                SetupCell {
                    SetupTextField(
                        placeholder: "", text: $draft.name,
                        focus: $focus, key: WalletCell(row: wallet.id, field: .name), submit: commitFocusedLine
                    )
                }
                .frame(minWidth: WalletColumn.nameMinimum)
                balanceCell(wallet)
                SetupCell(width: SetupColumn.action) { EmptyView() }
            } else {
                SetupCell { Text(wallet.name).font(Face.row).foregroundStyle(Ink.text) }
                    .frame(minWidth: WalletColumn.nameMinimum)
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
        .onEscape(when: isEditing, perform: cancel)
        .contextMenu {
            if store.canWrite {
                Button(String(localized: "Archive"), role: .destructive) {
                    Task { await store.archiveWallet(wallet.id) }
                }
            }
        }
    }

    /// A wallet may go negative (a card), and then it reads like any other
    /// negative amount; a zero reads as background.
    private func balanceCell(_ wallet: WalletView) -> some View {
        SetupCell(width: WalletColumn.balance, alignment: .trailing) {
            Text(LedgerMoney.amount(wallet.balance))
                .font(Face.row)
                .foregroundStyle(wallet.balance == 0 ? Ink.text3 : wallet.balance < 0 ? Ink.negative : Ink.text)
        }
    }

    private var archived: some View {
        ForEach(store.archivedWallets.filter { matchesFilter($0.name) }, id: \.id) { wallet in
            SetupArchivedRow(name: wallet.name, id: wallet.id, hovered: $hovered, canWrite: store.canWrite) {
                Task { await store.restoreWallet(wallet.id) }
            } cells: {
                SetupCell {
                    Text(wallet.name).font(Face.row).foregroundStyle(Ink.text3).strikethrough()
                }
                .frame(minWidth: WalletColumn.nameMinimum)
                SetupCell(width: WalletColumn.balance, alignment: .trailing) {
                    Text(LedgerMoney.amount(wallet.balance)).font(Face.row).foregroundStyle(Ink.text3)
                }
            }
        }
    }

    // MARK: - The empty line

    /// Always under the active wallets: a name, an opening balance (negative
    /// for a card that starts in debt) and ↩.
    private var newLineRow: some View {
        SetupRow(highlighted: focus?.row == nil && focus != nil, editing: focus?.row == nil && focus != nil) {
            SetupCell {
                SetupTextField(
                    placeholder: String(localized: "New wallet…"), text: $newLine.name,
                    focus: $focus, key: WalletCell(row: nil, field: .name), submit: commitFocusedLine
                )
            }
            .frame(minWidth: WalletColumn.nameMinimum)
            SetupCell(width: WalletColumn.balance, alignment: .trailing) {
                SetupTextField(
                    placeholder: String(localized: "opening balance"), text: $newLine.opening, alignment: .trailing,
                    focus: $focus, key: WalletCell(row: nil, field: .opening), submit: commitFocusedLine
                )
            }
            SetupCell(width: SetupColumn.action) { EmptyView() }
        }
        .onEscape {
            newLine = WalletDraft()
            focus = nil
        }
    }

    // MARK: - Actions

    /// Opens `wallet` with the caret on NOME, saving whatever row was open.
    private func open(_ wallet: WalletView) {
        guard store.canWrite else { return }
        if editing != wallet.id {
            if let editing { commit(editing) }
            editing = wallet.id
            draft = WalletDraft(wallet: wallet)
        }
        focus = WalletCell(row: wallet.id, field: .name)
    }

    private func cancel() {
        editing = nil
        focus = nil
    }

    private func commitFocusedLine() {
        if let row = focus?.row {
            commit(row)
            focus = nil
        } else {
            Task { await commitNewLine() }
        }
    }

    /// Closes the row and sends the new name, if there is one. A name the
    /// core refuses (a duplicate) comes back through the store's alert.
    private func commit(_ walletId: Uuid) {
        guard editing == walletId else { return }
        editing = nil
        guard let wallet = store.wallets.first(where: { $0.id == walletId }),
              let name = draft.rename(against: wallet)
        else { return }
        Task { await store.renameWallet(walletId, name: name) }
    }

    /// ↩ on the empty line. The line is cleared only when a wallet actually
    /// appeared, so a refused name keeps its text and can be fixed.
    private func commitNewLine() async {
        do {
            guard let entry = try newLine.wallet(currency: store.currency) else { return }
            let before = Set(store.wallets.map(\.id))
            await store.createWallet(name: entry.name, openingBalance: entry.opening)
            if store.wallets.contains(where: { !before.contains($0.id) }) {
                newLine = WalletDraft()
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

/// The text in the cells of the line being edited. A plain struct with no view
/// in it, so the diff and the parsing are testable on their own.
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
        let amount = opening.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !amount.isEmpty else { return (name: trimmed, opening: 0) }
        return (name: trimmed, opening: try parseMoney(text: amount, currency: currency))
    }
}
