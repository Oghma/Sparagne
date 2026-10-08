import SwiftUI

/// One entry of a status line: a label, and the value it names if any.
/// `"Conteggio 13"` is a label and a value; `"salvato alle 12:04"` is a
/// label alone, a whole sentence.
struct StatusItem: Hashable {
    let label: String
    var value: String?
}

/// The right-hand side of the tab bar: a few figures
/// about the sheet on screen, the way a spreadsheet's status bar shows the
/// count and the sum of a selection. Labels in `text3`, values in `text`,
/// " · " between the entries.
///
/// Each tab builds its own line (`LedgerStatusLine`, `SummaryStatusLine`,
/// `RecurringStatusLine`, `SetupStatusLine`); this only draws it.
struct StatusLine: View {
    let items: [StatusItem]

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                if index > 0 {
                    Text(verbatim: "\u{00B7}")
                        .padding(.horizontal, 7)
                        .accessibilityHidden(true)
                }
                Text(item.label)
                    .foregroundStyle(Ink.text3)
                if let value = item.value {
                    Text(value)
                        .fontWeight(.medium)
                        .foregroundStyle(Ink.text)
                        .padding(.leading, 4)
                }
            }
        }
        .font(Face.ui(11.5))
        .foregroundStyle(Ink.text3)
        .lineLimit(1)
        .fixedSize()
        // One sentence for VoiceOver, rather than a stop per fragment.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Self.spoken(items))
    }

    /// `"Count 13, Sum 619,94, saved at 12:04"`.
    static func spoken(_ items: [StatusItem]) -> String {
        items
            .map { item in item.value.map { "\(item.label) \($0)" } ?? item.label }
            .joined(separator: ", ")
    }
}
