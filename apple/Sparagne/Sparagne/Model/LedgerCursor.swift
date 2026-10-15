import Foundation
import SparagneCore

/// A cell: which row (nil = the new line) and which field.
struct CellFocus: Hashable, Sendable {
    let row: Uuid?
    let field: RowField
}

/// The cells a row is typed into, in the order the columns are laid out,
/// which is also the order ⇥ walks them in (`LedgerCursor`).
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

/// Where the grid's cursor can go, and which cells can be typed into: a
/// value with no SwiftUI in it, so the moves are tested without a window
/// (`SparagneTests/LedgerCursorTests.swift`).
///
/// The lines are the stored rows on screen, in their order, then the new
/// line when the vault can be written to. The recurring periods waiting
/// among the rows are not lines here: they are not rows, and ↑ ↓ pass over
/// them.
struct LedgerCursor {
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
        // A stub for now.
        cell
    }

    /// Whether `cell` can be typed into. Any cell can be selected; typing
    /// into one that cannot is a beep.
    ///
    /// Nothing can be typed into a vault only read, nor into a deleted row.
    /// On a transfer, ENVELOPE, WALLET and PERSONA are its two ends and its
    /// author, so only DATA, CATEGORIA, DESCRIZIONE and IMPORTO can.
    func isEditable(_ cell: CellFocus) -> Bool {
        // A stub for now.
        false
    }
}
