import AppKit
import SparagneCore
import SwiftUI
import Testing
import Vision

@testable import Sparagne

/// The Mastro in a window, driven with keys and clicks as a person drives it
/// (`HostedWindow`): the cursor, typing into a cell, saving it however it is
/// left, and the rows picked around it.
@Suite(.serialized)
struct LedgerGridKeyTests {
    // MARK: - Typing into a cell

    @Test("A click selects a cell; a key typed on it replaces what it says, the keys after it are added, and ⇥ saves it and moves on")
    func typingReplacesAndTabSaves() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 0, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .note) }
        #expect(grid.editor.cursor == grid.cell(line: 0, .note))
        #expect(!grid.editor.isEditing)

        grid.host.post(typing: "panino")
        await grid.host.settle { grid.host.fieldEditor?.string == "panino" }
        #expect(grid.editor.isEditing)
        #expect(grid.editor.text == "panino")
        #expect(grid.host.fieldSelection == NSRange(location: 6, length: 0))

        grid.host.post(.tab)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .person) }
        await grid.settle()
        #expect(grid.editor.cursor == grid.cell(line: 0, .person))
        #expect(!grid.editor.isEditing)
        #expect(grid.store.rows.map(\.note) == ["panino", "latte", "caffè"])
        #expect(grid.host.fieldEditor == nil)
    }

    @Test("However soon the second key follows the first on DATA, the day is the two of them, and ↩ saves it", arguments: [0, 5, 10, 15, 20, 30])
    func dayTypedFast(delay: Int) async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 0, .date))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .date) }
        grid.host.post(typing: "2")
        try await Task.sleep(for: .milliseconds(delay))
        grid.host.post(typing: "8")
        await grid.host.settle { grid.editor.text == "28" && grid.host.fieldSelection == NSRange(location: 2, length: 0) }
        #expect(grid.editor.text == "28")
        #expect(grid.host.fieldSelection == NSRange(location: 2, length: 0))

        grid.host.post(.enter)
        await grid.host.settle { !grid.editor.isEditing }
        await grid.settle()
        #expect(grid.editor.cursor == grid.cell(line: 2, .date))
        #expect(Calendar.current.component(.day, from: try #require(grid.store.rows.last).occurredAt) == 28)
    }

    @Test("↩ and F2 type into a cell from what it says, the caret at its end; esc puts it back and keeps it selected")
    func editAndCancel() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 1, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 1, .note) }
        grid.host.post(.enter)
        await grid.host.settle { grid.host.fieldEditor?.string == "latte" }
        #expect(grid.editor.isEditing)
        #expect(grid.host.fieldSelection == NSRange(location: 5, length: 0))
        grid.host.post(typing: " intero")
        await grid.host.settle { grid.editor.text == "latte intero" }
        grid.host.post(.escape)
        await grid.host.settle { !grid.editor.isEditing }
        #expect(grid.editor.cursor == grid.cell(line: 1, .note))
        await grid.settle()
        #expect(grid.store.rows[1].note == "latte")

        grid.host.post(.f2)
        await grid.host.settle { grid.host.fieldEditor?.string == "latte" }
        #expect(grid.editor.isEditing)
        grid.host.post(.escape)
        await grid.host.settle { !grid.editor.isEditing }
        // esc on a cell only selected lets the cursor go.
        grid.host.post(.escape)
        await grid.host.settle { grid.editor.cursor == nil }
        #expect(grid.editor.cursor == nil)
    }

    @Test("A double click types into the cell; the arrows move the cursor, and ↑ ↓ from a field save it on the way")
    func doubleClickAndArrows() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postDoubleClick(at: grid.point(line: 0, .amount))
        await grid.host.settle { grid.editor.isEditing }
        #expect(grid.editor.cursor == grid.cell(line: 0, .amount))
        #expect(grid.editor.isEditing)
        grid.host.post(.escape)
        await grid.host.settle { !grid.editor.isEditing }

        grid.host.post(.down)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 1, .amount) }
        grid.host.post(.left)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 1, .person) }
        #expect(grid.editor.cursor == grid.cell(line: 1, .person))

        grid.host.post(.up)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .person) }
        grid.host.post(.right)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .amount) }
        grid.host.post(typing: "12")
        await grid.host.settle { grid.editor.text == "12" }
        grid.host.post(.down)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 1, .amount) }
        await grid.settle()
        #expect(grid.store.rows[0].absoluteAmount == 1_200)
    }

    @Test("⌫ on a selected cell empties it and types into it; a cell that cannot be typed into stays selected")
    func deleteKeyOnACell() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 2, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 2, .note) }
        grid.host.post(.delete)
        await grid.host.settle { grid.editor.isEditing }
        #expect(grid.editor.text == "")
        grid.host.post(.escape)
        await grid.host.settle { !grid.editor.isEditing }

        // A vault turned read-only: every cell can still be selected, none
        // typed into.
        grid.store.setReadOnlyVaults([try #require(grid.store.currentVault).id])
        await grid.host.settle()
        grid.host.postClick(at: grid.point(line: 2, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 2, .note) }
        grid.host.post(typing: "x")
        await grid.host.settle()
        #expect(!grid.editor.isEditing)
    }

    // MARK: - Leaving the grid

    @Test("A click outside the grid saves the cell being typed into, lets the cursor go and still lands where it was aimed")
    func clickOutsideSaves() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 0, .note))
        await grid.host.settle { grid.editor.cursor != nil }
        grid.host.post(typing: "pane nero")
        await grid.host.settle { grid.editor.text == "pane nero" }

        grid.host.postClick(at: GridUnderTest.search)
        await grid.host.settle { grid.editor.cursor == nil && grid.probe.searchFocused }
        await grid.settle()
        #expect(grid.editor.cursor == nil)
        #expect(grid.probe.searchFocused)
        #expect(grid.store.rows[0].note == "pane nero")
    }

    @Test("A click on the ruled lines under the sheet saves the cell too; one the cell refuses to be saved leaves it as typed and still lands")
    func clickBelowAndRefusal() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }
        let below = CGPoint(x: GridUnderTest.size.width / 2, y: GridUnderTest.size.height - 12)

        grid.host.postClick(at: grid.point(line: 2, .note))
        await grid.host.settle { grid.editor.cursor != nil }
        grid.host.post(typing: "cappuccino")
        await grid.host.settle { grid.editor.text == "cappuccino" }
        grid.host.postClick(at: below)
        await grid.host.settle { grid.editor.cursor == nil }
        await grid.settle()
        #expect(grid.editor.cursor == nil)
        #expect(grid.store.rows[2].note == "cappuccino")

        // An amount that is no amount: saving it is refused with the core's
        // message, and the cell stays as typed; the click still lands.
        grid.host.postClick(at: grid.point(line: 2, .amount))
        await grid.host.settle { grid.editor.cursor != nil }
        grid.host.post(typing: "abc")
        await grid.host.settle { grid.host.fieldEditor?.string == "abc" }
        grid.host.postClick(at: GridUnderTest.search)
        await grid.host.settle { grid.store.presentedError != nil && grid.probe.searchFocused }
        #expect(grid.store.presentedError != nil)
        #expect(grid.probe.searchFocused)
        #expect(grid.editor.isEditing)
        #expect(grid.editor.cursor == grid.cell(line: 2, .amount))
        #expect(grid.editor.text == "abc")

        // Back on the cell, esc puts the amount back, and nothing is left
        // to save.
        grid.host.postClick(at: grid.point(line: 2, .amount))
        await grid.host.settle { grid.host.fieldEditor?.string == "abc" && !grid.probe.searchFocused }
        grid.host.post(.escape)
        await grid.host.settle { !grid.editor.isEditing }
        #expect(grid.editor.cursor == grid.cell(line: 2, .amount))
        #expect(grid.store.rows[2].absoluteAmount == 1_000)
    }

    @Test("The search field taking the keyboard saves the cell being typed into")
    func focusTakenSaves() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 1, .note))
        await grid.host.settle { grid.editor.cursor != nil }
        grid.host.post(typing: "latte di soia")
        await grid.host.settle { grid.editor.text == "latte di soia" }

        grid.probe.focusSearch = true
        await grid.host.settle { grid.editor.cursor == nil }
        await grid.settle()
        #expect(grid.editor.cursor == nil)
        #expect(grid.probe.searchFocused)
        #expect(grid.store.rows[1].note == "latte di soia")
    }

    @Test("A click on another cell saves the one being typed into and selects the other; a click on the CATEGORY list picks")
    func clicksInsideTheGrid() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 0, .note))
        await grid.host.settle { grid.editor.cursor != nil }
        grid.host.post(typing: "focaccia")
        await grid.host.settle { grid.editor.text == "focaccia" }
        grid.host.postClick(at: grid.point(line: 1, .category))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 1, .category) }
        await grid.settle()
        #expect(grid.store.rows[0].note == "focaccia")
        #expect(!grid.editor.isEditing)

        // Typed on the CATEGORY cell, the key opens the list at once: a
        // click on its first entry picks it, and the cell is still typed
        // into.
        grid.host.post(typing: "Cas")
        await grid.host.settle { grid.editor.text == "Cas" }
        await grid.host.settle()
        grid.host.postClick(at: grid.firstCategoryEntry(below: 1))
        await grid.host.settle { grid.editor.text == "Casa" }
        #expect(grid.editor.isEditing)
        #expect(grid.editor.text == "Casa")
        grid.host.post(.enter)
        await grid.host.settle { !grid.editor.isEditing }
        await grid.settle()
        #expect(grid.store.rows[1].category == "Casa")
    }

    @Test("⇥ with the CATEGORY list open picks and moves on; with it closed ⇥ and ⇧⇥ move the cursor")
    func tabThroughCategory() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 1, .category))
        await grid.host.settle { grid.editor.cursor != nil }
        grid.host.post(typing: "Cas")
        await grid.host.settle { grid.editor.text == "Cas" }
        await grid.host.settle()
        grid.host.post(.tab)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 1, .note) }
        await grid.settle()
        #expect(grid.store.rows[1].category == "Casa")

        grid.host.post(.backTab)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 1, .category) }
        grid.host.post(.enter)
        await grid.host.settle { grid.editor.isEditing }
        // Opened with ↩, the list waits for a keystroke: ↓ moves on.
        grid.host.post(.down)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 2, .category) }
        #expect(grid.editor.cursor == grid.cell(line: 2, .category))
    }

    @Test("The ENVELOPE cell opens its list as it is typed into, narrowed by what was typed; ↩ picks, a second ↩ saves")
    func envelopeList() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 0, .flow))
        await grid.host.settle { grid.editor.cursor != nil }
        grid.host.post(typing: "vac")
        await grid.host.settle { grid.editor.text == "vac" }
        await grid.host.settle()
        grid.host.post(.enter)
        await grid.host.settle { grid.editor.flowId != nil && grid.editor.text.isEmpty }
        #expect(grid.editor.isEditing)
        grid.host.post(.enter)
        await grid.host.settle { !grid.editor.isEditing }
        await grid.settle()
        #expect(grid.store.rows[0].envelopeDisplay == "Vacanze")
    }

    // MARK: - The empty line

    @Test("The empty line is registered as the cursor leaves it with an amount, and the cursor lands on DATA of the next one")
    func newLine() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }
        let newLine = grid.store.rows.count

        grid.host.postClick(at: grid.point(line: newLine, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: newLine, .note) }
        grid.host.post(typing: "giornale")
        await grid.host.settle { grid.editor.text == "giornale" }
        grid.host.post(.tab)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: newLine, .person) }
        grid.host.post(.tab)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: newLine, .amount) }
        grid.host.post(typing: "2,5")
        await grid.host.settle { grid.editor.text == "2,5" }
        grid.host.post(.enter)
        await grid.host.settle { grid.store.rows.count == newLine + 1 }
        await grid.settle()
        #expect(grid.store.rows.map(\.note).contains("giornale"))
        #expect(grid.editor.cursor == CellFocus(row: nil, field: .date))
    }

    @Test("⌘D copies a row into the empty line, with the caret in its IMPORTO")
    func duplicate() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        NotificationCenter.default.post(name: .duplicateLastRow, object: nil)
        await grid.host.settle { grid.host.fieldEditor != nil }
        #expect(grid.editor.cursor == CellFocus(row: nil, field: .amount))
        #expect(grid.editor.newLine.note == "caffè")
        #expect(grid.host.fieldEditor != nil)
    }

    // MARK: - Rows picked

    @Test("⌘-click picks rows and hides the cursor; a plain click lets them go; esc does too")
    func pickingRows() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 0, .note))
        await grid.host.settle { grid.editor.cursor != nil }
        grid.host.postClick(at: grid.point(line: 1, .note), modifiers: .command)
        await grid.host.settle { grid.store.selection.count == 1 }
        #expect(grid.editor.cursor == nil)
        grid.host.postClick(at: grid.point(line: 2, .note), modifiers: .shift)
        await grid.host.settle { grid.store.selection.count == 2 }
        #expect(grid.store.selection.count == 2)

        grid.host.postClick(at: grid.point(line: 0, .category))
        await grid.host.settle { grid.store.selection.isEmpty }
        #expect(grid.store.selection.isEmpty)
        #expect(grid.editor.cursor == grid.cell(line: 0, .category))

        grid.host.postClick(at: grid.point(line: 1, .note), modifiers: .command)
        await grid.host.settle { grid.store.selection.count == 1 }
        grid.host.post(.escape)
        await grid.host.settle { grid.store.selection.isEmpty }
        #expect(grid.store.selection.isEmpty)
    }

    @Test("⌘⌫ deletes the row the cursor is on")
    func commandDelete() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 1, .amount))
        await grid.host.settle { grid.editor.cursor != nil }
        grid.host.post(.delete, modifiers: .command)
        await grid.host.settle { grid.store.rows.count == 2 }
        await grid.settle()
        #expect(grid.store.rows.map(\.note) == ["pane", "caffè"])
    }

    // MARK: - The caret

    @Test("A key typed after ← lands at the caret, not at the end of the cell")
    func caretMovedThenTyped() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 1, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 1, .note) }
        grid.host.post(.enter)
        await grid.host.settle {
            grid.host.fieldEditor?.string == "latte" && grid.host.fieldSelection == NSRange(location: 5, length: 0)
        }
        try await Task.sleep(for: .milliseconds(200))
        grid.host.post(.left)
        await grid.host.settle { grid.host.fieldSelection == NSRange(location: 4, length: 0) }
        #expect(grid.host.fieldSelection == NSRange(location: 4, length: 0))
        try await Task.sleep(for: .milliseconds(200))
        grid.host.post(typing: "x")
        await grid.host.settle { grid.editor.text == "lattxe" }
        #expect(grid.editor.text == "lattxe")
        #expect(grid.host.fieldEditor?.string == "lattxe")
    }

    @Test("A click inside the cell being typed into puts the caret there, and the next key lands at it")
    func clickInFieldThenTyped() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postDoubleClick(at: grid.point(line: 1, .note))
        await grid.host.settle { grid.host.fieldEditor?.string == "latte" }
        try await Task.sleep(for: .milliseconds(200))
        // Near the left edge of DESCRIZIONE: before "latte".
        var point = grid.point(line: 1, .note)
        point.x = grid.left(of: .note) + GridColumn.padding + 1
        grid.host.postClick(at: point)
        await grid.host.settle { grid.host.fieldSelection == NSRange(location: 0, length: 0) }
        #expect(grid.host.fieldSelection == NSRange(location: 0, length: 0))
        grid.host.post(typing: "x")
        await grid.host.settle { grid.editor.text == "xlatte" }
        #expect(grid.editor.text == "xlatte")
    }

    @Test("Cas and ⇥ typed fast on a selected CATEGORY cell pick Casa", arguments: [0, 20, 40])
    func fastCategoryTab(delay: Int) async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }
        await grid.store.loadCategoryCompletion()

        grid.host.postClick(at: grid.point(line: 0, .category))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .category) }
        grid.host.post(typing: "Cas")
        try await Task.sleep(for: .milliseconds(delay))
        grid.host.post(.tab)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .note) }
        await grid.settle()
        #expect(grid.store.rows[0].category == "Casa")
    }

    // MARK: - Following the cursor

    @Test("The sheet scrolls to keep the cursor in sight as ↓ takes it down the rows")
    func cursorScrolledIntoView() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }
        try await grid.addRows(40)

        grid.host.postClick(at: grid.point(line: 0, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .note) }
        for _ in 0..<30 { grid.host.post(.down) }
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 30, .note) }
        await grid.host.settle { grid.scrollOffset > 200 }
        #expect(grid.editor.cursor == grid.cell(line: 30, .note))
        // Line 30 is 720 points down a viewport shorter than 450.
        #expect(grid.scrollOffset > 200)
    }

    @Test("Keys typed on a cell the cursor took far down the sheet type into it")
    func typingOffScreen() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }
        try await grid.addRows(40)

        grid.host.postClick(at: grid.point(line: 0, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .note) }
        for _ in 0..<30 { grid.host.post(.down) }
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 30, .note) }
        let target = grid.cell(line: 30, .note)
        let before = grid.store.rows.first { $0.id == target.row }?.note
        grid.host.post(typing: "x")
        await grid.host.settle(for: .milliseconds(300))
        grid.host.post(typing: "yz")
        await grid.host.settle { grid.editor.text == "xyz" }
        await grid.settle()
        #expect(grid.editor.cursor == target)
        #expect(grid.editor.isEditing)
        #expect(grid.editor.text == "xyz")
        #expect(grid.host.fieldEditor?.string == "xyz")
        #expect(grid.store.rows.first { $0.id == target.row }?.note == before)
    }

    @Test("Scrolling the cell being typed into out of sight keeps it typed into, and the keys still reach it")
    func scrollWhileEditing() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }
        try await grid.addRows(40)

        grid.host.postClick(at: grid.point(line: 0, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .note) }
        grid.host.post(typing: "abc")
        await grid.host.settle { grid.host.fieldEditor?.string == "abc" }
        let scroll = try #require(grid.scrollView)
        let height = scroll.documentView?.bounds.height ?? 0
        scroll.contentView.scroll(to: CGPoint(x: 0, y: max(0, height - scroll.contentView.bounds.height)))
        scroll.reflectScrolledClipView(scroll.contentView)
        await grid.host.settle(for: .milliseconds(500))
        await grid.settle()
        #expect(grid.scrollOffset > 200)
        #expect(grid.editor.isEditing)
        #expect(grid.editor.cursor == grid.cell(line: 0, .note))
        #expect(grid.store.rows[0].note == "pane")

        // A key brings the row back, and the field with it.
        grid.host.post(typing: "d")
        await grid.host.settle { grid.editor.text == "abcd" && grid.host.fieldEditor?.string == "abcd" }
        #expect(grid.editor.text == "abcd")
        #expect(grid.host.fieldEditor?.string == "abcd")
        #expect(grid.scrollOffset < 24)
        grid.host.post(.enter)
        await grid.host.settle { !grid.editor.isEditing }
        await grid.settle()
        #expect(grid.store.rows[0].note == "abcd")
    }

    // MARK: - A cell just saved

    @Test("A cell just left says what it was saved as while the write is still on its way, and after it lands")
    func savedCellShownAtOnce() async throws {
        let gate = TestGate()
        let grid = try await GridUnderTest(coreProbe: gate.probe)
        defer { grid.close() }
        let pane = try #require(grid.store.rows.first)

        grid.host.postClick(at: grid.point(line: 0, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: 0, .note) }
        grid.host.post(typing: "focaccia")
        await grid.host.settle { grid.editor.text == "focaccia" }
        await gate.arm()
        grid.host.post(.tab)
        await gate.holding()
        await grid.host.settle { grid.drawnText().contains("focaccia") }

        // The store still has the row as it was loaded; the sheet draws it
        // as saved.
        #expect(grid.store.rows.first { $0.id == pane.id }?.note == "pane")
        #expect(grid.editor.cursor == CellFocus(row: pane.id, field: .person))
        let held = grid.drawnText()
        #expect(held.contains("focaccia"))
        #expect(!held.contains("pane"))

        await gate.release()
        await grid.settle()
        #expect(grid.store.rows.first { $0.id == pane.id }?.note == "focaccia")
        let landed = grid.drawnText()
        #expect(landed.contains("focaccia"))
        #expect(!landed.contains("pane"))
    }

    // MARK: - A cell that refuses

    @Test("A new line refused as the cursor leaves it hands the caret to IMPORTO with what IMPORTO says")
    func refusedRegistrationText() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }
        let newLine = grid.store.rows.count

        grid.host.postClick(at: grid.point(line: newLine, .amount))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: newLine, .amount) }
        grid.host.post(typing: "abc")
        await grid.host.settle { grid.editor.text == "abc" }
        grid.host.post(.backTab)
        await grid.host.settle { grid.editor.cursor == grid.cell(line: newLine, .person) }
        #expect(grid.editor.newLine.amount == "abc")
        grid.host.postClick(at: grid.point(line: newLine, .note))
        await grid.host.settle { grid.editor.cursor == grid.cell(line: newLine, .note) }
        grid.host.post(typing: "pizza")
        await grid.host.settle { grid.host.fieldEditor?.string == "pizza" }
        try await Task.sleep(for: .milliseconds(200))
        grid.host.post(.up)
        await grid.host.settle { grid.host.fieldEditor?.string == "abc" }
        #expect(grid.store.presentedError != nil)
        #expect(grid.editor.cursor == grid.cell(line: newLine, .amount))
        #expect(grid.editor.isEditing)
        #expect(grid.editor.text == "abc")
        #expect(grid.host.fieldEditor?.string == "abc")
        #expect(grid.editor.newLine.note == "pizza")
    }

    @Test("A field that takes the keyboard from a cell that refuses to be saved keeps it; the cell stays as typed until it is come back to")
    func refusedCellLetsTheKeyboardGo() async throws {
        let grid = try await GridUnderTest()
        defer { grid.close() }

        grid.host.postClick(at: grid.point(line: 2, .amount))
        await grid.host.settle { grid.editor.cursor != nil }
        grid.host.post(typing: "abc")
        await grid.host.settle { grid.host.fieldEditor?.string == "abc" }
        grid.probe.focusSearch = true
        await grid.host.settle { grid.store.presentedError != nil && grid.probe.searchFocused }
        await grid.host.settle(for: .milliseconds(300))
        #expect(grid.store.presentedError != nil)
        #expect(grid.probe.searchFocused)
        #expect(grid.editor.isEditing)
        #expect(grid.editor.text == "abc")

        // Back on the cell, esc puts it back.
        grid.host.postClick(at: grid.point(line: 2, .amount))
        await grid.host.settle { grid.host.fieldEditor?.string == "abc" && !grid.probe.searchFocused }
        grid.host.post(.escape)
        await grid.host.settle { !grid.editor.isEditing }
        #expect(grid.store.rows[2].absoluteAmount == 1_000)
    }
}

// MARK: - The grid in a window

/// What the window holds besides the grid: a search field over it, which
/// the tests click on or give the keyboard to.
@Observable
private final class GridProbe {
    var focusSearch = false
    var searchFocused = false
}

private struct GridHostView: View {
    let store: AppStore
    let probe: GridProbe

    @FocusState private var searchFocused: Bool
    @State private var search = ""

    var body: some View {
        VStack(spacing: 0) {
            TextField("Search", text: $search)
                .focused($searchFocused)
                .frame(width: 200, height: GridUnderTest.searchHeight)
            LedgerGrid(store: store)
        }
        .frame(width: GridUnderTest.size.width, height: GridUnderTest.size.height)
        .onChange(of: probe.focusSearch) { _, now in searchFocused = now }
        .onChange(of: searchFocused) { _, now in probe.searchFocused = now }
    }
}

/// Vault `Casa` with envelopes Spesa and Vacanze, categories Spesa and Casa,
/// and three expenses this month, pane, latte and caffè, on the 3rd, 5th and
/// 7th, 10,00 each; the grid in a window under a search field.
@MainActor
private final class GridUnderTest {
    static let size = CGSize(width: 1000, height: 480)
    static let searchHeight: CGFloat = 30
    /// The search field, centred over the grid.
    static let search = CGPoint(x: size.width / 2, y: searchHeight / 2)

    let store: AppStore
    let probe = GridProbe()
    let host: HostedWindow

    var editor: LedgerEditor { store.ledgerEditor }

    /// `coreProbe` hears every call that reaches the core (`TestGate`).
    init(coreProbe: (@Sendable () async -> Void)? = nil) async throws {
        store = try await TestStore.make(undoWindow: .seconds(60), probe: coreProbe)
        await store.createEnvelope(name: "Spesa", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        await store.createEnvelope(name: "Vacanze", mode: .unlimited, allowNegative: true, openingAllocation: 0)
        let calendar = Calendar.current
        for (day, note, category) in [(3, "pane", "Spesa"), (5, "latte", "Spesa"), (7, "caffè", "Casa")] {
            let date = try #require(
                calendar.date(from: DateComponents(year: store.month.year, month: store.month.month, day: day))
            )
            await store.addRow(day: date, flowId: nil, category: category, note: note, amount: 1_000)
        }
        #expect(store.rows.count == 3)
        host = HostedWindow(GridHostView(store: store, probe: probe), size: Self.size)
        await host.settle(for: .milliseconds(300))
    }

    func close() { host.close() }

    /// Waits for what the editor and the store have sent to the core.
    func settle() async {
        await editor.settle()
        await store.settle()
        await host.settle()
    }

    /// The cell of `line`, the stored rows first and the empty line after.
    func cell(line: Int, _ field: RowField) -> CellFocus {
        CellFocus(row: store.rows.indices.contains(line) ? store.rows[line].id : nil, field: field)
    }

    /// The middle of a cell, from the column widths the grid lays out.
    func point(line: Int, _ field: RowField) -> CGPoint {
        let columns = store.ledgerColumns
        let right = [LedgerColumn.person, .amount].map { columns.outer($0) }.reduce(GridColumn.action, +)
            + (store.showWalletColumn ? columns.outer(.wallet) : 0)
        let x: CGFloat = switch field {
        case .date: left(of: field) + columns.outer(.date) / 2
        case .flow: left(of: field) + columns.outer(.flow) / 2
        case .category: left(of: field) + columns.outer(.category) / 2
        case .note: (left(of: field) + Self.size.width - right) / 2
        case .wallet: Self.size.width - right + columns.outer(.wallet) / 2
        case .person: Self.size.width - GridColumn.action - columns.outer(.amount) - columns.outer(.person) / 2
        case .amount: Self.size.width - GridColumn.action - columns.outer(.amount) / 2
        }
        let y = Self.searchHeight + Metrics.headerHeight + CGFloat(line) * Metrics.rowHeight + Metrics.rowHeight / 2
        return CGPoint(x: x, y: y)
    }

    /// The left edge of a column; past DESCRIZIONE, the middle of the cell.
    func left(of field: RowField) -> CGFloat {
        let columns = store.ledgerColumns
        let start = GridColumn.ordinal + 2 * GridColumn.padding
        return switch field {
        case .date: start
        case .flow: start + columns.outer(.date)
        case .category: start + columns.outer(.date) + columns.outer(.flow)
        case .note: start + columns.outer(.date) + columns.outer(.flow) + columns.outer(.category)
        default: point(line: 0, field).x
        }
    }

    /// `count` more expenses on the 20th, after the three of the fixture:
    /// a sheet longer than the window.
    func addRows(_ count: Int) async throws {
        let date = try #require(
            Calendar.current.date(from: DateComponents(year: store.month.year, month: store.month.month, day: 20))
        )
        for index in 0..<count {
            await store.addRow(day: date, flowId: nil, category: "Spesa", note: "riga \(index)", amount: 100)
        }
        await settle()
        #expect(store.rows.count == 3 + count)
    }

    /// The words drawn in the window, read off a picture of it as a person
    /// would read them: what the cells show, which no accessor of the
    /// views says.
    func drawnText() -> [String] {
        guard let content = host.window.contentView,
              let picture = content.bitmapImageRepForCachingDisplay(in: content.bounds)
        else { return [] }
        content.cacheDisplay(in: content.bounds, to: picture)
        guard let image = picture.cgImage else { return [] }
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = false
        try? VNImageRequestHandler(cgImage: image).perform([request])
        return (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
    }

    /// The grid's scroll view, and how far down it is scrolled.
    var scrollView: NSScrollView? {
        func find(in view: NSView) -> NSScrollView? {
            if let scroll = view as? NSScrollView { return scroll }
            return view.subviews.lazy.compactMap(find).first
        }
        return host.window.contentView.flatMap(find)
    }

    var scrollOffset: CGFloat { scrollView?.contentView.bounds.origin.y ?? 0 }

    /// The middle of the first entry of the CATEGORY list hung under the
    /// cell of `line`: the list starts three points into the cell and a
    /// point up into it, and its first entry is under the group's label.
    func firstCategoryEntry(below line: Int) -> CGPoint {
        let rowBottom = Self.searchHeight + Metrics.headerHeight + CGFloat(line + 1) * Metrics.rowHeight
        return CGPoint(
            x: left(of: .category) + 3 + FloatingList.width / 2,
            y: rowBottom - 1 + 4 + FloatingList.groupHeight + FloatingList.rowHeight / 2
        )
    }
}
