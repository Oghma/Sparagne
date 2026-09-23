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
            ranked.append((match.tier, CategoryCandidate(categoryId: category.id, name: category.name, alias: match.alias)))
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
        candidates = CategoryCompletion.candidates(
            for: text,
            categories: store.categories,
            aliases: store.categoryAliases,
            recent: store.recentCategoryIds
        )
        selection = 0
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
    let store: AppStore
    let completion: CategoryCompletionModel
    @FocusState.Binding var focus: CellFocus?
    let key: CellFocus
    let onSubmit: () -> Void

    var body: some View {
        TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .font(Face.row)
            .foregroundStyle(Ink.text)
            .focused($focus, equals: key)
            .onSubmit(onSubmit)
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

/// The entries under the cell, in the grid's own ink: a list of choices, not a
/// menu. A click picks, the pointer highlights.
struct CategoryCompletionList: View {
    let completion: CategoryCompletionModel

    static let rowHeight: CGFloat = 22
    static let width: CGFloat = 240

    /// What the list will measure, for the grid to decide whether it fits
    /// under the cell or has to open upwards.
    static func height(for count: Int) -> CGFloat { CGFloat(count) * rowHeight + 2 }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(completion.candidates.enumerated()), id: \.element.id) { index, candidate in
                row(index: index, candidate: candidate)
            }
        }
        .padding(.vertical, 1)
        .frame(width: Self.width, alignment: .leading)
        .background(Ink.panel)
        .overlay(Rectangle().strokeBorder(Ink.accent, lineWidth: 1))
        .shadow(color: .black.opacity(0.5), radius: 8, y: 4)
    }

    private func row(index: Int, candidate: CategoryCandidate) -> some View {
        let active = index == completion.selection
        return HStack(spacing: 6) {
            if let alias = candidate.alias {
                Text(alias).foregroundStyle(active ? Ink.bg : Ink.dim)
                Text("\u{2192}").foregroundStyle(active ? Ink.bg : Ink.dim)
            }
            Text(candidate.name).foregroundStyle(active ? Ink.bg : Ink.text)
            Spacer(minLength: 0)
        }
        .font(Face.row)
        .lineLimit(1)
        .truncationMode(.tail)
        .padding(.horizontal, 8)
        .frame(height: Self.rowHeight)
        .background(active ? Ink.accent : Color.clear)
        .contentShape(Rectangle())
        .onHover { inside in
            if inside { completion.highlight(index) }
        }
        .onTapGesture {
            completion.highlight(index)
            completion.pick()
        }
    }
}
