import SwiftUI

extension View {
    /// One cell of a hand-drawn table row: one line cut short with "…",
    /// `padding` on either side, and either a fixed `width` or at least
    /// `minWidth` and whatever the row has left. `hitTestable` makes the
    /// whole cell the click target rather than only what is drawn in it.
    func tableCell(
        width: CGFloat? = nil,
        minWidth: CGFloat? = nil,
        alignment: Alignment = .leading,
        padding: CGFloat = Metrics.cellPad,
        hitTestable: Bool = false
    ) -> some View {
        modifier(
            TableCell(width: width, minWidth: minWidth, alignment: alignment, padding: padding, hitTestable: hitTestable)
        )
    }

    /// A `tableCell` of a `FittedTable`: at least `width`, and a share of
    /// what the table has left while its columns are `flexible`; `width`
    /// exactly once they scroll.
    func fittedCell(
        _ width: CGFloat,
        flexible: Bool,
        alignment: Alignment = .leading,
        padding: CGFloat = Metrics.cellPad
    ) -> some View {
        tableCell(width: flexible ? nil : width, minWidth: width, alignment: alignment, padding: padding)
    }
}

/// A table whose columns share the width it is offered when they fit
/// (`table(true)`), and that keeps their own widths and scrolls sideways
/// when they do not (`table(false)`).
struct FittedTable<Table: View>: View {
    @ViewBuilder let table: (_ flexible: Bool) -> Table

    var body: some View {
        ViewThatFits(in: .horizontal) {
            table(true)
            ScrollView(.horizontal) { table(false) }
                .scrollBounceBehavior(.basedOnSize)
        }
    }
}

private struct TableCell: ViewModifier {
    let width: CGFloat?
    let minWidth: CGFloat?
    let alignment: Alignment
    let padding: CGFloat
    let hitTestable: Bool

    func body(content: Content) -> some View {
        let cell = content
            .lineLimit(1)
            .truncationMode(.tail)
            .padding(.horizontal, padding)
            .frame(
                minWidth: width ?? minWidth,
                idealWidth: width,
                maxWidth: width ?? .infinity,
                alignment: alignment
            )
        if hitTestable {
            cell.contentShape(Rectangle())
        } else {
            cell
        }
    }
}
