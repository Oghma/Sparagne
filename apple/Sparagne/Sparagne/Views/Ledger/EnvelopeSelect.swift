import SwiftUI
import SparagneCore

/// One entry of the list under an ENVELOPE cell.
struct EnvelopeChoice: Identifiable, Equatable, Sendable {
    let id: Uuid
    /// As the column writes it: Unallocated in the app's language.
    let name: String
}

/// The list behind an ENVELOPE cell, which is a select and not a field: it
/// offers the vault's active envelopes and only ever writes one of them back,
/// so a row cannot be saved on an envelope that does not exist.
///
/// The cell opens the list as it takes the caret, every envelope in the
/// vault's order (Unallocated first, as the core sorts them) and the one the
/// row is on highlighted. What is typed narrows the list and is never saved:
/// the names from their start first, then the names that hold it anywhere,
/// case and accents aside, as the CATEGORY cell's list matches.
///
/// One per grid, shared by its rows, as `CategoryCompletionModel` is: only one
/// cell has the caret at a time, and the grid draws the list over everything
/// else. Free of views, so the filter and the arrows can be tested without a
/// window.
@Observable
final class EnvelopeChoiceModel {
    /// What the list shows now: every envelope, or those matching the text.
    private(set) var choices: [EnvelopeChoice] = []
    /// Index into `choices`.
    private(set) var selection = 0
    /// The cell the list is open for; `nil` while it is closed.
    private(set) var owner: AnyHashable?
    /// The envelope the open cell is on: the list ticks it, and highlights
    /// it while nothing is typed.
    private(set) var current: Uuid?
    /// The text the list was narrowed by, for the line it shows when nothing
    /// matches.
    private(set) var query = ""

    @ObservationIgnored private var envelopes: [EnvelopeChoice] = []
    /// Writes the picked envelope into the cell that opened the list.
    @ObservationIgnored private var write: ((Uuid) -> Void)?

    /// Open even with nothing to show: the list then says nothing matches.
    var isOpen: Bool { owner != nil }

    var selected: EnvelopeChoice? {
        choices.indices.contains(selection) ? choices[selection] : nil
    }

    /// The vault's active envelopes as the list names them.
    static func envelopes(in store: AppStore) -> [EnvelopeChoice] {
        store.flows.map { EnvelopeChoice(id: $0.id, name: store.flowName($0)) }
    }

    /// `envelopes` narrowed by `query`: from the start of the name, then
    /// anywhere in it, each group in the vault's order. Blank text is all of
    /// them.
    static func choices(for query: String, among envelopes: [EnvelopeChoice]) -> [EnvelopeChoice] {
        let needle = CategoryCompletion.fold(query.trimmingCharacters(in: .whitespaces))
        guard !needle.isEmpty else { return envelopes }
        let folded = envelopes.map { (choice: $0, name: CategoryCompletion.fold($0.name)) }
        let fromStart = folded.filter { $0.name.hasPrefix(needle) }
        let inside = folded.filter { !$0.name.hasPrefix(needle) && $0.name.contains(needle) }
        return (fromStart + inside).map(\.choice)
    }

    /// The cell `owner` took the caret, or was typed into while closed.
    func open(
        owner: AnyHashable,
        envelopes: [EnvelopeChoice],
        current: Uuid?,
        write: @escaping (Uuid) -> Void
    ) {
        self.owner = owner
        self.envelopes = envelopes
        self.current = current
        self.write = write
        filter("")
    }

    /// The text of the open cell changed: show what matches it. The
    /// highlight starts on the best match, or on the envelope the row is on
    /// while nothing is typed.
    func filter(_ text: String) {
        guard isOpen else { return }
        query = text.trimmingCharacters(in: .whitespaces)
        choices = Self.choices(for: query, among: envelopes)
        selection = query.isEmpty ? (choices.firstIndex { $0.id == current } ?? 0) : 0
    }

    /// ↓ is `move(by: 1)`, ↑ is `move(by: -1)`; both wrap, as the palette's do.
    func move(by delta: Int) {
        guard !choices.isEmpty else { return }
        selection = ((selection + delta) % choices.count + choices.count) % choices.count
    }

    /// The pointer on an entry.
    func highlight(_ index: Int) {
        guard choices.indices.contains(index) else { return }
        selection = index
    }

    /// ↩, ⇥ or a click: the highlighted envelope goes into the cell and the
    /// list closes. `false` when there was nothing to pick.
    @discardableResult
    func pick() -> Bool {
        guard let chosen = selected, let write else { return false }
        close()
        write(chosen.id)
        return true
    }

    /// esc, a pick, or the cell losing the caret.
    func close() {
        choices = []
        selection = 0
        owner = nil
        current = nil
        query = ""
        write = nil
    }

    /// Whether ↩ should pick rather than save the row: only when the pick
    /// would move the row to another envelope.
    var picksOnReturn: Bool {
        guard let selected else { return false }
        return selected.id != current
    }
}

// MARK: - The cell

/// Where the open list's cell is, for the grid to draw the list at.
struct EnvelopeChoiceAnchor: PreferenceKey {
    static let defaultValue: Anchor<CGRect>? = nil

    static func reduce(value: inout Anchor<CGRect>?, nextValue: () -> Anchor<CGRect>?) {
        value = value ?? nextValue()
    }
}

/// An ENVELOPE cell: it shows the envelope and opens the list as it takes the
/// caret. ↑↓ move in the list, ↩ and ⇥ pick, esc closes it; typing narrows it.
/// With the list closed every key does what it does in the other cells: ↩
/// saves the row, ⇥ moves on, esc throws the draft away, and ↓ or a letter
/// opens the list again.
///
/// The text field holds only what is typed to narrow the list, never the
/// envelope: whatever is left in it when the caret goes is dropped, so the
/// row keeps the last envelope picked. The envelope itself is the field's
/// prompt, dimmed while the caret is in the cell.
struct EnvelopeSelectCell: View {
    @Binding var flowId: Uuid?
    /// What the cell says while it holds no envelope: the empty line before
    /// the vault has loaded, which then lands on the sticky default.
    let placeholder: String
    let store: AppStore
    let choices: EnvelopeChoiceModel
    @FocusState.Binding var focus: CellFocus?
    let key: CellFocus
    let onSubmit: () -> Void

    @State private var query = ""

    var body: some View {
        HStack(spacing: 4) {
            TextField("", text: $query, prompt: Text(name ?? placeholder).foregroundStyle(promptTint))
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .accessibilityLabel(RowField.flow.label)
                .accessibilityValue(query.isEmpty ? (name ?? placeholder) : query)
                .focused($focus, equals: key)
                .onSubmit(onSubmit)
            // The mark of a select. A click on it opens the list whether or
            // not the cell already has the caret, and closes it when open.
            Image(systemName: "chevron.up.chevron.down")
                .font(.system(size: 8, weight: .semibold))
                .foregroundStyle(Ink.text3)
                .contentShape(Rectangle())
                .onTapGesture {
                    if isOwner {
                        choices.close()
                        query = ""
                    } else {
                        focus = key
                        open()
                    }
                }
                .accessibilityHidden(true)
        }
        .cellFocusRing(focus == key)
        .onKeyPress(.upArrow) { arrow(-1) }
        .onKeyPress(.downArrow) { arrow(1) }
        .onKeyPress(.return) {
            guard isOwner else { return .ignored }
            guard choices.picksOnReturn else {
                // Nothing to change: the key saves the row.
                choices.close()
                query = ""
                return .ignored
            }
            choices.pick()
            return .handled
        }
        .onKeyPress(.tab) {
            guard isOwner, choices.pick() else { return .ignored }
            // On to CATEGORIA, as ⇥ would have gone. After the pick has
            // reached the draft, or leaving the row would save the old one.
            Task { focus = CellFocus(row: key.row, field: .category) }
            return .handled
        }
        .onKeyPress(.escape) {
            guard isOwner else { return .ignored }
            choices.close()
            query = ""
            return .handled
        }
        // A click on the ENVELOPE cell of a closed row focuses it in the
        // update that builds this view, where `onChange` does not fire.
        .onAppear {
            if focus == key { open() }
        }
        .onChange(of: focus) { old, new in
            if new == key {
                open()
            } else if old == key {
                query = ""
                if isOwner { choices.close() }
            }
        }
        .onChange(of: query) { _, new in
            guard focus == key else { return }
            // A letter typed after esc or a pick opens the list again; the
            // text emptied by a pick does not.
            if !isOwner {
                guard !new.isEmpty else { return }
                open()
            }
            choices.filter(new)
        }
        // The whole row's height, so the grid can hang the list from the
        // row's bottom edge rather than from the text's.
        .frame(height: Metrics.rowHeight)
        .anchorPreference(key: EnvelopeChoiceAnchor.self, value: .bounds) { isOwner ? $0 : nil }
    }

    private var isOwner: Bool { choices.owner == AnyHashable(key) }

    /// The envelope's name; an archived one still has it.
    private var name: String? {
        flowId.map { store.envelopeName($0) }
    }

    /// Dimmed under the caret, where it says what typing would replace.
    private var promptTint: Color {
        flowId == nil || focus == key ? Ink.text3 : Ink.text
    }

    private func open() {
        choices.open(
            owner: key,
            envelopes: EnvelopeChoiceModel.envelopes(in: store),
            current: flowId
        ) { picked in
            flowId = picked
            query = ""
        }
    }

    private func arrow(_ delta: Int) -> KeyPress.Result {
        guard focus == key else { return .ignored }
        if isOwner {
            choices.move(by: delta)
        } else {
            open()
        }
        return .handled
    }
}

// MARK: - The list

/// The envelopes under the cell, drawn as the CATEGORY cell's list is: the
/// envelope the row is on ticked, the highlighted one tinted with the accent.
/// A click picks, the pointer highlights, and the footer says which keys do
/// the same. Longer than `visibleRows`, it scrolls, keeping the highlighted
/// entry in sight.
struct EnvelopeChoiceList: View {
    let choices: EnvelopeChoiceModel

    static let visibleRows = 10

    /// What the list will measure, for the grid to decide whether it fits
    /// under the cell or has to open upwards. With nothing matching it is
    /// one line saying so.
    static func height(for count: Int) -> CGFloat {
        FloatingList.chrome + FloatingList.groupHeight
            + CGFloat(min(max(count, 1), visibleRows)) * FloatingList.rowHeight
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            FloatingListGroup(title: String(localized: "Envelopes"))
            if choices.choices.isEmpty {
                Text(String(localized: "No envelope matches \u{201C}\(choices.query)\u{201D}"))
                    .foregroundStyle(Ink.text3)
                    .floatingListRow(active: false)
            } else if choices.choices.count > Self.visibleRows {
                ScrollViewReader { proxy in
                    ScrollView {
                        entries
                    }
                    .scrollIndicators(.never)
                    .frame(height: CGFloat(Self.visibleRows) * FloatingList.rowHeight)
                    .onChange(of: choices.selection, initial: true) { _, index in
                        guard choices.choices.indices.contains(index) else { return }
                        proxy.scrollTo(choices.choices[index].id)
                    }
                }
            } else {
                entries
            }
            FloatingListFooter()
        }
        .floatingList()
    }

    private var entries: some View {
        VStack(spacing: 0) {
            ForEach(Array(choices.choices.enumerated()), id: \.element.id) { index, choice in
                row(index: index, choice: choice)
                    .id(choice.id)
            }
        }
    }

    private func row(index: Int, choice: EnvelopeChoice) -> some View {
        let active = index == choices.selection
        let current = choice.id == choices.current
        return HStack(spacing: 8) {
            Text(choice.name).foregroundStyle(Ink.text)
            Spacer(minLength: 0)
            if current {
                Image(systemName: "checkmark")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(Ink.accent)
            }
        }
        .floatingListRow(active: active)
        .onHover { inside in
            if inside { choices.highlight(index) }
        }
        .onTapGesture { choose(index) }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(choice.name)
        .accessibilityAddTraits(active || current ? [.isButton, .isSelected] : .isButton)
        // A tap gesture is not an action VoiceOver can press: the row says it
        // is a button, so it has to act like one.
        .accessibilityAction { choose(index) }
    }

    /// A click on a row, or VoiceOver pressing it: that envelope goes into
    /// the cell and the list closes (`EnvelopeChoiceModel.pick`).
    private func choose(_ index: Int) {
        choices.highlight(index)
        choices.pick()
    }
}
