import SwiftUI

extension View {
    /// Keeps `hovered` on `id` while the pointer is over this view. Leaving
    /// clears it only if it still says `id`: the pointer may already be on
    /// the next row, whose enter can arrive before this row's exit.
    func hoverId<ID: Equatable>(_ hovered: Binding<ID?>, _ id: ID) -> some View {
        onHover { inside in
            if inside {
                hovered.wrappedValue = id
            } else if hovered.wrappedValue == id {
                hovered.wrappedValue = nil
            }
        }
    }
}
