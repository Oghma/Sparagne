import SwiftUI
import SparagneCore

/// ⌘K: the one-line grammar of `DISTILLATO_V1.md` §3.1, over the grid, and
/// the command palette of `docs/v2/UI.md` §6 when the line starts with `>`.
///
/// The grid covers the common case; this covers the fast case, where the
/// whole row is one line of text and the fingers never leave the keyboard.
/// One field, two grammars: a transaction, or a command.
struct QuickAddOverlay: View {
    @Bindable var store: AppStore
    let engine: SyncEngine?
    @Binding var isPresented: Bool

    @FocusState private var focused: Bool
    @State private var palette = CommandPaletteModel()
    /// What the vault's history files the line's note under, when the line
    /// names no category: shown beside the preview, added only by ⇥.
    @State private var hint: NoteSuggestion?

    /// `>` in first position turns the field into the palette.
    private var isCommand: Bool { CommandPaletteModel.isCommand(store.quickAddText) }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            TextField(
                String(localized: "-12.50 pizza #food @cash >groceries"),
                text: $store.quickAddText
            )
            .textFieldStyle(.plain)
            .font(Face.ui(14))
            .foregroundStyle(Ink.text)
            .focused($focused)
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
            // ⇥ takes the hint into the line, where the preview shows it;
            // without one the key does what it always did.
            .onKeyPress(.tab) {
                guard !isCommand, let category = categoryHint,
                      let accepted = QuickAddSummary.accepting(category, into: store.quickAddText)
                else { return .ignored }
                store.quickAddText = accepted
                return .handled
            }
            .onSubmit {
                if isCommand {
                    // Nothing highlighted means nothing matched: stay open so
                    // the query can be fixed.
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

            if isCommand {
                CommandPaletteList(model: palette, onRun: close)
            } else {
                HStack(spacing: 12) {
                    Text(preview.text)
                        .font(Face.footnote)
                        .foregroundStyle(preview.isError ? Ink.negative : Ink.dim)
                        .lineLimit(1)
                    if let category = categoryHint {
                        Spacer(minLength: 0)
                        Text("\u{21E5} #\(category)")
                            .font(Face.footnote)
                            .foregroundStyle(Ink.text)
                            .lineLimit(1)
                            .accessibilityLabel(String(localized: "Suggested category \(category), tab to add it"))
                    }
                }
            }
        }
        .padding(14)
        .frame(width: 520, alignment: .leading)
        .background(Ink.panel)
        .overlay(Rectangle().strokeBorder(Ink.accent, lineWidth: 1))
        .shadow(color: .black.opacity(0.5), radius: 20, y: 8)
        .onAppear {
            focused = true
            refreshActions()
        }
        // The entries say what the toggles will do and list the other vaults,
        // so they are built fresh every time the `>` is typed.
        .onChange(of: isCommand) { _, now in
            if now { refreshActions() }
        }
        .onChange(of: store.quickAddText) { _, new in
            palette.query = CommandPaletteModel.query(in: new)
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

    private func close() {
        store.quickAddText = ""
        isPresented = false
    }

    private func refreshActions() {
        palette.actions = CommandPaletteModel.ledgerActions(store: store, engine: engine)
        palette.query = CommandPaletteModel.query(in: store.quickAddText)
    }

    private var preview: (text: String, isError: Bool) {
        let trimmed = store.quickAddText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return (" ", false) }
        switch store.preview(quickAdd: trimmed) {
        case .success(let parsed):
            return (QuickAddSummary.describe(parsed, currency: store.currency), false)
        case .failure(let error):
            return (error.message, true)
        }
    }

    /// The note of the line as typed, when the line parses and names no
    /// category.
    private var noteWithoutCategory: String? {
        let trimmed = store.quickAddText.trimmingCharacters(in: .whitespacesAndNewlines)
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
