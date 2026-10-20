import SwiftUI
import SparagneCore

/// One entry of the list under a CATEGORY cell: a category, reached by its
/// name or by one of its aliases.
struct CategoryCandidate: Identifiable, Equatable, Sendable {
    let categoryId: Uuid
    /// What the cell gets when the entry is picked: always the category's own
    /// name, so an alias typed files the row where the alias points.
    let name: String
    /// The alias the text matched, shown as `alias → name`; `nil` when the
    /// name itself matched.
    let alias: String?
    /// Used in the last 90 days (`AppStore.recentCategoryIds`): the list says
    /// "recent" beside it, which is why it came first.
    var isRecent = false

    var id: String { alias.map { "\(categoryId)/\($0)" } ?? categoryId }
}

/// The ranking behind the list: which categories match what was typed, best
/// first.
///
/// Tiers first: the name from its start, then an alias from its start, then
/// the name anywhere, then an alias anywhere; a category shows once, at its
/// best tier. Within a tier, the categories used lately come first, most
/// recent first, then the rest alphabetically. Case and accents never count:
/// `citta` finds "Città".
enum CategoryCompletion {
    /// Enough to choose from without turning the cell into a menu.
    static let limit = 8

    /// `recent` is `RecentUsage.categories`: ids, most recent first. System
    /// categories are never offered (blank already means Uncategorized) and
    /// neither are archived ones, which the core would refuse.
    static func candidates(
        for query: String,
        categories: [CategoryView],
        aliases: [AliasView],
        recent: [Uuid],
        limit: Int = limit
    ) -> [CategoryCandidate] {
        let needle = FloatingList.fold(query.trimmingCharacters(in: .whitespaces))
        guard !needle.isEmpty else { return [] }
        let recency = Dictionary(recent.enumerated().map { ($1, $0) }, uniquingKeysWith: min)
        let aliasesByCategory = Dictionary(grouping: aliases, by: \.categoryId)

        var ranked: [(tier: Int, candidate: CategoryCandidate)] = []
        for category in categories where !category.isSystem && !category.archived {
            let name = FloatingList.fold(category.name)
            let own = aliasesByCategory[category.id, default: []].sorted { $0.alias < $1.alias }
            let prefixAlias = own.first { FloatingList.fold($0.alias).hasPrefix(needle) }
            let innerAlias = own.first { FloatingList.fold($0.alias).contains(needle) }
            let match: (tier: Int, alias: String?)? =
                if name.hasPrefix(needle) { (0, nil) }
                else if let prefixAlias { (1, prefixAlias.alias) }
                else if name.contains(needle) { (2, nil) }
                else if let innerAlias { (3, innerAlias.alias) }
                else { nil }
            guard let match else { continue }
            let candidate = CategoryCandidate(
                categoryId: category.id,
                name: category.name,
                alias: match.alias,
                isRecent: recency[category.id] != nil
            )
            ranked.append((match.tier, candidate))
        }
        return ranked
            .sorted { lhs, rhs in
                if lhs.tier != rhs.tier { return lhs.tier < rhs.tier }
                let left = recency[lhs.candidate.categoryId] ?? Int.max
                let right = recency[rhs.candidate.categoryId] ?? Int.max
                if left != right { return left < right }
                return lhs.candidate.name.localizedStandardCompare(rhs.candidate.name) == .orderedAscending
            }
            .prefix(limit)
            .map(\.candidate)
    }
}

/// The list under a CATEGORY cell or field. One per grid or form, shared by
/// its fields; whatever holds them draws it (`hangingCategoryList`), so the
/// rows under it never clip it.
final class CategoryCompletionModel {
    let list = FloatingListModel<CategoryCandidate>()
    /// The name just picked: the field changes to it, and that change must
    /// not open the list again on its own name.
    private var picked: String?

    var isOpen: Bool { list.isOpen }

    /// The text of `owner` changed: offer what matches it now.
    func textChanged(
        _ text: String,
        owner: AnyHashable,
        store: AppStore,
        write: @escaping (String) -> Void
    ) {
        if let picked, picked == text {
            self.picked = nil
            return
        }
        picked = nil
        let ranked = CategoryCompletion.candidates(
            for: text,
            categories: store.categories,
            aliases: store.categoryAliases,
            recent: store.recentCategoryIds
        )
        guard let best = ranked.first else { return list.close() }
        // Drawn in two groups, names then aliases (`CategoryCompletionList`),
        // and the arrows walk them in that order; the highlight still starts
        // on the best match, wherever its group put it.
        let candidates = ranked.filter { $0.alias == nil } + ranked.filter { $0.alias != nil }
        list.open(for: owner) { [unowned self] chosen in
            picked = chosen.name
            write(chosen.name)
        }
        list.show(candidates, selection: candidates.firstIndex(of: best) ?? 0)
    }

    /// Whether ↩ should pick rather than commit: an entry that says exactly
    /// what is typed already has nothing to add, and the key saves the row.
    func picksOnReturn(over text: String) -> Bool {
        guard let chosen = list.selected else { return false }
        return FloatingList.fold(chosen.name) != FloatingList.fold(text.trimmingCharacters(in: .whitespaces))
    }
}

// MARK: - The cell

/// Where the open list's cell is, for the grid to draw the list at.
struct CategoryCompletionAnchor: PreferenceKey {
    static let defaultValue: Anchor<CGRect>? = nil

    static func reduce(value: inout Anchor<CGRect>?, nextValue: () -> Anchor<CGRect>?) {
        value = value ?? nextValue()
    }
}

/// The CATEGORY cell while it is typed into: typing opens the list, ↑↓ move
/// in it, ↩ and ⇥ pick, esc closes it. With the list closed every key does
/// what it does in the other cells (`cellKeys`): ↩ saves the cell, ⇥ and the
/// arrows move the cursor, esc puts the cell back as it was.
///
/// A cell typed into from a key pressed on it opens the list on that key at
/// once; one opened with ↩, F2 or a double click waits for a keystroke, so
/// ↑ ↓ still move the cursor off a category left as it is.
struct CategoryCell: View {
    let placeholder: String
    var placeholderTint: Color = Ink.text3
    let store: AppStore
    let completion: CategoryCompletionModel
    let key: CellFocus
    let edited: EditedCell

    var body: some View {
        TextField("", text: edited.text, selection: edited.selection, prompt: Text(placeholder).foregroundStyle(placeholderTint))
            .textFieldStyle(.plain)
            .font(Face.row)
            .foregroundStyle(Ink.text)
            .accessibilityLabel(RowField.category.label)
            .focused(edited.focus, equals: key)
            .cellFocusRing(true)
            .cellKeys(edited, key: key)
            // The cell is a field only while it is typed into, so every
            // change of its text is typing, or a pick. After `cellKeys`, so
            // the list hears its keys first.
            .completing(text: edited.text, owner: key, isFocused: true, store: store, completion: completion) {
                // ⇥ picks and moves on, as it would have with nothing to
                // pick: the name is already in the editor's text, which the
                // move saves.
                edited.editor.move(.tab)
            }
            // The recent categories and the aliases, as the cell becomes a
            // field.
            .task { await store.loadCategoryCompletion() }
            .onAppear {
                guard edited.typed, !edited.text.wrappedValue.isEmpty else { return }
                completion.textChanged(edited.text.wrappedValue, owner: key, store: store) { edited.text.wrappedValue = $0 }
            }
            .onDisappear {
                if completion.list.owner == AnyHashable(key) { completion.list.close() }
            }
            // The whole row's height, so the grid can hang the list from the
            // row's bottom edge rather than from the text's.
            .frame(height: Metrics.rowHeight)
            .anchorPreference(key: CategoryCompletionAnchor.self, value: .bounds) {
                completion.list.owner == AnyHashable(key) ? $0 : nil
            }
    }
}

extension View {
    /// The keys and the text watch every completing field shares: the grid's
    /// CATEGORY cell, the bulk "Set Category…" field, the Ricorrenze
    /// inspector's and the statement review's. `afterTab` runs once ⇥ has
    /// picked; the key itself is consumed, so the field decides where the
    /// caret goes. With the list closed every key is left to whatever holds
    /// the field.
    ///
    /// Only typing opens the list: text that changes while the field is not
    /// being typed into (a duplicated row landing in the empty line) does not.
    func completing(
        text: Binding<String>,
        owner: AnyHashable,
        isFocused: Bool,
        store: AppStore,
        completion: CategoryCompletionModel,
        afterTab: @escaping () -> Void = {}
    ) -> some View {
        floatingListKeys(
            completion.list,
            owner: owner,
            picksOnReturn: { completion.picksOnReturn(over: text.wrappedValue) },
            afterTab: afterTab
        )
        .onChange(of: text.wrappedValue) { _, new in
            guard isFocused else { return }
            completion.textChanged(new, owner: owner, store: store) { text.wrappedValue = $0 }
        }
    }
}

// MARK: - Outside the grid

/// A CATEGORY field in a form rather than in the grid: the Ricorrenze
/// inspector's. The box of a `FormTextField`, completing as the CATEGORY
/// cell does (`completing`): typing opens the list, ↑↓ move in it, ↩ and ⇥
/// pick, esc closes it. A name nobody has used yet still becomes a new
/// category when a row is recorded under it, as it does from a cell; the
/// list is there so that a slip of the keys does not.
///
/// The list is drawn by the form that holds the field
/// (`hangingCategoryList`), over everything else, so the form's scroll view
/// never clips it. ⇥ leaves the caret in the field after its pick: a form
/// has no next cell to send it to, and the next ⇥ moves on as it always did.
struct CategoryFormField: View {
    let label: String
    @Binding var text: String
    let store: AppStore
    let completion: CategoryCompletionModel

    @FocusState private var focused: Bool
    @Environment(\.formMetrics) private var metrics

    /// The one field of its form: the model is the form's own.
    private static let owner = "form"

    var body: some View {
        FormBox(focused: metrics.showsFocus && focused) {
            // An empty prompt rather than none, as `FormTextField` has it:
            // the label column already says what the field is.
            TextField(label, text: $text, prompt: Text(verbatim: ""))
                .textFieldStyle(.plain)
                .labelsHidden()
                .focused($focused)
                .accessibilityLabel(label)
                .completing(text: $text, owner: Self.owner, isFocused: focused, store: store, completion: completion)
        }
        .categoryListAnchor(owner: Self.owner, isFocused: focused, store: store, completion: completion)
    }
}

extension View {
    /// What a completing field outside the grid adds to `completing`: the
    /// recent categories and the aliases loaded as the field takes the caret,
    /// as a CATEGORY cell loads them; the list closed as the caret goes; and
    /// where the field is, for `hangingCategoryList` to hang the list from.
    /// On the box of a form field rather than on the text in it, so the list
    /// hangs from the box's edge.
    func categoryListAnchor(
        owner: AnyHashable,
        isFocused: Bool,
        store: AppStore,
        completion: CategoryCompletionModel
    ) -> some View {
        onChange(of: isFocused) { _, now in
            if now {
                Task { await store.loadCategoryCompletion() }
            } else if completion.list.owner == owner {
                completion.list.close()
            }
        }
        .anchorPreference(key: CategoryCompletionAnchor.self, value: .bounds) {
            completion.list.owner == owner ? $0 : nil
        }
    }

    /// The CATEGORY list hung from the field that opened it (`hangingList`),
    /// for the view that holds the grid, the form or the table.
    func hangingCategoryList(
        _ completion: CategoryCompletionModel,
        inset: CGFloat = 0,
        placed: @escaping (CGRect?) -> Void = { _ in }
    ) -> some View {
        hangingList(
            completion.list,
            anchor: CategoryCompletionAnchor.self,
            height: { CategoryCompletionList.height(for: $0) },
            inset: inset,
            placed: placed
        ) {
            CategoryCompletionList(completion: completion)
        }
    }
}

// MARK: - The list

/// The entries under the cell (`.ac`): a floating list of choices, not a
/// menu. Names first, then aliases, each group under a small upper-case
/// label; the highlighted entry is tinted with the accent rather than filled
/// with it, so its text stays legible. A click picks, the pointer highlights,
/// and the footer says which keys do the same.
struct CategoryCompletionList: View {
    let completion: CategoryCompletionModel

    /// What the list will measure, for the grid to decide whether it fits
    /// under the cell or has to open upwards.
    static func height(for candidates: [CategoryCandidate]) -> CGFloat {
        let groups = (candidates.contains { $0.alias == nil } ? 1 : 0) + (candidates.contains { $0.alias != nil } ? 1 : 0)
        return FloatingList.chrome + CGFloat(groups) * FloatingList.groupHeight
            + CGFloat(candidates.count) * FloatingList.rowHeight
    }

    var body: some View {
        let entries = Array(completion.list.items.enumerated())
        let names = entries.filter { $0.element.alias == nil }
        let aliases = entries.filter { $0.element.alias != nil }
        VStack(alignment: .leading, spacing: 0) {
            if !names.isEmpty {
                FloatingListGroup(title: String(localized: "Categories"))
                ForEach(names, id: \.element.id) { index, candidate in
                    row(index: index, candidate: candidate)
                }
            }
            if !aliases.isEmpty {
                FloatingListGroup(title: String(localized: "Aliases"))
                ForEach(aliases, id: \.element.id) { index, candidate in
                    row(index: index, candidate: candidate)
                }
            }
            FloatingListFooter()
        }
        .floatingList()
    }

    private func row(index: Int, candidate: CategoryCandidate) -> some View {
        HStack(spacing: 8) {
            if let alias = candidate.alias {
                Text(alias).foregroundStyle(Ink.text3)
                Spacer(minLength: 0)
                Text(verbatim: "\u{2192} \(candidate.name)")
                    .font(Face.small)
                    .foregroundStyle(Ink.text3)
            } else {
                Text(candidate.name).foregroundStyle(Ink.text)
                Spacer(minLength: 0)
                if candidate.isRecent {
                    Text(String(localized: "recent"))
                        .font(Face.small)
                        .foregroundStyle(Ink.text3)
                }
            }
        }
        .floatingListEntry(
            completion.list,
            at: index,
            label: candidate.alias.map { "\($0) \u{2192} \(candidate.name)" } ?? candidate.name
        )
    }
}

// MARK: - The floating list's pieces

/// What the lists hung from a grid cell share (`.ac`): the CATEGORY cell's
/// completions and the ENVELOPE cell's choices look and measure the same.
enum FloatingList {
    static let rowHeight: CGFloat = 24
    static let width: CGFloat = 244
    static let groupHeight: CGFloat = 20
    static let footerHeight: CGFloat = 32
    /// The border, the padding and the footer: what a list measures beyond
    /// its groups and rows.
    static let chrome: CGFloat = 2 + 8 + footerHeight
    /// `#18181C`: a step above the raised ground of the focused cell it
    /// hangs from, so the list reads as floating over the grid.
    static let ground = Color(hex: 0x18181C)
}

extension View {
    /// The list's card: padded, on its own ground, with a hairline border
    /// and a deep shadow, so it reads as floating over the grid.
    func floatingList() -> some View {
        let shape = RoundedRectangle(cornerRadius: 8)
        return padding(4)
            .frame(width: FloatingList.width, alignment: .leading)
            .background(FloatingList.ground, in: shape)
            .overlay(shape.strokeBorder(Ink.line2, lineWidth: 1))
            .shadow(color: .black.opacity(0.6), radius: 18, y: 14)
    }

    /// An entry: the highlighted one is tinted with the accent rather than
    /// filled with it, so its text stays legible.
    func floatingListRow(active: Bool) -> some View {
        font(Face.row)
            .lineLimit(1)
            .truncationMode(.tail)
            .padding(.horizontal, 7)
            .frame(height: FloatingList.rowHeight)
            .background(active ? Ink.accent.opacity(0.14) : Color.clear, in: RoundedRectangle(cornerRadius: 5))
            .contentShape(Rectangle())
    }
}

/// A group's small upper-case label.
struct FloatingListGroup: View {
    let title: String

    var body: some View {
        Text(title.uppercased())
            .font(Face.ui(10, .semibold))
            .tracking(0.5)
            .foregroundStyle(Ink.text3)
            .padding(.horizontal, 7)
            .padding(.bottom, 2)
            .frame(height: FloatingList.groupHeight, alignment: .bottomLeading)
            .accessibilityAddTraits(.isHeader)
    }
}

/// The keys the cell takes while its list is open.
struct FloatingListFooter: View {
    var body: some View {
        VStack(spacing: 0) {
            Hairline()
            HStack(spacing: 10) {
                KeyHint(key: "\u{2191}\u{2193}", label: String(localized: "scroll"))
                KeyHint(key: "\u{21E5}", label: String(localized: "select"))
                KeyHint(key: "esc", label: String(localized: "close"))
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 7)
            .padding(.top, 6)
        }
        .padding(.top, 4)
        .frame(height: FloatingList.footerHeight, alignment: .top)
        .accessibilityElement(children: .combine)
    }
}

/// A key cap and what it does, as the hint lines of the completion list and
/// of the ⌘K panel write them: `[esc] close`.
struct KeyHint: View {
    let key: String
    let label: String
    var font: Font = Face.small

    var body: some View {
        HStack(spacing: 5) {
            KeyCap(text: key)
            Text(label)
                .font(font)
                .foregroundStyle(Ink.text3)
        }
        .lineLimit(1)
        .fixedSize()
    }
}
