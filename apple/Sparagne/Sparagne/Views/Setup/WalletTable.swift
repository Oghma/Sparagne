import SwiftUI
import SparagneCore

/// Column geometry of the wallet table. It sits above the envelope table in
/// the setup tab's left column (`docs/v2/UI.md` §2.3) and has the same width,
/// so SALDO lines up with the envelopes' SALDO. NOME takes what is left.
private enum WalletColumn {
    static let nameMinimum: CGFloat = 140
    static let balance: CGFloat = 104
    /// The archive/restore icon, empty until the pointer is over the row.
    static let action: CGFloat = 28
}

/// The wallets of the vault as an editable table (`docs/v2/UI.md` §2.3). The
/// wallets say where the money is and the envelopes below say what it is for.
/// NOME edits in place (`RenameWallet`). SALDO comes from the transactions, so
/// it is only typed on the empty line, where it is the opening balance.
///
/// Editing works as in `EnvelopeTable`: clicking a row opens it, ↩ commits,
/// esc throws the draft away and leaving the row saves it.
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
        Panel(padding: 0) {
            VStack(spacing: 0) {
                title
                Hairline()
                header
                Hairline()
                // No scroll view: a vault has a handful of wallets, and the
                // table takes only the height they need so the envelopes
                // below keep the rest (`SetupView`).
                ForEach(store.wallets, id: \.id) { wallet in
                    activeRow(wallet)
                    Hairline()
                }
                // A viewer reads the wallets; nothing to add.
                if store.canWrite { newLineRow }
                archived
                if store.canWrite {
                    Hairline()
                    footnote
                }
            }
        }
        // A different vault means different wallets: whatever was half typed
        // belongs to the vault that is gone.
        .onChange(of: store.currentVault?.id) { _, _ in
            editing = nil
            focus = nil
            newLine = WalletDraft()
        }
        .onChange(of: focus) { old, new in focusMoved(from: old, to: new) }
        // Turned read-only under an open row: it could not be saved.
        .onChange(of: store.isReadOnly) { _, _ in cancel() }
    }

    // MARK: - Chrome

    private var title: some View {
        HStack {
            SectionLabel(text: String(localized: "Wallets"))
            Spacer()
        }
        .padding(.horizontal, GridColumn.padding)
        .frame(height: 24)
    }

    /// Where an opening balance goes, which is what ties the wallets to the
    /// envelopes: Σ wallets = Σ envelopes (`docs/v2/DISTILLATO_V1.md` §1.1).
    private var footnote: some View {
        Text(String(localized: "The opening balance of a new wallet goes to Unallocated"))
            .font(Face.footnote)
            .foregroundStyle(Ink.dim)
            .padding(.horizontal, GridColumn.padding)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var header: some View {
        HStack(spacing: 0) {
            GridCell { SectionLabel(text: String(localized: "Name")) }
                .frame(minWidth: WalletColumn.nameMinimum)
            GridCell(width: WalletColumn.balance, alignment: .trailing) {
                SectionLabel(text: String(localized: "Balance"))
            }
            GridCell(width: WalletColumn.action) { EmptyView() }
        }
        .frame(height: 24)
    }

    // MARK: - An active wallet

    @ViewBuilder
    private func activeRow(_ wallet: WalletView) -> some View {
        let isEditing = editing == wallet.id
        HStack(spacing: 0) {
            if isEditing {
                GridCell {
                    textField($draft.name, key: WalletCell(row: wallet.id, field: .name), placeholder: "")
                }
                .frame(minWidth: WalletColumn.nameMinimum)
                balanceCell(wallet)
                GridCell(width: WalletColumn.action) { EmptyView() }
            } else {
                GridCell { Text(wallet.name).font(Face.row).foregroundStyle(Ink.text) }
                    .frame(minWidth: WalletColumn.nameMinimum)
                    .onTapGesture { open(wallet) }
                balanceCell(wallet)
                    .onTapGesture { open(wallet) }
                actionCell(wallet)
            }
        }
        .frame(height: Metrics.rowHeight)
        .contentShape(Rectangle())
        .background(isEditing || hovered == wallet.id ? Ink.raised : Color.clear)
        .overlay(alignment: .leading) {
            if isEditing { Rectangle().fill(Ink.accent).frame(width: 2) }
        }
        .onHover { inside in hover(wallet.id, inside) }
        .onKeyPress(.escape) {
            guard isEditing else { return .ignored }
            cancel()
            return .handled
        }
        .contextMenu {
            if store.canWrite {
                Button(String(localized: "Archive"), role: .destructive) {
                    Task { await store.archiveWallet(wallet.id) }
                }
            }
        }
    }

    /// The archive icon, only for the row under the pointer: the same
    /// `ArchiveWallet` the context menu sends. The core refuses a wallet that
    /// still holds money, and the refusal comes through the store's alert.
    private func actionCell(_ wallet: WalletView) -> some View {
        GridCell(width: WalletColumn.action) {
            if hovered == wallet.id, store.canWrite {
                Button {
                    Task { await store.archiveWallet(wallet.id) }
                } label: {
                    Image(systemName: "archivebox")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(Ink.dim)
                }
                .buttonStyle(.plain)
                .help(String(localized: "Archive"))
            }
        }
    }

    /// A wallet may go negative (a card), and then it reads like any other
    /// negative amount; a zero reads as background (`docs/v2/UI.md` §5).
    private func balanceCell(_ wallet: WalletView) -> some View {
        GridCell(width: WalletColumn.balance, alignment: .trailing) {
            Text(LedgerMoney.amount(wallet.balance))
                .font(Face.row)
                .foregroundStyle(wallet.balance == 0 ? Ink.dim : wallet.balance < 0 ? Ink.negative : Ink.text)
        }
    }

    // MARK: - The archived wallets

    @ViewBuilder
    private var archived: some View {
        if !store.archivedWallets.isEmpty {
            Hairline()
            HStack(spacing: 0) {
                GridCell { SectionLabel(text: String(localized: "Archived")) }
                Spacer(minLength: 0)
            }
            .frame(height: 22)
            ForEach(store.archivedWallets, id: \.id) { wallet in
                Hairline()
                HStack(spacing: 0) {
                    GridCell {
                        Text(wallet.name)
                            .font(Face.row)
                            .foregroundStyle(Ink.dim)
                            .strikethrough()
                    }
                    .frame(minWidth: WalletColumn.nameMinimum)
                    GridCell(width: WalletColumn.balance, alignment: .trailing) {
                        Text(LedgerMoney.amount(wallet.balance)).font(Face.row).foregroundStyle(Ink.dim)
                    }
                    GridCell(width: WalletColumn.action) {
                        if hovered == wallet.id, store.canWrite {
                            Button {
                                Task { await store.restoreWallet(wallet.id) }
                            } label: {
                                Image(systemName: "tray.and.arrow.up")
                                    .font(.system(size: 11, weight: .medium))
                                    .foregroundStyle(Ink.dim)
                            }
                            .buttonStyle(.plain)
                            .help(String(localized: "Restore"))
                        }
                    }
                }
                .frame(height: Metrics.rowHeight)
                .contentShape(Rectangle())
                .background(hovered == wallet.id ? Ink.raised : Color.clear)
                .onHover { inside in hover(wallet.id, inside) }
                .contextMenu {
                    if store.canWrite {
                        Button(String(localized: "Restore")) {
                            Task { await store.restoreWallet(wallet.id) }
                        }
                    }
                }
            }
        }
    }

    // MARK: - The empty line

    /// Always under the active wallets: a name, an opening balance (negative
    /// for a card that starts in debt) and ↩.
    private var newLineRow: some View {
        HStack(spacing: 0) {
            GridCell {
                textField(
                    $newLine.name,
                    key: WalletCell(row: nil, field: .name),
                    placeholder: String(localized: "name…")
                )
            }
            .frame(minWidth: WalletColumn.nameMinimum)
            GridCell(width: WalletColumn.balance, alignment: .trailing) {
                textField(
                    $newLine.opening,
                    key: WalletCell(row: nil, field: .opening),
                    placeholder: "0,00",
                    alignment: .trailing
                )
            }
            GridCell(width: WalletColumn.action) { EmptyView() }
        }
        .frame(height: Metrics.rowHeight)
        .background(focus?.row == nil && focus != nil ? Ink.raised : Color.clear)
        .overlay(alignment: .leading) {
            Rectangle().fill(Ink.accent).frame(width: 2)
        }
        .onKeyPress(.escape) {
            newLine = WalletDraft()
            focus = nil
            return .handled
        }
    }

    private func textField(
        _ text: Binding<String>,
        key: WalletCell,
        placeholder: String,
        alignment: TextAlignment = .leading
    ) -> some View {
        TextField(placeholder, text: text)
            .textFieldStyle(.plain)
            .font(Face.row)
            .multilineTextAlignment(alignment)
            .foregroundStyle(Ink.text)
            .focused($focus, equals: key)
            .onSubmit { commitFocusedLine() }
    }

    // MARK: - Actions

    private func hover(_ id: Uuid, _ inside: Bool) {
        if inside {
            hovered = id
        } else if hovered == id {
            hovered = nil
        }
    }

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

    /// esc: throw the draft away. `editing` is cleared before the focus, so
    /// the focus change that follows is not read as "left the row".
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

    /// Leaving the open row saves it, as in `EnvelopeTable.focusMoved`: ⇥ off
    /// NOME, a click on another row or on another table all write the name.
    ///
    /// The exception is the row's own context menu, which is attached while
    /// the row is open too: it resigns the text field without giving the
    /// focus to anything, and that would close the row under the pointer.
    /// The pointer tells the two apart, so a blur while hovering the open
    /// row is not a departure.
    private func focusMoved(from old: WalletCell?, to new: WalletCell?) {
        guard let editing else { return }
        if let new {
            if new.row != editing { commit(editing) }
        } else if old?.row == editing, hovered != editing {
            commit(editing)
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

// MARK: - Focus

/// A text cell of the wallet table: which row (nil = the empty line) and which
/// field.
struct WalletCell: Hashable {
    let row: Uuid?
    let field: WalletField
}

/// `opening` is the empty line's SALDO and exists nowhere else.
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
