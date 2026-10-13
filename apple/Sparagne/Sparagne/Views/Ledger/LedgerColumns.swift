import AppKit
import SwiftUI
import SparagneCore

/// A column of the ledger whose width can be dragged in the header, as a
/// spreadsheet's can. DESCRIZIONE is not one: it takes whatever the others
/// leave, so dragging any of them gives to it or takes from it. # is not one
/// either: it only ever holds a short number.
enum LedgerColumn: String, CaseIterable, Sendable {
    case date
    case flow
    case category
    case wallet
    case person
    case amount

    /// The narrowest a drag leaves a column, inside its padding: a few
    /// letters, enough to find it again.
    static let minimum: CGFloat = 24
    /// The widest: past this a name is no longer a cell, it is a paragraph.
    static let maximum: CGFloat = 480
    /// What DESCRIZIONE keeps whatever the other columns take.
    static let noteMinimum: CGFloat = 120

    /// The width the column opens at, inside its padding: the canvas's, and
    /// what Reset Column Widths goes back to.
    var standard: CGFloat {
        let outer: CGFloat = switch self {
        case .date: 60
        case .flow: 84
        case .category: 124
        case .wallet: 84
        case .person: 84
        case .amount: 96
        }
        return outer - 2 * GridColumn.padding
    }

    /// The columns after DESCRIZIONE hang from the right edge, so each one's
    /// handle is its left edge and a drag to the left widens it: whichever
    /// side the column is on, the edge follows the pointer.
    var growsLeft: Bool {
        switch self {
        case .wallet, .person, .amount: true
        case .date, .flow, .category: false
        }
    }

    var field: RowField {
        switch self {
        case .date: .date
        case .flow: .flow
        case .category: .category
        case .wallet: .wallet
        case .person: .person
        case .amount: .amount
        }
    }

    static func clamp(_ width: CGFloat) -> CGFloat {
        min(max(width.rounded(), minimum), maximum)
    }
}

/// The ledger's column widths: the standard ones, and those a drag or a
/// double click changed. Only the changed ones are saved, so a column never
/// touched follows the standard if it ever changes.
struct LedgerColumnWidths: Equatable, Sendable {
    private var custom: [LedgerColumn: CGFloat] = [:]

    init() {}

    /// What `stored` wrote; anything unreadable is the standard.
    init(stored: [String: Any]?) {
        for (key, value) in stored ?? [:] {
            guard let column = LedgerColumn(rawValue: key), let width = (value as? NSNumber)?.doubleValue else {
                continue
            }
            self[column] = width
        }
    }

    /// Inside the column's padding, as `GridCell` takes it.
    subscript(column: LedgerColumn) -> CGFloat {
        get { custom[column] ?? column.standard }
        set {
            let width = LedgerColumn.clamp(newValue)
            custom[column] = width == column.standard ? nil : width
        }
    }

    /// The column with its padding on both sides: how far apart its edges are.
    func outer(_ column: LedgerColumn) -> CGFloat {
        self[column] + 2 * GridColumn.padding
    }

    var isStandard: Bool { custom.isEmpty }

    /// For `UserDefaults`.
    var stored: [String: Double] {
        Dictionary(uniqueKeysWithValues: custom.map { ($0.key.rawValue, Double($0.value)) })
    }
}

// MARK: - Fitting a column to what it holds

/// The width a double click on a column's edge gives it: enough for the
/// widest thing the column shows this month and for its heading, as a
/// spreadsheet fits a column to its contents.
///
/// Measured with AppKit's fonts, the same faces `Face` draws with.
enum LedgerColumnFit {
    private static let rowFont = NSFont.monospacedDigitSystemFont(ofSize: 12, weight: .regular)
    private static let smallFont = NSFont.monospacedDigitSystemFont(ofSize: 10.5, weight: .regular)
    private static let labelFont = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .medium)
    /// The ENVELOPE cell's select mark and the gap before it, drawn while a
    /// row is open (`EnvelopeSelectCell`).
    private static let selectMark: CGFloat = 12
    /// A point of air either side, so rounding never truncates the widest.
    private static let slack: CGFloat = 2

    static func width(of text: String, font: NSFont) -> CGFloat {
        ceil((text as NSString).size(withAttributes: [.font: font]).width)
    }

    /// `rows` and `due` are the month on screen; the empty line counts too,
    /// with the author in PERSONA and a zero in IMPORTO.
    static func width(
        of column: LedgerColumn,
        rows: [TransactionRow],
        due: [DuePeriod],
        store: AppStore
    ) -> CGFloat {
        let heading = width(of: column.field.label, font: labelFont)
        let content: [CGFloat] = switch column {
        case .date:
            (rows.map(\.occurredAt) + due.compactMap { CoreDate.localDay($0.date) }).map(day)
        case .flow:
            (rows.map(\.envelopeColumn) + due.map { store.envelopeName($0.template.flowId) })
                .map { width(of: $0, font: rowFont) + selectMark }
        case .category:
            (rows.map(\.category) + due.map { category($0.template) }).map { width(of: $0, font: rowFont) }
        case .wallet:
            (rows.map(\.walletDisplay) + due.map { store.walletName($0.template.walletId) })
                .map { width(of: $0, font: rowFont) }
        case .person:
            (rows.map(\.person) + due.map(\.template.owner) + [store.currentAuthor])
                .map { width(of: $0, font: rowFont) }
        case .amount:
            (rows.map(LedgerRowView.amountText) + due.map { LedgerMoney.bare($0.template.amount) } + [LedgerMoney.bare(0)])
                .map { width(of: $0, font: rowFont) }
        }
        return LedgerColumn.clamp(max(heading, content.max() ?? 0) + slack)
    }

    /// `DayLabel`: the day, four points, the weekday smaller.
    private static func day(_ date: Date) -> CGFloat {
        width(of: LedgerDate.dayNumber(date), font: rowFont) + 4 + width(of: LedgerDate.weekday(date), font: smallFont)
    }

    /// As `PendingRowView` writes a template's category.
    private static func category(_ template: RecurringView) -> String {
        guard let category = template.category, !category.isEmpty else { return String(localized: "Uncategorized") }
        return category
    }
}

// MARK: - The handles in the header

/// The edges a column can be dragged by, laid over the header: one after each
/// column before DESCRIZIONE, one before each column after it, each centred
/// on the line between two columns. Laid over the header rather than inside
/// its cells, so no neighbouring cell sits on top of half a handle.
///
/// Drag one to resize, double click it to fit the column to what it holds.
/// The widths are saved when the drag ends.
struct ColumnResizeHandles: View {
    let store: AppStore
    let showsWallet: Bool

    /// The header's width, for how far a column may grow before DESCRIZIONE
    /// would drop under its minimum.
    @State private var headerWidth: CGFloat = 0

    var body: some View {
        let widths = store.ledgerColumns
        ZStack(alignment: .topLeading) {
            ForEach(leading, id: \.self) { column in
                ColumnResizeHandle(column: column, store: store, room: room)
                    .offset(x: edge(of: column, in: widths) - ColumnResizeHandle.width / 2)
            }
            ForEach(trailing, id: \.self) { column in
                ColumnResizeHandle(column: column, store: store, room: room)
                    .offset(x: headerWidth - edge(of: column, in: widths) - ColumnResizeHandle.width / 2)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { headerWidth = $0 }
    }

    private var leading: [LedgerColumn] { [.date, .flow, .category] }

    private var trailing: [LedgerColumn] {
        showsWallet ? [.amount, .person, .wallet] : [.amount, .person]
    }

    /// Where a column's handle sits: from the left edge for a column before
    /// DESCRIZIONE, from the right edge for one after it.
    private func edge(of column: LedgerColumn, in widths: LedgerColumnWidths) -> CGFloat {
        if column.growsLeft {
            let order = trailing
            let index = order.firstIndex(of: column) ?? 0
            return GridColumn.action + order[...index].reduce(0) { $0 + widths.outer($1) }
        }
        let order = leading
        let index = order.firstIndex(of: column) ?? 0
        let ordinal = GridColumn.ordinal + 2 * GridColumn.padding
        return ordinal + order[...index].reduce(0) { $0 + widths.outer($1) }
    }

    /// How many points the columns may still take from DESCRIZIONE.
    private func room() -> CGFloat {
        let widths = store.ledgerColumns
        let shown = leading + trailing
        let taken = GridColumn.ordinal + 2 * GridColumn.padding + GridColumn.action
            + shown.reduce(0) { $0 + widths.outer($1) }
        return max(0, headerWidth - taken - LedgerColumn.noteMinimum - 2 * GridColumn.padding)
    }
}

/// One column's edge: the resize pointer over it, a hairline while the
/// pointer is there, an accent line while it is being dragged.
private struct ColumnResizeHandle: View {
    static let width: CGFloat = 9

    let column: LedgerColumn
    let store: AppStore
    /// How far the column may grow, asked when a drag starts.
    let room: () -> CGFloat

    @State private var hovering = false
    /// The width and the room when the drag started: the drag's translation
    /// is measured from there, not from the last step.
    @State private var start: (width: CGFloat, room: CGFloat)?

    var body: some View {
        Rectangle()
            .fill(start != nil ? Ink.accent : (hovering ? Ink.line2 : Color.clear))
            .frame(width: start != nil ? 2 : 1)
            .frame(width: Self.width, height: Metrics.headerHeight)
            .contentShape(Rectangle())
            .pointerStyle(.columnResize)
            .onHover { hovering = $0 }
            .onTapGesture(count: 2) { fit() }
            .gesture(
                DragGesture(minimumDistance: 1, coordinateSpace: .global)
                    .onChanged { value in
                        let origin = start ?? (store.ledgerColumns[column], room())
                        start = origin
                        let delta = column.growsLeft ? -value.translation.width : value.translation.width
                        store.ledgerColumns[column] = min(origin.width + delta, origin.width + origin.room)
                    }
                    .onEnded { _ in
                        start = nil
                        store.saveLedgerColumns()
                    }
            )
            // VoiceOver cannot drag: it adjusts the width a step at a time.
            .accessibilityElement()
            .accessibilityLabel(String(localized: "\(column.field.label) column width"))
            .accessibilityValue(Text(store.ledgerColumns[column], format: .number.precision(.fractionLength(0))))
            .accessibilityAdjustableAction { direction in
                let step: CGFloat = direction == .increment ? 8 : -8
                let current = store.ledgerColumns[column]
                store.ledgerColumns[column] = min(current + step, current + room())
                store.saveLedgerColumns()
            }
    }

    private func fit() {
        let current = store.ledgerColumns[column]
        let fitted = LedgerColumnFit.width(of: column, rows: store.rows, due: store.duePeriods, store: store)
        store.ledgerColumns[column] = min(fitted, current + room())
        store.saveLedgerColumns()
    }
}
