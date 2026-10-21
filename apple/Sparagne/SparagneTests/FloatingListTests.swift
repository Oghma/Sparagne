import SwiftUI
import Testing

@testable import Sparagne

/// The state the floating lists share: the arrows wrapping round, and what
/// each key does to an open list, without a window.
struct FloatingListTests {
    /// A list of three open for the field `"cell"`, writing picks into
    /// `picked`.
    private final class Fixture {
        let list = FloatingListModel<String>()
        var picked: [String] = []

        init(_ items: [String] = ["Casa", "Bar", "Spesa"], selection: Int = 0) {
            list.open(for: "cell") { [unowned self] in self.picked.append($0) }
            list.show(items, selection: selection)
        }
    }

    @Test("The index wraps round both ways, however far it moves")
    func wrapFormula() {
        #expect(FloatingList.wrapped(0, by: 1, count: 3) == 1)
        #expect(FloatingList.wrapped(2, by: 1, count: 3) == 0)
        #expect(FloatingList.wrapped(0, by: -1, count: 3) == 2)
        #expect(FloatingList.wrapped(1, by: 7, count: 3) == 2)
        #expect(FloatingList.wrapped(1, by: -7, count: 3) == 0)
        #expect(FloatingList.wrapped(0, by: -1, count: 1) == 0)
    }

    @Test("The arrows walk the entries round; with none they do nothing")
    func moving() {
        let fixture = Fixture()
        fixture.list.move(by: -1)
        #expect(fixture.list.selected == "Spesa")
        fixture.list.move(by: 1)
        #expect(fixture.list.selected == "Casa")

        let empty = Fixture([])
        empty.list.move(by: 1)
        #expect(empty.list.selection == 0)
        #expect(empty.list.selected == nil)
    }

    @Test("The pointer highlights an entry, never one past the end")
    func highlighting() {
        let fixture = Fixture()
        fixture.list.highlight(2)
        #expect(fixture.list.selected == "Spesa")
        fixture.list.highlight(3)
        #expect(fixture.list.selection == 2)
    }

    @Test("A pick writes the highlighted entry and closes the list; a closed list picks nothing")
    func picking() {
        let fixture = Fixture(selection: 1)
        #expect(fixture.list.pick())
        #expect(fixture.picked == ["Bar"])
        #expect(!fixture.list.isOpen)
        #expect(fixture.list.items.isEmpty)
        #expect(!fixture.list.pick())
        #expect(fixture.picked == ["Bar"])
    }

    @Test("Keys from another field, or with the list closed, are left alone")
    func otherOwner() {
        let fixture = Fixture()
        for key in FloatingList.Key.allCases {
            #expect(fixture.list.handle(key, from: "other") == .ignored)
        }
        #expect(fixture.list.isOpen)
        #expect(fixture.list.selection == 0)

        fixture.list.close()
        #expect(fixture.list.handle(.down, from: "cell") == .ignored)
    }

    @Test("↑ and ↓ move in the list")
    func arrows() {
        let fixture = Fixture()
        #expect(fixture.list.handle(.up, from: "cell") == .handled)
        #expect(fixture.list.selected == "Spesa")
        #expect(fixture.list.handle(.down, from: "cell") == .handled)
        #expect(fixture.list.selected == "Casa")
    }

    @Test("↩ picks when the pick changes something, and otherwise closes the list and lets the key save")
    func returnKey() {
        let picking = Fixture(selection: 2)
        var closed = false
        #expect(picking.list.handle(.return, from: "cell", picksOnReturn: { true }, afterClose: { closed = true }) == .handled)
        #expect(picking.picked == ["Spesa"])
        #expect(!closed)

        let saving = Fixture()
        #expect(saving.list.handle(.return, from: "cell", picksOnReturn: { false }, afterClose: { closed = true }) == .ignored)
        #expect(saving.picked.isEmpty)
        #expect(!saving.list.isOpen)
        #expect(closed)
    }

    @Test("⇥ picks and then moves on; with nothing to pick the key goes on")
    func tabKey() {
        let fixture = Fixture(selection: 1)
        var moved = 0
        #expect(fixture.list.handle(.tab, from: "cell", afterTab: { moved += 1 }) == .handled)
        #expect(fixture.picked == ["Bar"])
        #expect(moved == 1)

        let empty = Fixture([])
        #expect(empty.list.handle(.tab, from: "cell", afterTab: { moved += 1 }) == .ignored)
        #expect(moved == 1)
    }

    @Test("esc closes the list without a pick")
    func escapeKey() {
        let fixture = Fixture()
        var closed = false
        #expect(fixture.list.handle(.escape, from: "cell", afterClose: { closed = true }) == .handled)
        #expect(!fixture.list.isOpen)
        #expect(fixture.picked.isEmpty)
        #expect(closed)
    }
}
