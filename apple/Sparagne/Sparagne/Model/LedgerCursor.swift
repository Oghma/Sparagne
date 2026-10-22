import Foundation
import SparagneCore

/// A cell: which row (nil = the new line) and which field.
struct CellFocus: Hashable, Sendable {
    let row: Uuid?
    let field: RowField
}

/// The cells a row is typed into, in column order, which is the order ⇥ walks.
enum RowField: Hashable, CaseIterable, Sendable {
    case date
    case flow
    case category
    case note
    /// Only in the hierarchy while the optional WALLET column is on.
    case wallet
    /// Who the row is for: one of `AppStore.assignablePeople`. Not a field on
    /// a transfer, which is always its author's.
    case person
    case amount

    /// The column heading, which is what VoiceOver calls the cell: an open
    /// row's fields have no placeholder to be read instead.
    var label: String {
        switch self {
        case .date: String(localized: "Date")
        case .flow: String(localized: "Envelope")
        case .category: String(localized: "Category")
        case .note: String(localized: "Description")
        case .wallet: String(localized: "Wallet")
        case .person: String(localized: "Person")
        case .amount: String(localized: "Amount")
        }
    }
}

/// A key that moves the grid's cursor from one cell to another.
enum CursorMove: Hashable, Sendable {
    /// ↑ and ↓: the same column, one row up or down.
    case up, down
    /// ← and →, while the cell is not being typed into.
    case left, right
    /// ⇥ and ⇧⇥: the next cell or the previous one, wrapping to the next row
    /// or the previous one at the ends of a line, as a table in a
    /// spreadsheet does.
    case tab, backTab
}

/// Where the grid's cursor can go and which cells take typing, with no SwiftUI in it.
struct LedgerCursor {
    // The lines are the stored rows, then the new line when the vault can be written; the
    // due periods among them are not lines, and ↑ ↓ pass over them.

    /// The stored rows on screen, top to bottom.
    let rows: [TransactionRow]
    /// The vault on screen can be written to: the new line is there, and the
    /// cells of a live row can be typed into.
    let canWrite: Bool
    /// The optional WALLET column is on.
    let showsWallet: Bool

    /// The columns of a line, left to right: WALLET only while it is shown.
    var fields: [RowField] {
        RowField.allCases.filter { $0 != .wallet || showsWallet }
    }

    /// The cell `move` lands on from `cell`. At an edge the cursor stays
    /// where it is: ↑ on the first row, ← on DATA, ⇧⇥ on the first row's
    /// DATA. ⇥ from IMPORTO goes to DATA of the next line; from the last
    /// stored row that is the new line, and from the new line it is the new
    /// line's own DATA, which by then is a blank line again (`LedgerEditor`).
    func target(of move: CursorMove, from cell: CellFocus) -> CellFocus {
        let lines = self.lines
        let fields = self.fields
        // A cell on a row no longer on screen, or in a column hidden since,
        // has nowhere to go from: the editor drops it (`rowsChanged`).
        guard let line = lines.firstIndex(of: cell.row), let column = fields.firstIndex(of: cell.field) else {
            return cell
        }
        switch move {
        case .up:
            guard line > 0 else { return cell }
            return CellFocus(row: lines[line - 1], field: cell.field)
        case .down:
            guard line < lines.count - 1 else { return cell }
            return CellFocus(row: lines[line + 1], field: cell.field)
        case .left:
            guard column > 0 else { return cell }
            return CellFocus(row: cell.row, field: fields[column - 1])
        case .right:
            guard column < fields.count - 1 else { return cell }
            return CellFocus(row: cell.row, field: fields[column + 1])
        case .tab:
            if column < fields.count - 1 { return CellFocus(row: cell.row, field: fields[column + 1]) }
            if line < lines.count - 1 { return CellFocus(row: lines[line + 1], field: fields[0]) }
            // The new line wraps onto itself: by the time the cursor lands,
            // what it said has been registered and it is blank again. The
            // last row of a vault only read has no line after it.
            return cell.row == nil ? CellFocus(row: nil, field: fields[0]) : cell
        case .backTab:
            if column > 0 { return CellFocus(row: cell.row, field: fields[column - 1]) }
            guard line > 0 else { return cell }
            return CellFocus(row: lines[line - 1], field: fields[fields.count - 1])
        }
    }

    /// The lines the cursor walks, top to bottom: each stored row by its id,
    /// then `nil`, the new line, when there is one.
    private var lines: [Uuid?] {
        rows.map { Optional($0.id) } + (canWrite ? [nil] : [])
    }

    /// Whether `cell` can be typed into. Any cell can be selected; typing
    /// into one that cannot is a beep.
    ///
    /// Nothing can be typed into a vault only read, nor into a deleted row.
    /// On a transfer only DATA, DESCRIZIONE and IMPORTO can, and the cell
    /// that shows its route opens its two ends instead (`opensEnds`). The
    /// core takes no category and no person on a transfer.
    func isEditable(_ cell: CellFocus) -> Bool {
        guard canWrite, fields.contains(cell.field) else { return false }
        // The new line: every cell it shows.
        guard let id = cell.row else { return true }
        guard let row = rows.first(where: { $0.id == id }), !row.voided else { return false }
        guard row.isTransfer else { return true }
        return Self.transferFields.contains(cell.field) || cell.field == Self.routeField(of: row.kind)
    }

    /// Whether ↩ or a double click on `cell` opens the "Da → A" popover of
    /// its transfer: the route cell of a live one, which is never typed into.
    func opensEnds(_ cell: CellFocus) -> Bool {
        guard isEditable(cell), let id = cell.row, let row = rows.first(where: { $0.id == id }) else { return false }
        return cell.field == Self.routeField(of: row.kind)
    }

    /// The cell of the column a transfer's ends belong to: ENVELOPE for one
    /// between envelopes, WALLET for one between wallets, which opens only
    /// while that column is shown. `nil` for an entry.
    static func routeField(of kind: TransactionKind) -> RowField? {
        switch kind {
        case .transferFlow: .flow
        case .transferWallet: .wallet
        case .income, .expense, .refund: nil
        }
    }

    /// What can be typed into on a transfer besides its route.
    private static let transferFields: Set<RowField> = [.date, .note, .amount]
}
