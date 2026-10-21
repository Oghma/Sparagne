import SwiftUI

/// The frame the Ricorrenze and Riparto inspectors share: a heading and the
/// groups under it in a scroll view, and a footer of buttons pinned below,
/// all on the chrome's ground. The groups and the footer are the
/// inspector's own; `accessory` sits at the right of the title (the
/// template's "…" menu).
///
/// `fieldsDisabled` covers the heading too, as the inspectors had it;
/// `footerDisabled` is for a command in flight, which the footer waits for.
struct InspectorShell<Accessory: View, Content: View, Footer: View>: View {
    let title: String
    /// The schedule in words, under the title.
    let subtitle: String
    var fieldsDisabled = false
    var footerDisabled = false
    @ViewBuilder var accessory: Accessory
    @ViewBuilder var content: Content
    @ViewBuilder var footer: Footer

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    heading
                    content
                }
                .disabled(fieldsDisabled)
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            InspectorFooter(disabled: footerDisabled) { footer }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Ink.bg)
    }

    private var heading: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 8) {
                Text(title)
                    .font(Face.ui(16, .semibold))
                    .foregroundStyle(Ink.text)
                    .lineLimit(1)
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: 0)
                accessory
            }
            Text(subtitle)
                .font(Face.row)
                .foregroundStyle(Ink.text2)
                .lineLimit(2)
        }
    }
}

extension InspectorShell where Accessory == EmptyView {
    init(
        title: String,
        subtitle: String,
        fieldsDisabled: Bool = false,
        footerDisabled: Bool = false,
        @ViewBuilder content: () -> Content,
        @ViewBuilder footer: () -> Footer
    ) {
        self.init(
            title: title,
            subtitle: subtitle,
            fieldsDisabled: fieldsDisabled,
            footerDisabled: footerDisabled,
            accessory: { EmptyView() },
            content: content,
            footer: footer
        )
    }
}

/// The inspector's band of buttons: 44 pt at least, a hairline above it.
struct InspectorFooter<Content: View>: View {
    var disabled = false
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 8) { content }
            .disabled(disabled)
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .frame(minHeight: 44)
            .overlay(alignment: .top) { Hairline() }
            .background(Ink.bg)
            .accessibilityElement(children: .contain)
    }
}

/// The footer's on/off switch with its word beside it. The switch speaks
/// for itself to VoiceOver; the word is for the eye, or it would be a
/// second stop saying the same. `disabled` holds the switch only, not the
/// word. Two views, which the footer's row spaces as its own.
struct InspectorSwitch: View {
    let label: String
    let isOn: Bool
    var disabled = false
    let set: (Bool) -> Void

    var body: some View {
        FormSwitch(label: label, isOn: isOn, set: set)
            .disabled(disabled)
        Text(label)
            .font(Face.row)
            .foregroundStyle(Ink.text2)
            .accessibilityHidden(true)
    }
}
