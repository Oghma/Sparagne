import SwiftUI

/// The sync state at the end of the top bar: a dot
/// and a few words, and a popover with the account, the counts and what can
/// be done about the state. The words and the buttons come from
/// `SyncPillState`, so the Settings window says the same.
///
/// Without an engine (a demo database) it reads "Local only", and the
/// popover says the file never syncs.
struct SyncPill: View {
    let engine: SyncEngine?
    /// Requests one of `MainWindow`'s sheets: the refused changes.
    let present: (MainWindow.SheetKind) -> Void

    @State private var showsPopover = false

    var body: some View {
        let state = SyncPillState(engine: engine)
        Button {
            showsPopover.toggle()
        } label: {
            HStack(spacing: 6) {
                StatusDot(color: Self.color(state.tone))
                    .accessibilityHidden(true)
                Text(state.title)
            }
        }
        .buttonStyle(.pill(state.tone == .warning ? .warning : .neutral))
        .accessibilityLabel(String(localized: "Sync"))
        .accessibilityValue(state.title)
        .popover(isPresented: $showsPopover, arrowEdge: .bottom) {
            SyncPopover(state: state, engine: engine) { action in
                showsPopover = false
                if action == .review { present(.rejected) }
            }
        }
    }

    static func color(_ tone: SyncPillState.Tone) -> Color {
        switch tone {
        case .positive: Ink.positive
        case .active: Ink.accent
        case .muted: Ink.text3
        case .negative: Ink.negative
        case .warning: Ink.warning
        }
    }
}

/// The card under the pill: the state in a line, the account in a few
/// rows, and the state's buttons (`SyncPillState.actions`).
private struct SyncPopover: View {
    let state: SyncPillState
    let engine: SyncEngine?
    /// Called after a button did its part, so the pill can close the
    /// popover and present a sheet if the action needs one.
    let finished: (SyncPillState.Action) -> Void

    @Environment(\.openSettings) private var openSettings

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                StatusDot(color: SyncPill.color(state.tone))
                    .accessibilityHidden(true)
                Text(state.headline)
                    .font(Face.ui(13.5, .semibold))
                    .foregroundStyle(Ink.text)
            }
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(.isHeader)

            if let detail = state.detail {
                Text(detail)
                    .font(Face.ui(12.5))
                    .foregroundStyle(Ink.text2)
                    .fixedSize(horizontal: false, vertical: true)
            }

            if state.showsAccount, let engine {
                account(engine)
            }

            if !state.actions.isEmpty {
                HStack(spacing: 8) {
                    Spacer(minLength: 0)
                    ForEach(state.actions, id: \.self) { action in
                        button(action)
                    }
                }
                .padding(.top, 2)
            }
        }
        .padding(14)
        .frame(width: 300, alignment: .leading)
        // The popover draws the rounded card and its border; this is the
        // ground inside it, a step above the bars.
        .background(Self.ground)
        .presentationBackground(Self.ground)
    }

    private static let ground = Color(hex: 0x17171B)

    private func account(_ engine: SyncEngine) -> some View {
        Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 6) {
            row(String(localized: "Server"), engine.account.serverURLText)
            row(String(localized: "Account"), engine.account.username ?? "\u{2014}")
            row(
                String(localized: "Last sync"),
                engine.lastSyncAt.map { DateFormatting.relativeDayAndTime($0) } ?? "\u{2014}"
            )
            row(String(localized: "Pending"), engine.pendingCount.formatted(.number))
        }
        .font(Face.ui(12.5))
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .top) { Hairline() }
        .overlay(alignment: .bottom) { Hairline() }
    }

    private func row(_ label: String, _ value: String) -> some View {
        GridRow {
            Text(label)
                .foregroundStyle(Ink.text2)
                .frame(width: 86, alignment: .leading)
            Text(value)
                .foregroundStyle(Ink.text)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private func button(_ action: SyncPillState.Action) -> some View {
        switch action {
        case .syncNow:
            Button(String(localized: "Sync Now")) {
                if let engine { Task { await engine.syncNow() } }
                finished(action)
            }
            .buttonStyle(.chrome(small: true))
            // A round already running would ignore a second one.
            .disabled(state == .syncing)
        case .settings:
            Button {
                openSettings()
                finished(action)
            } label: {
                HStack(spacing: 5) {
                    Text(String(localized: "Account & Server\u{2026}"))
                    KeyCap(text: "\u{2318},")
                        .accessibilityHidden(true)
                }
            }
            .buttonStyle(.chrome(state == .sessionExpired ? .primary : .ghost, small: true))
        case .review:
            Button(String(localized: "Review")) { finished(action) }
                .buttonStyle(.chrome(.warning, small: true))
        case .connect:
            Button(String(localized: "Connect a Server\u{2026}")) {
                openSettings()
                finished(action)
            }
            .buttonStyle(.chrome(.primary, small: true))
        }
    }
}
