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
/// One per grid, shared by its rows, as `CategoryCompletionModel` is. The
/// list is open even with nothing to show: it then says nothing matches.
@Observable
final class EnvelopeChoiceModel {
    let list = FloatingListModel<EnvelopeChoice>()
    /// The envelope the open cell is on: the list ticks it, and highlights
    /// it while nothing is typed.
    private(set) var current: Uuid?
    /// The text the list was narrowed by, for the line it shows when nothing
    /// matches.
    private(set) var query = ""

    @ObservationIgnored private var envelopes: [EnvelopeChoice] = []

    /// The vault's active envelopes as the list names them.
    static func envelopes(in store: AppStore) -> [EnvelopeChoice] {
        store.flows.map { EnvelopeChoice(id: $0.id, name: store.flowName($0)) }
    }

    /// `envelopes` narrowed by `query`: from the start of the name, then
    /// anywhere in it, each group in the vault's order. Blank text is all of
    /// them.
    static func choices(for query: String, among envelopes: [EnvelopeChoice]) -> [EnvelopeChoice] {
        let needle = FloatingList.fold(query.trimmingCharacters(in: .whitespaces))
        guard !needle.isEmpty else { return envelopes }
        let folded = envelopes.map { (choice: $0, name: FloatingList.fold($0.name)) }
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
        self.envelopes = envelopes
        self.current = current
        list.open(for: owner) { write($0.id) }
        filter("")
    }

    /// The text of the open cell changed: show what matches it. The
    /// highlight starts on the best match, or on the envelope the row is on
    /// while nothing is typed.
    func filter(_ text: String) {
        guard list.isOpen else { return }
        query = text.trimmingCharacters(in: .whitespaces)
        let choices = Self.choices(for: query, among: envelopes)
        list.show(choices, selection: query.isEmpty ? (choices.firstIndex { $0.id == current } ?? 0) : 0)
    }

    /// Whether ↩ should pick rather than save the row: only when the pick
    /// would move the row to another envelope.
    var picksOnReturn: Bool {
        guard let selected = list.selected else { return false }
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

/// The ENVELOPE cell while it is typed into: it opens the list as it becomes
/// a field. ↑↓ move in the list, ↩ and ⇥ pick, esc closes it; typing narrows
/// it. With the list closed every key does what it does in the other cells
/// (`cellKeys`): ↩ saves the cell, ⇥ and the arrows move the cursor, esc puts
/// the cell back as it was, and a letter opens the list again.
///
/// The text field holds only what is typed to narrow the list, never the
/// envelope (`LedgerEditor.text`, while `flowId` is the pick): whatever is
/// left in it when the cell is saved is dropped, so the row keeps the last
/// envelope picked. The envelope itself is the field's prompt, dimmed, since
/// the caret is in the cell.
struct EnvelopeSelectCell: View {
    /// What the cell says while it holds no envelope: the empty line before
    /// the vault has loaded, which then lands on the sticky default.
    let placeholder: String
    let store: AppStore
    let choices: EnvelopeChoiceModel
    let key: CellFocus
    let edited: EditedCell

    private var query: Binding<String> { edited.text }

    var body: some View {
        HStack(spacing: 4) {
            TextField("", text: query, selection: edited.selection, prompt: Text(name ?? placeholder).foregroundStyle(Ink.text3))
                .textFieldStyle(.plain)
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .accessibilityLabel(RowField.flow.label)
                .accessibilityValue(query.wrappedValue.isEmpty ? (name ?? placeholder) : query.wrappedValue)
                .focused(edited.focus, equals: key)
                // On the field, inside the list's keys below, which SwiftUI
                // asks first.
                .cellKeys(edited, key: key)
            // The mark of a select. A click on it opens the list, and closes
            // it when open.
            Image(systemName: "chevron.up.chevron.down")
                .font(.system(size: 8, weight: .semibold))
                .foregroundStyle(Ink.text3)
                .contentShape(Rectangle())
                .onTapGesture {
                    if isOwner {
                        choices.list.close()
                        query.wrappedValue = ""
                    } else {
                        open()
                    }
                }
                .accessibilityHidden(true)
        }
        .cellFocusRing(true)
        .floatingListKeys(
            choices.list,
            owner: key,
            picksOnReturn: { choices.picksOnReturn },
            // On to the next cell, as ⇥ would have gone: the pick is already
            // in the editor, which the move saves.
            afterTab: { edited.editor.move(.tab) },
            afterClose: { query.wrappedValue = "" }
        )
        // A cell typed into from a letter opens the list narrowed to it.
        .onAppear {
            open()
            choices.filter(query.wrappedValue)
        }
        .onDisappear {
            if isOwner { choices.list.close() }
        }
        .onChange(of: query.wrappedValue) { _, new in
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

    private var isOwner: Bool { choices.list.owner == AnyHashable(key) }

    /// The envelope's name; an archived one still has it.
    private var name: String? {
        edited.flowId.wrappedValue.map { store.envelopeName($0) }
    }

    private func open() {
        choices.open(
            owner: key,
            envelopes: EnvelopeChoiceModel.envelopes(in: store),
            current: edited.flowId.wrappedValue
        ) { picked in
            edited.flowId.wrappedValue = picked
            query.wrappedValue = ""
        }
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
            if list.items.isEmpty {
                Text(String(localized: "No envelope matches \u{201C}\(choices.query)\u{201D}"))
                    .foregroundStyle(Ink.text3)
                    .floatingListRow(active: false)
            } else if list.items.count > Self.visibleRows {
                ScrollViewReader { proxy in
                    ScrollView {
                        entries
                    }
                    .scrollIndicators(.never)
                    .frame(height: CGFloat(Self.visibleRows) * FloatingList.rowHeight)
                    .onChange(of: list.selection, initial: true) { _, index in
                        guard list.items.indices.contains(index) else { return }
                        proxy.scrollTo(list.items[index].id)
                    }
                }
            } else {
                entries
            }
            FloatingListFooter()
        }
        .floatingList()
    }

    private var list: FloatingListModel<EnvelopeChoice> { choices.list }

    private var entries: some View {
        VStack(spacing: 0) {
            ForEach(Array(list.items.enumerated()), id: \.element.id) { index, choice in
                row(index: index, choice: choice)
                    .id(choice.id)
            }
        }
    }

    private func row(index: Int, choice: EnvelopeChoice) -> some View {
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
        .floatingListEntry(list, at: index, label: choice.name, selected: current)
    }
}
