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
/// first (`docs/v2/DISTILLATO_V1.md` §3.2).
///
/// Tiers first: the name from its start, then an alias from its start, then
/// the name anywhere, then an alias anywhere; a category shows once, at its
/// best tier. Within a tier, the categories used lately come first, most
/// recent first, then the rest alphabetically. Case and accents never count:
/// `citta` finds "Città".
enum CategoryCompletion {
    /// Enough to choose from without turning the cell into a menu.
    static let limit = 8

    static func fold(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
    }

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
        let needle = fold(query.trimmingCharacters(in: .whitespaces))
        guard !needle.isEmpty else { return [] }
        let recency = Dictionary(recent.enumerated().map { ($1, $0) }, uniquingKeysWith: min)
        let aliasesByCategory = Dictionary(grouping: aliases, by: \.categoryId)

        var ranked: [(tier: Int, candidate: CategoryCandidate)] = []
        for category in categories where !category.isSystem && !category.archived {
            let name = fold(category.name)
            let own = aliasesByCategory[category.id, default: []].sorted { $0.alias < $1.alias }
            let prefixAlias = own.first { fold($0.alias).hasPrefix(needle) }
            let innerAlias = own.first { fold($0.alias).contains(needle) }
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

/// The list's state while a CATEGORY cell is being typed into: what it
/// offers, which entry the arrows are on, and the cell it belongs to.
///
/// One per grid, shared by its rows, since only one cell has the caret at a
/// time; the grid draws the list over everything else (`LedgerGrid`), so it is
/// never clipped by the rows under it. Free of views, so the ranking and the
/// arrows can be tested without a window.
@MainActor
@Observable
final class CategoryCompletionModel {
    private(set) var candidates: [CategoryCandidate] = []
    /// Index into `candidates`.
    private(set) var selection = 0
    /// The cell the list is open for; `nil` while it is closed.
    private(set) var owner: AnyHashable?

    /// Writes the picked name into the cell that opened the list.
    @ObservationIgnored private var write: ((String) -> Void)?
    /// The name just picked: the cell changes to it, and that change must not
    /// open the list again on its own name.
    @ObservationIgnored private var picked: String?

    var isOpen: Bool { !candidates.isEmpty }

    var selected: CategoryCandidate? {
        candidates.indices.contains(selection) ? candidates[selection] : nil
    }

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
        // Drawn in two groups, names then aliases (`CategoryCompletionList`),
        // and the arrows walk them in that order; the highlight still starts
        // on the best match, wherever its group put it.
        candidates = ranked.filter { $0.alias == nil } + ranked.filter { $0.alias != nil }
        selection = ranked.first.flatMap { best in candidates.firstIndex(of: best) } ?? 0
        self.owner = candidates.isEmpty ? nil : owner
        self.write = candidates.isEmpty ? nil : write
    }

    /// ↓ is `move(by: 1)`, ↑ is `move(by: -1)`; both wrap, as the palette's do.
    func move(by delta: Int) {
        guard !candidates.isEmpty else { return }
        selection = ((selection + delta) % candidates.count + candidates.count) % candidates.count
    }

    /// The pointer on an entry.
    func highlight(_ index: Int) {
        guard candidates.indices.contains(index) else { return }
        selection = index
    }

    /// ↩, ⇥ or a click: the highlighted entry goes into the cell and the list
    /// closes. `false` when nothing was open to pick from.
    @discardableResult
    func pick() -> Bool {
        guard let chosen = selected, let write else { return false }
        picked = chosen.name
        close()
        write(chosen.name)
        return true
    }

    /// esc, or the cell losing the caret.
    func close() {
        candidates = []
        selection = 0
        owner = nil
        write = nil
    }

    /// Whether ↩ should pick rather than commit: an entry that says exactly
    /// what is typed already has nothing to add, and the key saves the row.
    func picksOnReturn(over text: String) -> Bool {
        guard let chosen = selected else { return false }
        return CategoryCompletion.fold(chosen.name) != CategoryCompletion.fold(text.trimmingCharacters(in: .whitespaces))
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

/// A CATEGORY cell that completes: typing opens the list, ↑↓ move in it, ↩
/// and ⇥ pick, esc closes it. With the list closed every key does what it
/// did before: ↩ saves the row, ⇥ moves on, esc throws the draft away.
struct CategoryCell: View {
    @Binding var text: String
    let placeholder: String
    var placeholderTint: Color = Ink.text3
    let store: AppStore
    let completion: CategoryCompletionModel
    @FocusState.Binding var focus: CellFocus?
    let key: CellFocus
    let onSubmit: () -> Void

    var body: some View {
        TextField("", text: $text, prompt: Text(placeholder).foregroundStyle(placeholderTint))
            .textFieldStyle(.plain)
            .font(Face.row)
            .foregroundStyle(Ink.text)
            .accessibilityLabel(RowField.category.label)
            .focused($focus, equals: key)
            .onSubmit(onSubmit)
            .cellFocusRing(focus == key)
            .completing(text: $text, owner: key, isFocused: focus == key, store: store, completion: completion) {
                // ⇥ picks and moves on to DESCRIZIONE, as it would have. After
                // the pick has reached the field, or leaving it would write
                // back what was typed.
                Task { @MainActor in focus = CellFocus(row: key.row, field: .note) }
            }
            // The recent categories and the aliases, once the cell has the
            // caret. A click on the CATEGORY cell of a closed row focuses it
            // in the update that builds this view, where `onChange` does not
            // fire, hence `onAppear` as well.
            .onAppear {
                if focus == key { Task { await store.loadCategoryCompletion() } }
            }
            .onChange(of: focus) { old, new in
                if new == key {
                    Task { await store.loadCategoryCompletion() }
                } else if old == key, completion.owner == AnyHashable(key) {
                    completion.close()
                }
            }
            // The whole row's height, so the grid can hang the list from the
            // row's bottom edge rather than from the text's.
            .frame(height: Metrics.rowHeight)
            .anchorPreference(key: CategoryCompletionAnchor.self, value: .bounds) {
                completion.owner == AnyHashable(key) ? $0 : nil
            }
    }
}

extension View {
    /// The keys and the text watch every completing field shares: the grid's
    /// CATEGORY cells and the bulk "Set Category…" field. `afterTab` runs once
    /// ⇥ has picked; the key itself is consumed, so the field decides where
    /// the caret goes.
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
        onKeyPress(.upArrow) {
            guard completion.owner == owner else { return .ignored }
            completion.move(by: -1)
            return .handled
        }
        .onKeyPress(.downArrow) {
            guard completion.owner == owner else { return .ignored }
            completion.move(by: 1)
            return .handled
        }
        .onKeyPress(.return) {
            guard completion.owner == owner else { return .ignored }
            guard completion.picksOnReturn(over: text.wrappedValue) else {
                completion.close()
                return .ignored
            }
            completion.pick()
            return .handled
        }
        .onKeyPress(.tab) {
            guard completion.owner == owner, completion.pick() else { return .ignored }
            afterTab()
            return .handled
        }
        .onKeyPress(.escape) {
            guard completion.owner == owner else { return .ignored }
            completion.close()
            return .handled
        }
        .onChange(of: text.wrappedValue) { _, new in
            guard isFocused else { return }
            completion.textChanged(new, owner: owner, store: store) { text.wrappedValue = $0 }
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

    static let rowHeight: CGFloat = 24
    static let width: CGFloat = 244
    private static let groupHeight: CGFloat = 20
    private static let footerHeight: CGFloat = 32
    /// `#18181C`: a step above the raised ground of the focused cell it
    /// hangs from, so the list reads as floating over the grid.
    private static let ground = Color(hex: 0x18181C)

    /// What the list will measure, for the grid to decide whether it fits
    /// under the cell or has to open upwards.
    static func height(for candidates: [CategoryCandidate]) -> CGFloat {
        let groups = (candidates.contains { $0.alias == nil } ? 1 : 0) + (candidates.contains { $0.alias != nil } ? 1 : 0)
        return 2 + 8 + CGFloat(groups) * groupHeight + CGFloat(candidates.count) * rowHeight + footerHeight
    }

    var body: some View {
        let entries = Array(completion.candidates.enumerated())
        let names = entries.filter { $0.element.alias == nil }
        let aliases = entries.filter { $0.element.alias != nil }
        VStack(alignment: .leading, spacing: 0) {
            if !names.isEmpty {
                group(String(localized: "Categories"))
                ForEach(names, id: \.element.id) { index, candidate in
                    row(index: index, candidate: candidate)
                }
            }
            if !aliases.isEmpty {
                group(String(localized: "Aliases"))
                ForEach(aliases, id: \.element.id) { index, candidate in
                    row(index: index, candidate: candidate)
                }
            }
            footer
        }
        .padding(4)
        .frame(width: Self.width, alignment: .leading)
        .background(Self.ground, in: shape)
        .overlay(shape.strokeBorder(Ink.line2, lineWidth: 1))
        .shadow(color: .black.opacity(0.6), radius: 18, y: 14)
    }

    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: 8) }

    private func group(_ title: String) -> some View {
        Text(title.uppercased())
            .font(Face.ui(10, .semibold))
            .tracking(0.5)
            .foregroundStyle(Ink.text3)
            .padding(.horizontal, 7)
            .padding(.bottom, 2)
            .frame(height: Self.groupHeight, alignment: .bottomLeading)
            .accessibilityAddTraits(.isHeader)
    }

    private func row(index: Int, candidate: CategoryCandidate) -> some View {
        let active = index == completion.selection
        return HStack(spacing: 8) {
            if let alias = candidate.alias {
                Text(alias).foregroundStyle(Ink.text3)
                Spacer(minLength: 0)
                Text("\u{2192} \(candidate.name)")
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
        .font(Face.row)
        .lineLimit(1)
        .truncationMode(.tail)
        .padding(.horizontal, 7)
        .frame(height: Self.rowHeight)
        .background(active ? Ink.accent.opacity(0.14) : Color.clear, in: RoundedRectangle(cornerRadius: 5))
        .contentShape(Rectangle())
        .onHover { inside in
            if inside { completion.highlight(index) }
        }
        .onTapGesture { choose(index) }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(candidate.alias.map { "\($0) \u{2192} \(candidate.name)" } ?? candidate.name)
        .accessibilityAddTraits(active ? [.isButton, .isSelected] : .isButton)
        // A tap gesture is not an action VoiceOver can press: the row says it
        // is a button, so it has to act like one.
        .accessibilityAction { choose(index) }
    }

    /// A click on a row, or VoiceOver pressing it: that candidate goes into
    /// the cell and the list closes (`CategoryCompletionModel.pick`).
    private func choose(_ index: Int) {
        completion.highlight(index)
        completion.pick()
    }

    /// The keys the cell takes while the list is open.
    private var footer: some View {
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
        .frame(height: Self.footerHeight, alignment: .top)
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
