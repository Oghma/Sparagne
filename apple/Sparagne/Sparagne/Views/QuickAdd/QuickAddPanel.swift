import SwiftUI
import SparagneCore

/// ⌘K: the one-line grammar of `DISTILLATO_V1.md` §3.1, floating near the top
/// of the window, and the command palette of `docs/v2/UI.md` §6 when the line
/// starts with `>`.
///
/// The grid covers the common case; this covers the fast case, where the
/// whole row is one line of text and the fingers never leave the keyboard.
/// One field, two grammars: a transaction, or a command. Under the field the
/// parsed line is laid out as chips (`QuickAddTokens`), one per field it will
/// write, so a line read differently from what was meant shows before ↩. On
/// a vault that is only read, the field is the palette alone
/// (`commandsOnly`).
struct QuickAddOverlay: View {
    @Bindable var store: AppStore
    let engine: SyncEngine?
    @Binding var isPresented: Bool

    @FocusState private var focused: Bool
    @State private var palette = CommandPaletteModel()
    /// What the vault's history files the line's note under, when the line
    /// names no category: shown beside the chips, added only by ⇥.
    @State private var hint: NoteSuggestion?

    static let width: CGFloat = 600
    /// `#17171B`: a step above the cards, so the panel floats over the sheet.
    private static let ground = Color(hex: 0x17171B)

    /// An account that only reads the vault cannot add a row, so for it the
    /// panel is the palette and nothing else: the list shows from the start,
    /// whatever is typed filters it, and the `>` may be typed or not. ⌘K
    /// stays the one way into the palette (`SparagneApp`, File menu).
    private var commandsOnly: Bool { !store.canWrite }

    /// `>` in first position turns the field into the palette.
    private var isCommand: Bool { commandsOnly || CommandPaletteModel.isCommand(store.quickAddText) }

    /// What filters the palette: the text after the `>`, or the whole line
    /// when the panel is only the palette and no `>` was typed.
    private var query: String {
        let text = store.quickAddText
        guard commandsOnly, !CommandPaletteModel.isCommand(text) else { return CommandPaletteModel.query(in: text) }
        return text.trimmingCharacters(in: .whitespaces)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            input
            if isCommand {
                Hairline()
                CommandPaletteList(model: palette, onRun: close)
            } else if !trimmed.isEmpty {
                Hairline()
                parsed
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
            }
            footer
        }
        .frame(width: Self.width, alignment: .leading)
        .background(Self.ground)
        .clipShape(shape)
        .overlay(shape.strokeBorder(Ink.line2, lineWidth: 1))
        .shadow(color: .black.opacity(0.65), radius: 30, y: 24)
        .onAppear {
            focused = true
            refreshActions()
        }
        // The entries say what the toggles will do and list the other vaults,
        // so they are built fresh every time the `>` is typed.
        .onChange(of: isCommand) { _, now in
            if now { refreshActions() }
        }
        .onChange(of: store.quickAddText) { _, _ in
            palette.query = query
        }
        .task(id: store.quickAddText) { await suggestCategory() }
        .onExitCommand(perform: close)
        .onChange(of: store.savedAt) { _, _ in
            // `resolveAmbiguous` resubmits from the error alert's candidate
            // buttons, outside this field's own `onSubmit`, and clears the
            // text on success. A save that lands while the line still holds
            // text is someone else's (a pending void flushing) and must not
            // take the line away.
            if store.quickAddText.isEmpty { isPresented = false }
        }
    }

    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: 14) }

    private var trimmed: String {
        store.quickAddText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    // MARK: - The field

    private var input: some View {
        HStack(spacing: 10) {
            // A plus would promise a row the panel cannot add.
            Image(systemName: commandsOnly ? "chevron.right" : "plus")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Ink.accent)
                .accessibilityHidden(true)
            TextField(
                "",
                text: $store.quickAddText,
                prompt: Text(placeholder).foregroundStyle(Ink.text3)
            )
            .textFieldStyle(.plain)
            .font(Face.ui(16))
            .foregroundStyle(Ink.text)
            .focused($focused)
            .accessibilityLabel(commandsOnly ? String(localized: "Command Palette") : String(localized: "Quick Add"))
            // The arrows belong to the list while the field is a palette; the
            // caret gets them back as soon as the `>` is gone.
            .onKeyPress(.upArrow) {
                guard isCommand else { return .ignored }
                palette.move(by: -1)
                return .handled
            }
            .onKeyPress(.downArrow) {
                guard isCommand else { return .ignored }
                palette.move(by: 1)
                return .handled
            }
            // ⇥ takes the hint into the line, where the chips show it; without
            // one the key does what it always did.
            .onKeyPress(.tab) {
                guard !isCommand, let category = categoryHint,
                      let accepted = QuickAddSummary.accepting(category, into: store.quickAddText)
                else { return .ignored }
                store.quickAddText = accepted
                return .handled
            }
            .onSubmit(submit)
        }
        .padding(.horizontal, 16)
        .frame(height: 52)
    }

    /// The grammar by example, or for a vault that is only read, why the
    /// panel lists commands and takes no line.
    private var placeholder: String {
        commandsOnly
            ? String(localized: "Type a command: this vault is read-only")
            : String(localized: "-12.50 pizza #food @cash >groceries")
    }

    private func submit() {
        if isCommand {
            // Nothing highlighted means nothing matched: stay open so the
            // query can be fixed.
            Task { if await palette.run() { close() } }
            return
        }
        Task {
            await store.submit(quickAdd: store.quickAddText)
            // A failed submit leaves the text in place and raises
            // `presentedError`; stay open so the user can fix the line
            // instead of closing over an empty grid.
            if store.presentedError == nil {
                isPresented = false
            }
        }
    }

    // MARK: - The parsed line

    /// The chips, or the parser's complaint in red.
    @ViewBuilder
    private var parsed: some View {
        switch store.preview(quickAdd: trimmed) {
        case .success(let line):
            let tokens = QuickAddTokens.make(line, currency: store.currency)
            TokenFlow(spacing: 6) {
                ForEach(tokens) { token in
                    TokenChip(token: token)
                }
                if let category = categoryHint {
                    HintChip(category: category)
                }
            }
            .accessibilityElement(children: .combine)
        case .failure(let error):
            Text(error.message)
                .font(Face.ui(12))
                .foregroundStyle(Ink.negative)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    // MARK: - The footer

    /// The keys the panel takes, for the mode it is in.
    private var footer: some View {
        HStack(spacing: 16) {
            if isCommand {
                hint("\u{2191}\u{2193}", String(localized: "select"))
                hint("\u{21A9}", String(localized: "run"))
            } else {
                hint("\u{21A9}", String(localized: "add"))
                hint("\u{21E5}", String(localized: "suggested category"))
                hint(">", String(localized: "commands"))
            }
            hint("esc", String(localized: "close"))
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 9)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Ink.bg)
        .overlay(alignment: .top) { Hairline() }
        .accessibilityElement(children: .combine)
    }

    private func hint(_ key: String, _ label: String) -> some View {
        KeyHint(key: key, label: label, font: Face.ui(11.5))
    }

    // MARK: - Actions

    private func close() {
        store.quickAddText = ""
        isPresented = false
    }

    private func refreshActions() {
        palette.actions = CommandPaletteModel.ledgerActions(store: store, engine: engine)
        palette.query = query
    }

    /// The note of the line as typed, when the line parses and names no
    /// category.
    private var noteWithoutCategory: String? {
        guard !trimmed.isEmpty, !isCommand, case .success(let parsed) = store.preview(quickAdd: trimmed) else {
            return nil
        }
        return QuickAddSummary.noteWithoutCategory(parsed)
    }

    /// The hint, while it is about the note on the line and `#` can carry it.
    private var categoryHint: String? {
        guard let hint, hint.note == noteWithoutCategory,
              QuickAddSummary.accepting(hint.category, into: store.quickAddText) != nil
        else { return nil }
        return hint.category
    }

    /// The line has been still for a moment: ask the core about its note. An
    /// answer for a note the line no longer has is dropped.
    private func suggestCategory() async {
        guard let note = noteWithoutCategory, hint?.note != note else { return }
        guard (try? await Task.sleep(for: .milliseconds(250))) != nil else { return }
        guard let category = await store.suggestedCategory(forNote: note), noteWithoutCategory == note else { return }
        hint = NoteSuggestion(note: note, category: category)
    }
}

// MARK: - Chips

/// One field of the parsed line (`.tok`): the small word naming it, then its
/// value. A default the line did not say is drawn in `text3`.
private struct TokenChip: View {
    let token: QuickAddToken

    var body: some View {
        HStack(spacing: 6) {
            if let label = token.label {
                Text(label).foregroundStyle(Ink.text3)
            }
            Text(token.value).foregroundStyle(token.isDefault ? Ink.text3 : Ink.text)
        }
        .font(Face.ui(12))
        .lineLimit(1)
        .padding(.horizontal, 9)
        .frame(height: 24)
        .background(Ink.raised, in: RoundedRectangle(cornerRadius: 6))
    }
}

/// The category the history suggests for the note, offered with ⇥: in the
/// accent, since it is the one thing on the panel waiting for a key.
private struct HintChip: View {
    let category: String

    var body: some View {
        HStack(spacing: 6) {
            KeyCap(text: "\u{21E5}")
            Text(category).foregroundStyle(Ink.accent)
        }
        .font(Face.ui(12))
        .lineLimit(1)
        .padding(.leading, 4)
        .padding(.trailing, 9)
        .frame(height: 24)
        .background(Ink.accent.opacity(0.14), in: RoundedRectangle(cornerRadius: 6))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(String(localized: "Suggested category \(category), tab to add it"))
    }
}

/// Lays its children out left to right and wraps onto a new line when the
/// width runs out, as the chips of a long line need.
private struct TokenFlow: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(subviews, width: proposal.width ?? .infinity)
        let width = rows.map(\.width).max() ?? 0
        let height = rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(rows.count - 1, 0))
        return CGSize(width: proposal.width ?? width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in arrange(subviews, width: bounds.width) {
            var x = bounds.minX
            for index in row.indices {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index].place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += row.height + spacing
        }
    }

    private struct Row {
        var indices: [Int] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
    }

    private func arrange(_ subviews: Subviews, width: CGFloat) -> [Row] {
        var rows: [Row] = []
        var current = Row()
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            let needed = current.indices.isEmpty ? size.width : current.width + spacing + size.width
            if needed > width, !current.indices.isEmpty {
                rows.append(current)
                current = Row()
            }
            current.width = current.indices.isEmpty ? size.width : current.width + spacing + size.width
            current.height = max(current.height, size.height)
            current.indices.append(index)
        }
        if !current.indices.isEmpty { rows.append(current) }
        return rows
    }
}
