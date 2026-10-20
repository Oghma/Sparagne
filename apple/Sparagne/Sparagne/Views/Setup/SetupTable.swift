import SwiftUI
import SparagneCore

// The frame the Setup tables share: a card with a title row, a header, rows
// of 24 pt with a hairline under each, the pieces the rows are made of and
// the one rule of editing them. The cell is local rather than the ledger's
// `GridCell`: this sheet pads cells by 8 and lets rows highlight with rounded
// corners, which the ledger's grid does not.

/// The width of the trailing column that holds the archive icon.
enum SetupColumn {
    static let action: CGFloat = 36
}

/// One cell: a fixed width (or the rest of the row), one line, 8 pt of
/// padding. The whole cell is the click target, like the ledger's.
struct SetupCell<Content: View>: View {
    var width: CGFloat?
    var alignment: Alignment = .leading
    @ViewBuilder var content: Content

    var body: some View {
        content
            .lineLimit(1)
            .truncationMode(.tail)
            .frame(width: width, alignment: alignment)
            .frame(maxWidth: width == nil ? .infinity : nil, alignment: alignment)
            .padding(.horizontal, Metrics.cellPad)
            .contentShape(Rectangle())
    }
}

/// A rounded card with a title, an optional hint in `text3` and an optional
/// control at the right of the title.
struct SetupCard<Accessory: View, Content: View>: View {
    let title: String
    var hint: String?
    @ViewBuilder var accessory: Accessory
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            CardHeading(title: title, hint: hint, bottomPadding: 4) { accessory }
            content
        }
        .headedCard()
        .accessibilityElement(children: .contain)
        .accessibilityLabel(title)
    }
}

extension SetupCard where Accessory == EmptyView {
    init(title: String, hint: String? = nil, @ViewBuilder content: () -> Content) {
        self.init(title: title, hint: hint, accessory: { EmptyView() }, content: content)
    }
}

/// The rows of a card's table, pulled 8 pt past the card's padding so the
/// rows' own padding lines their text up with the card's title and a
/// hovered row's rounded background sits 4 pt from the border.
struct SetupTableBody<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(spacing: 0) { content }
            .padding(.horizontal, -Metrics.cellPad)
    }
}

/// The column headings: 11 pt, medium, `text3`, over a `line2` rule.
/// `horizontalPadding` insets them as a table's rows are inset;
/// `accessibilityHeader` reads them to VoiceOver as one heading, for a
/// table whose card does not already have one.
struct SetupHeaderRow<Content: View>: View {
    var horizontalPadding: CGFloat = 0
    var accessibilityHeader = false
    @ViewBuilder var content: Content

    var body: some View {
        let row = HStack(spacing: 0) { content }
            .font(Face.label)
            .foregroundStyle(Ink.text3)
            .padding(.horizontal, horizontalPadding)
            .frame(height: Metrics.headerHeight)
            .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
        if accessibilityHeader {
            row
                .accessibilityElement(children: .combine)
                .accessibilityAddTraits(.isHeader)
        } else {
            row
        }
    }
}

/// One 24 pt row. `highlighted` (the pointer is over it, or it is open)
/// paints `raised` with a 4 pt radius and takes the hairline away, as the
/// canvas does; `editing` adds the amber bar at the left edge.
struct SetupRow<Content: View>: View {
    var highlighted = false
    var editing = false
    var separator = true
    /// A `line2` rule above the row, for the totals.
    var topRule = false
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 0) { content }
            .frame(height: Metrics.rowHeight)
            .contentShape(Rectangle())
            .background {
                if highlighted {
                    RoundedRectangle(cornerRadius: 4).fill(Ink.raised)
                }
            }
            .overlay(alignment: .bottom) {
                if separator && !highlighted {
                    Rectangle().fill(Ink.rowLine).frame(height: 1)
                }
            }
            .overlay(alignment: .top) {
                if topRule { Rectangle().fill(Ink.line2).frame(height: 1) }
            }
            .overlay(alignment: .leading) {
                if editing {
                    RoundedRectangle(cornerRadius: 1).fill(Ink.accent).frame(width: 2).padding(.vertical, 3)
                }
            }
    }
}

/// A small label in a `raised` pill: "system", "archived". With `fixed`
/// off it takes the width it is offered, and shortens with "…" in a narrow
/// cell.
struct SetupTag: View {
    let text: String
    var fixed = true

    var body: some View {
        Text(text)
            .font(Face.ui(10, .semibold))
            .foregroundStyle(Ink.text3)
            .lineLimit(1)
            .padding(.horizontal, 5)
            .frame(height: 16)
            .background(Ink.raised, in: RoundedRectangle(cornerRadius: 4))
            .fixedSize(horizontal: fixed, vertical: fixed)
    }
}

/// An alias as a chip.
struct AliasChip: View {
    let text: String

    var body: some View {
        Text(text)
            .font(Face.footnote)
            .foregroundStyle(Ink.text2)
            .padding(.horizontal, 5)
            .frame(height: 16)
            .background(Ink.raised, in: RoundedRectangle(cornerRadius: 4))
            .fixedSize()
    }
}

/// The icon button of a row's last column. Only the row under the pointer
/// shows one, so a quiet table stays quiet.
struct SetupIconButton: View {
    let symbol: String
    let help: String
    let label: String
    /// Smaller and heavier for a glyph that reads as a mark (`xmark`) rather
    /// than a picture.
    var symbolSize: CGFloat = 11
    var symbolWeight: Font.Weight = .medium
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: symbolSize, weight: symbolWeight))
                .foregroundStyle(Ink.text3)
                .frame(width: 20, height: 20)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
        .accessibilityLabel(label)
    }
}

/// The last column's icon, only on the row under the pointer of a vault
/// that may change (`shown`): archive, or restore for an archived row. The
/// command is the one the row's menu sends.
struct SetupArchiveCell: View {
    let name: String
    var archived = false
    let shown: Bool
    let action: () -> Void

    var body: some View {
        SetupCell(width: SetupColumn.action) {
            if shown {
                if archived {
                    SetupIconButton(
                        symbol: "tray.and.arrow.up",
                        help: String(localized: "Restore"),
                        label: String(localized: "Restore \(name)"),
                        action: action
                    )
                } else {
                    SetupIconButton(
                        symbol: "archivebox",
                        help: String(localized: "Archive"),
                        label: String(localized: "Archive \(name)"),
                        action: action
                    )
                }
            }
        }
    }
}

/// An archived row, its cells dimmed by the table: raised under the
/// pointer, with the restore icon there and Restore in its menu.
struct SetupArchivedRow<Cells: View>: View {
    let name: String
    let id: Uuid
    @Binding var hovered: Uuid?
    let canWrite: Bool
    let restore: () -> Void
    @ViewBuilder var cells: Cells

    var body: some View {
        SetupRow(highlighted: hovered == id) {
            cells
            SetupArchiveCell(name: name, archived: true, shown: hovered == id && canWrite, action: restore)
        }
        .hoverId($hovered, id)
        .contextMenu {
            if canWrite { Button(String(localized: "Restore"), action: restore) }
        }
    }
}

// MARK: - Editing by rows

/// A cell that can hold the caret in a table edited by rows: its row, `nil`
/// for the empty line.
protocol RowFocus: Hashable {
    var row: Uuid? { get }
}

/// A text cell of a Setup table.
nonisolated struct SetupFocus<Field: Hashable>: RowFocus {
    let row: Uuid?
    let field: Field
}

/// A text cell as the Setup tables type into it: plain, in the row's face,
/// ↩ for `submit`.
struct SetupTextField<Focus: Hashable>: View {
    let placeholder: String
    @Binding var text: String
    var alignment: TextAlignment = .leading
    let focus: FocusState<Focus?>.Binding
    let key: Focus
    let submit: () -> Void

    var body: some View {
        TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .font(Face.row)
            .multilineTextAlignment(alignment)
            .foregroundStyle(Ink.text)
            .focused(focus, equals: key)
            .onSubmit(submit)
    }
}

extension View {
    /// The one rule of the tables edited by rows: the open row (`editing`)
    /// is saved when the focus leaves it, for another cell or for nothing
    /// with the pointer elsewhere, the way a spreadsheet does. The row's own
    /// context menu resigns the field without giving the focus to anything,
    /// with the pointer over the row: that is not leaving it. Another vault
    /// drops the open row and `reset`s the empty line; a vault turned
    /// read-only drops the open row, which could not be saved.
    ///
    /// Whoever drops the open row clears `editing` before the focus, so the
    /// blur that follows is not read as leaving it.
    func editsRows<Focus: RowFocus>(
        editing: Binding<Uuid?>,
        focus: FocusState<Focus?>.Binding,
        hovered: Uuid?,
        store: AppStore,
        reset: @escaping () -> Void,
        commit: @escaping (Uuid) -> Void
    ) -> some View {
        onChange(of: store.currentVault?.id) { _, _ in
            editing.wrappedValue = nil
            focus.wrappedValue = nil
            reset()
        }
        .onChange(of: focus.wrappedValue) { old, new in
            guard let open = editing.wrappedValue else { return }
            if let new {
                if new.row != open { commit(open) }
            } else if old?.row == open, hovered != open {
                commit(open)
            }
        }
        .onChange(of: store.isReadOnly) { _, _ in
            editing.wrappedValue = nil
            focus.wrappedValue = nil
        }
    }

    /// esc while `active`, and otherwise left to whoever else wants it.
    func onEscape(when active: Bool = true, perform action: @escaping () -> Void) -> some View {
        onKeyPress(.escape) {
            guard active else { return .ignored }
            action()
            return .handled
        }
    }
}

/// How full a capped envelope is, as the 2 pt bar under its balance.
/// The RIEPILOGO's gauges answer the same question,
/// so this asks `FundGauge` rather than redoing its arithmetic: the balance
/// against a net cap, the cumulative income against an income cap.
enum EnvelopeCapFill {
    /// 0...1, or `nil` for an envelope with no cap (no bar at all).
    static func fraction(mode: FlowMode, balance: Int64, incomeTotal: Int64?) -> Double? {
        let gauge: FundGauge
        switch mode {
        case .unlimited:
            return nil
        case .netCapped(let cap):
            gauge = FundGauge(id: "", name: "", cap: cap, filled: balance)
        case .incomeCapped(let cap):
            gauge = FundGauge(id: "", name: "", cap: cap, filled: incomeTotal ?? balance)
        }
        return gauge.fraction
    }
}

/// The 72 × 2 pt bar itself.
struct CapBar: View {
    let fraction: Double

    var body: some View {
        MeterBar(fraction: fraction, tint: Ink.chartIncome, track: Ink.line2, height: 2, rounded: true)
            .frame(width: 72)
            .accessibilityElement()
            .accessibilityLabel(String(localized: "\(fraction.formatted(.percent.precision(.fractionLength(0)))) of the cap"))
    }
}
