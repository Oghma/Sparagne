import SwiftUI

/// The strip of things to do under the RIEPILOGO's heading: one chip per
/// item, a click opens the tab that deals with it. Not drawn when empty
/// (`SummaryView`).
///
/// The chips are the fund cards' tag made clickable: the same small text on
/// the raised ground, with the accent dot that marks them as something to
/// act on, since amber is the palette's color for interaction.
struct SummaryTodoStrip: View {
    let items: [SummaryTodoItem]
    let store: AppStore

    @State private var hovered: String?

    var body: some View {
        HStack(spacing: 6) {
            ForEach(items) { item in
                chip(item)
            }
            Spacer(minLength: 0)
        }
    }

    private func chip(_ item: SummaryTodoItem) -> some View {
        let label = SummaryText.todo(item.kind)
        return Button {
            Task { await store.open(item) }
        } label: {
            HStack(spacing: 6) {
                Circle()
                    .fill(Ink.accent)
                    .frame(width: 5, height: 5)
                Text(label)
                    .lineLimit(1)
            }
            .font(Face.ui(11.5, .medium))
            .foregroundStyle(Ink.text)
            .padding(.horizontal, 7)
            .frame(height: 20)
            .background(hovered == item.id ? Ink.hi : Ink.raised, in: RoundedRectangle(cornerRadius: 4))
            .contentShape(RoundedRectangle(cornerRadius: 4))
        }
        .buttonStyle(.plain)
        .hoverId($hovered, item.id)
        // One button per chip: the dot is decoration, the label says it all.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { Task { await store.open(item) } }
    }
}
