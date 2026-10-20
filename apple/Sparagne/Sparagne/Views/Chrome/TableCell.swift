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
