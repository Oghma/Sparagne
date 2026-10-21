import SwiftUI

/// What the floating lists have in common: the CATEGORY completion, the
/// ENVELOPE select and the ⌘K palette each offer entries, keep one
/// highlighted, and put the picked one somewhere. Each keeps what is its
/// own (which entries match, when ↩ picks rather than saves) and leaves
/// the rest to one of these.
///
/// One per grid or form, shared by its fields, since only one field has the
/// caret at a time. Free of views, so the arrows and the keys can be tested
/// without a window.
@Observable
final class FloatingListModel<Item> {
    /// What the list shows, in the order the arrows walk it.
    private(set) var items: [Item] = []
    /// Index into `items`.
    private(set) var selection = 0
    /// The field the list is open for; `nil` while it is closed.
    private(set) var owner: AnyHashable?

    /// Puts the picked entry into the field that opened the list.
    @ObservationIgnored private var write: ((Item) -> Void)?

    var isOpen: Bool { owner != nil }

    var selected: Item? {
        items.indices.contains(selection) ? items[selection] : nil
    }

    /// The list now belongs to the field `owner`, and a pick goes to `write`.
    func open(for owner: AnyHashable, write: @escaping (Item) -> Void) {
        self.owner = owner
        self.write = write
    }

    /// New entries, with the highlight on `selection`: the text they match
    /// changed.
    func show(_ items: [Item], selection: Int = 0) {
        self.items = items
        self.selection = selection
    }

    /// ↓ is `move(by: 1)`, ↑ is `move(by: -1)`; both wrap, so holding one
    /// arrow walks the list round instead of sticking at an end.
    func move(by delta: Int) {
        guard !items.isEmpty else { return }
        selection = FloatingList.wrapped(selection, by: delta, count: items.count)
    }

    /// The pointer on an entry.
    func highlight(_ index: Int) {
        guard items.indices.contains(index) else { return }
        selection = index
    }

    /// ↩, ⇥ or a click: the highlighted entry goes into the field and the
    /// list closes. `false` when there was nothing to pick.
    @discardableResult
    func pick() -> Bool {
        guard let chosen = selected, let write else { return false }
        close()
        write(chosen)
        return true
    }

    /// esc, a pick, or the field losing the caret.
    func close() {
        items = []
        selection = 0
        owner = nil
        write = nil
    }

    /// What `key`, pressed in the field `owner`, does to the list: ↑ ↓ move
    /// in it, ↩ and ⇥ pick, esc closes it. A list closed, or open for
    /// another field, leaves every key to whatever holds the field.
    ///
    /// ↩ picks only if `picksOnReturn` says so (a pick that would change
    /// nothing): otherwise the list closes and the key goes on to save the
    /// field. `afterTab` runs once ⇥ has picked; the key itself is consumed,
    /// so the field decides where the caret goes. `afterClose` runs when
    /// the list closes with no pick, on esc or that ↩.
    func handle(
        _ key: FloatingList.Key,
        from owner: AnyHashable,
        picksOnReturn: () -> Bool = { true },
        afterTab: () -> Void = {},
        afterClose: () -> Void = {}
    ) -> KeyPress.Result {
        guard self.owner == owner else { return .ignored }
        switch key {
        case .up:
            move(by: -1)
            return .handled
        case .down:
            move(by: 1)
            return .handled
        case .return:
            guard picksOnReturn() else {
                close()
                afterClose()
                return .ignored
            }
            pick()
            return .handled
        case .tab:
            guard pick() else { return .ignored }
            afterTab()
            return .handled
        case .escape:
            close()
            afterClose()
            return .handled
        }
    }
}

extension FloatingList {
    /// The keys an open list answers.
    enum Key: CaseIterable {
        case up, down, `return`, tab, escape
    }

    /// `index` moved by `delta` and wrapped into `0..<count`, however far
    /// and whichever way it goes. `count` must be positive.
    static func wrapped(_ index: Int, by delta: Int, count: Int) -> Int {
        ((index + delta) % count + count) % count
    }

    /// What every list matches on: case and accents never count, so `citta`
    /// finds "Città".
    static func fold(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
    }
}

extension View {
    /// ↑ ↓ ↩ ⇥ esc for the list open from this field, as
    /// `FloatingListModel.handle` decides. After the field's own keys
    /// (`cellKeys`), so the list hears them first.
    func floatingListKeys<Item>(
        _ list: FloatingListModel<Item>,
        owner: AnyHashable,
        picksOnReturn: @escaping () -> Bool = { true },
        afterTab: @escaping () -> Void = {},
        afterClose: @escaping () -> Void = {}
    ) -> some View {
        onKeyPress(.upArrow) { list.handle(.up, from: owner) }
            .onKeyPress(.downArrow) { list.handle(.down, from: owner) }
            .onKeyPress(.return) {
                list.handle(.return, from: owner, picksOnReturn: picksOnReturn, afterClose: afterClose)
            }
            .onKeyPress(.tab) { list.handle(.tab, from: owner, afterTab: afterTab) }
            .onKeyPress(.escape) { list.handle(.escape, from: owner, afterClose: afterClose) }
    }

    /// Entry `index` of `list`: the pointer highlights it, a click or
    /// VoiceOver's press picks it. `selected` marks it for VoiceOver besides
    /// the highlight.
    func floatingListEntry<Item>(
        _ list: FloatingListModel<Item>,
        at index: Int,
        label: String,
        selected: Bool = false
    ) -> some View {
        let active = index == list.selection
        let choose: () -> Void = {
            list.highlight(index)
            list.pick()
        }
        return floatingListRow(active: active)
            .onHover { inside in
                if inside { list.highlight(index) }
            }
            .onTapGesture(perform: choose)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(label)
            .accessibilityAddTraits(active || selected ? [.isButton, .isSelected] : .isButton)
            // A tap gesture is not an action VoiceOver can press: the row
            // says it is a button, so it has to act like one.
            .accessibilityAction { choose() }
    }

    /// `content`, `list` drawn `height(list.items)` tall, hung over this view
    /// from the field `key` anchors while the list is open: from the field's
    /// bottom edge, or from its top when there is no room below, one point
    /// into the field as the canvas hangs it (`.ac`). For a view outside any
    /// scroll view, so the list is never clipped by one, no later row covers
    /// it, and a click on it is not a click on what is under it (in the
    /// grid, one that would save the cell). `inset` moves the list along the
    /// field's edge: a grid cell's starts three points in from the cell's
    /// edge rather than from its padded text.
    ///
    /// `placed` hears where the list is, in this view's coordinates, and
    /// `nil` once it is gone: the grid lets a click there through to the list
    /// rather than saving the cell the list belongs to (`LedgerGrid`).
    func hangingList<Key: PreferenceKey, Item, Content: View>(
        _ list: FloatingListModel<Item>,
        anchor key: Key.Type,
        height: @escaping ([Item]) -> CGFloat,
        inset: CGFloat,
        placed: @escaping (CGRect?) -> Void,
        @ViewBuilder content: @escaping () -> Content
    ) -> some View where Key.Value == Anchor<CGRect>? {
        overlayPreferenceValue(key) { anchor in
            if let anchor, list.isOpen {
                GeometryReader { proxy in
                    let field = proxy[anchor]
                    let height = height(list.items)
                    let fitsBelow = field.maxY + height <= proxy.size.height
                    let frame = CGRect(
                        x: field.minX + inset,
                        y: fitsBelow ? field.maxY - 1 : field.minY - height + 1,
                        width: FloatingList.width,
                        height: height
                    )
                    content()
                        .offset(x: frame.minX, y: frame.minY)
                        .onChange(of: frame, initial: true) { _, new in placed(new) }
                        .onDisappear { placed(nil) }
                }
            }
        }
    }
}
