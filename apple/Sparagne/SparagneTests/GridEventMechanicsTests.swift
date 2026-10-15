import AppKit
import SparagneCore
import SwiftUI
import Testing

@testable import Sparagne

/// The three things the Mastro's cell-by-cell editing stands on, checked on a
/// small sheet of two cells rather than on the grid, so they hold whatever
/// the grid's model does: a key typed on a selected cell starts editing it
/// with that key and the caret after it, keys typed faster than the field
/// takes the caret included; a click is seen before it lands, and placed
/// against the frame of the cell being edited; ⇥ and ⇧⇥ inside a field are
/// the sheet's own moves, not the window's focus loop.
///
/// Keys posted to the app's queue reach SwiftUI's `onKeyPress` and the field
/// editor as typed ones do; clicks reach the local monitors and, through a
/// hosting view that takes the first click (`HostedWindow`), the tap
/// gestures, with `NSApp.currentEvent` saying how many clicks and which
/// modifiers. The ⌫ key comes to a handler for any key as U+007F, which is
/// neither `.delete` nor `.deleteForward`; ⇧⇥ comes as U+0019, not as ⇥ with
/// ⇧.
@Suite(.serialized)
struct GridEventMechanicsTests {
    @Test("A key typed on a selected cell edits it with that key and the caret after it, and a fast second key lands too")
    func typingReplaces() async throws {
        let sheet = ProtoSheet()
        let host = HostedWindow(ProtoSheetView(sheet: sheet))
        defer { host.close() }
        await host.settle()

        host.postClick(at: ProtoSheetView.center(of: 0))
        await host.settle { sheet.cursor == 0 }
        #expect(sheet.cursor == 0)
        #expect(!sheet.editing)

        // Two keys back to back: the second is dispatched before the field
        // exists, so it reaches the sheet's keys as well, which append.
        host.post(typing: "29")
        await host.settle { host.fieldEditor != nil && sheet.cells[0] == "29" }
        #expect(sheet.cells[0] == "29")
        #expect(sheet.editing)
        #expect(host.fieldEditor?.string == "29")
        #expect(host.fieldSelection == NSRange(location: 2, length: 0))

        // Once the field has the caret, keys go to it, after what is there.
        host.post(typing: "5")
        await host.settle { sheet.cells[0] == "295" }
        #expect(sheet.cells[0] == "295")

        // One key, then the field: the key is the cell's whole text and the
        // caret sits after it, not around it, so the next key does not
        // replace it.
        host.postClick(at: ProtoSheetView.center(of: 1))
        await host.settle { sheet.cursor == 1 && !sheet.editing }
        host.post(typing: "7")
        await host.settle { host.fieldEditor?.string == "7" }
        #expect(host.fieldSelection == NSRange(location: 1, length: 0))
        host.post(typing: "4")
        await host.settle { sheet.cells[1] == "74" }
        #expect(sheet.cells[1] == "74")
    }

    /// Without `placesCaret`, a second key 10 to 20 ms after the first found
    /// the field holding the first one selected, and replaced it.
    @Test("However soon the second key follows the first, it is added after it", arguments: Array(0...40))
    func secondKeyAtAnyMoment(delay: Int) async throws {
        let sheet = ProtoSheet()
        let host = HostedWindow(ProtoSheetView(sheet: sheet))
        defer { host.close() }
        await host.settle()
        host.postClick(at: ProtoSheetView.center(of: 0))
        await host.settle { sheet.cursor == 0 }

        host.post(typing: "2")
        try await Task.sleep(for: .milliseconds(delay))
        host.post(typing: "9")
        await host.settle { sheet.cells[0] == "29" && host.fieldSelection == NSRange(location: 2, length: 0) }
        #expect(sheet.cells[0] == "29")
        #expect(host.fieldSelection == NSRange(location: 2, length: 0))
    }

    @Test("A click is seen before it lands, and the frames tell the edited cell, the sheet's cells and the rest apart")
    func clicksOutside() async throws {
        let sheet = ProtoSheet()
        let host = HostedWindow(ProtoSheetView(sheet: sheet))
        defer { host.close() }
        await host.settle()

        host.postClick(at: ProtoSheetView.center(of: 0))
        await host.settle { sheet.cursor == 0 }
        host.post(typing: "x")
        await host.settle { host.fieldEditor != nil }

        // On the cell being edited: the caret moves, nothing is saved.
        host.postClick(at: ProtoSheetView.center(of: 0))
        await host.settle()
        #expect(sheet.editing)
        #expect(sheet.outsideClicks == 0)

        // On another cell: that cell's own click selects it. The click does
        // not take the caret from the field on its way: a cell is no view
        // the keyboard can go to.
        host.postClick(at: ProtoSheetView.center(of: 1))
        await host.settle { sheet.cursor == 1 }
        #expect(sheet.outsideClicks == 0)
        #expect(sheet.focusLosses == 0)
        #expect(!sheet.editing)
        #expect(sheet.taps.last?.cell == 1)

        // Outside the cells: the monitor finishes the edit, and the click
        // still lands where it was aimed.
        host.post(typing: "y")
        await host.settle { host.fieldEditor != nil }
        host.postClick(at: ProtoSheetView.search)
        await host.settle { !sheet.editing }
        #expect(sheet.outsideClicks == 1)
        #expect(sheet.focusLosses == 0)
        #expect(sheet.cursor == nil)
        #expect(sheet.cells[1] == "y")
        #expect(sheet.searchFocused)

        // A second click of a double click says so to the tap's handler.
        host.postDoubleClick(at: ProtoSheetView.center(of: 0))
        await host.settle { sheet.editing }
        #expect(sheet.taps.suffix(2).map(\.count) == [1, 2])
        #expect(sheet.editing)
        #expect(sheet.cursor == 0)

        // The modifiers of a click are the click's own.
        host.postClick(at: ProtoSheetView.center(of: 1), modifiers: .command)
        await host.settle { sheet.taps.last?.command == true }
        #expect(sheet.taps.last?.command == true)
    }

    @Test("⇥ and ⇧⇥ in a field are the sheet's moves, not the window's focus loop; the sheet's keys reach it while no field has the caret")
    func tabsInAField() async throws {
        let sheet = ProtoSheet()
        let host = HostedWindow(ProtoSheetView(sheet: sheet))
        defer { host.close() }
        await host.settle()

        host.postClick(at: ProtoSheetView.center(of: 0))
        await host.settle { sheet.cursor == 0 }
        host.post(typing: "a")
        await host.settle { host.fieldEditor != nil }
        host.post(.tab)
        await host.settle { sheet.cursor == 1 }
        #expect(sheet.moves == ["tab"])
        #expect(sheet.cursor == 1)
        // The search field, the next view in the window's loop, never got it.
        #expect(!sheet.searchFocused)

        host.post(typing: "b")
        await host.settle { host.fieldEditor != nil && sheet.editing }
        host.post(.backTab)
        await host.settle { sheet.cursor == 0 }
        #expect(sheet.moves == ["tab", "backTab"])
        #expect(!sheet.searchFocused)

        host.post(typing: "c")
        await host.settle { host.fieldEditor != nil && sheet.editing }
        host.post(.down)
        await host.settle { sheet.moves.count == 3 }
        #expect(sheet.moves == ["tab", "backTab", "down"])
        #expect(!sheet.editing)
        #expect(sheet.cursor == 0)

        // No field: the arrows, ↩, F2, ⌫ and esc reach the sheet's keys.
        host.post(.right)
        await host.settle { sheet.cursor == 1 }
        host.post(.left)
        await host.settle { sheet.cursor == 0 }
        #expect(sheet.moves.suffix(2) == ["right", "left"])
        host.post(.f2)
        await host.settle { sheet.editing }
        #expect(sheet.editing)
        host.post(.escape)
        await host.settle { !sheet.editing }
        host.post(.enter)
        await host.settle { sheet.editing }
        #expect(sheet.editing)
        host.post(.escape)
        await host.settle { !sheet.editing }
        host.post(.delete)
        await host.settle { sheet.editing }
        #expect(sheet.cells[0] == "")
        host.post(.escape)
        await host.settle { !sheet.editing }
        host.post(.escape)
        await host.settle { sheet.cursor == nil }
        #expect(sheet.cursor == nil)
    }

    @Test("⇥ and ⇧⇥ pass through the CATEGORY and ENVELOPE cells to the grid while their lists are closed")
    func tabsThroughTheCells() async throws {
        let defaults = try #require(UserDefaults(suiteName: "sparagne.mechanics.\(UUID().uuidString)"))
        let store = AppStore(core: try CoreActor.inMemory(author: "matteo"), defaults: defaults)
        await store.bootstrap()
        await store.createVault(name: "Casa", walletName: "Conto", openingBalance: 0)
        let probe = TabProbe()
        let host = HostedWindow(TabProbeView(store: store, probe: probe))
        defer { host.close() }
        await host.settle()

        probe.focus = CellFocus(row: nil, field: .category)
        await host.settle { host.fieldEditor != nil }
        #expect(host.fieldEditor != nil)
        host.post(.tab)
        await host.settle { probe.moves.count == 1 }
        host.post(.backTab)
        await host.settle { probe.moves.count == 2 }
        #expect(probe.moves == ["tab", "backTab"])
        #expect(probe.focus == CellFocus(row: nil, field: .category))

        // The ENVELOPE cell opens its list as it becomes a field, and there
        // ⇥ picks; esc closes it, and then ⇥ is the grid's again.
        probe.focus = CellFocus(row: nil, field: .flow)
        await host.settle { probe.envelopes.isOpen }
        #expect(probe.envelopes.isOpen)
        host.post(.escape)
        await host.settle { !probe.envelopes.isOpen }
        host.post(.tab)
        await host.settle { probe.moves.count == 3 }
        host.post(.backTab)
        await host.settle { probe.moves.count == 4 }
        #expect(probe.moves == ["tab", "backTab", "tab", "backTab"])
        #expect(probe.focus == CellFocus(row: nil, field: .flow))
    }
}

// MARK: - A sheet of two cells

/// The state of `ProtoSheetView`: which cell is selected, whether it is
/// being typed into, and what happened, for the tests to read.
@Observable
private final class ProtoSheet {
    var cursor: Int?
    var editing = false
    var cells = ["old", "other"]
    var moves: [String] = []
    var outsideClicks = 0
    var taps: [(cell: Int, count: Int, command: Bool)] = []
    var searchFocused = false
    /// The field lost the caret while it was still being typed into: what
    /// the grid treats as the keyboard going elsewhere, and saves.
    var focusLosses = 0

    func select(_ cell: Int) {
        cursor = cell
        editing = false
    }

    func edit() {
        editing = cursor != nil
    }

    func type(_ characters: String) {
        guard let cursor else { return }
        if editing {
            cells[cursor] += characters
        } else {
            cells[cursor] = characters
            editing = true
        }
    }

    func move(_ name: String) {
        moves.append(name)
        guard let cursor else { return }
        editing = false
        switch name {
        case "tab", "right": self.cursor = min(cursor + 1, 1)
        case "backTab", "left": self.cursor = max(cursor - 1, 0)
        default: break
        }
    }

    func cancel() {
        if editing { editing = false } else { cursor = nil }
    }

    func finish() {
        editing = false
        cursor = nil
    }
}

/// Two cells side by side and a search field away from them, each at a known
/// place: the cells 160 points wide from (20, 40), 24 tall.
private struct ProtoSheetView: View {
    let sheet: ProtoSheet

    @FocusState private var fieldFocused: Bool
    @FocusState private var keysFocused: Bool
    @FocusState private var searchFocused: Bool
    @State private var selection: TextSelection?
    @State private var cellsFrame = CGRect.zero
    @State private var editedFrame = CGRect.zero
    @State private var search = ""
    /// Editing has just started. The field selects all it holds as it takes
    /// the caret, and the update that puts the caret back after it can come
    /// a pass later than the next key: until the caret is there, keys are
    /// appended here rather than given to the field to replace what it has
    /// selected.
    @State private var placesCaret = false

    private var endOfCell: TextSelection {
        TextSelection(insertionPoint: sheet.cells[sheet.cursor ?? 0].endIndex)
    }

    static func center(of cell: Int) -> CGPoint { CGPoint(x: 20 + 160 * CGFloat(cell) + 80, y: 52) }
    static let search = CGPoint(x: 500, y: 200)

    var body: some View {
        ZStack(alignment: .topLeading) {
            Color.clear
            HStack(spacing: 0) {
                cell(0)
                cell(1)
            }
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .named("sheet")) } action: { cellsFrame = $0 }
            .offset(x: 20, y: 40)
            TextField("Search", text: $search)
                .focused($searchFocused)
                .frame(width: 160)
                .offset(x: 420, y: 188)
        }
        .frame(width: 900, height: 420)
        .coordinateSpace(.named("sheet"))
        .background { keys }
        .background {
            ClickWatcher { point in
                guard sheet.editing, !editedFrame.contains(point), !cellsFrame.contains(point) else { return }
                sheet.outsideClicks += 1
                sheet.finish()
            }
        }
        .onChange(of: sheet.editing) { _, now in
            if now {
                placesCaret = true
                selection = endOfCell
                fieldFocused = true
            } else if sheet.cursor != nil {
                keysFocused = true
            }
        }
        .onChange(of: sheet.cursor) { _, new in
            if new != nil, !sheet.editing { keysFocused = true }
        }
        .onChange(of: searchFocused) { _, now in sheet.searchFocused = now }
        .onChange(of: fieldFocused) { was, now in
            if was, !now, sheet.editing { sheet.focusLosses += 1 }
        }
        // The field selects all it holds as it takes the caret: the caret
        // goes back after the keys typed so far.
        .onChange(of: fieldFocused) { _, now in
            if now, placesCaret { selection = endOfCell }
        }
    }

    @ViewBuilder
    private func cell(_ index: Int) -> some View {
        Group {
            if sheet.editing, sheet.cursor == index {
                TextField("", text: Bindable(sheet).cells[index], selection: $selection)
                    .textFieldStyle(.plain)
                    .focused($fieldFocused)
                    .onKeyPress(keys: [.tab, KeyEquivalent("\u{19}"), .upArrow, .downArrow]) { press in
                        switch press.key {
                        case .tab: sheet.move(press.modifiers.contains(.shift) ? "backTab" : "tab")
                        case KeyEquivalent("\u{19}"): sheet.move("backTab")
                        case .upArrow: sheet.move("up")
                        default: sheet.move("down")
                        }
                        return .handled
                    }
                    .onKeyPress(.escape) {
                        sheet.cancel()
                        return .handled
                    }
                    .onKeyPress(phases: .down) { press in
                        guard placesCaret, Self.isPrintable(press) else { return .ignored }
                        if selection == endOfCell {
                            placesCaret = false
                            return .ignored
                        }
                        sheet.type(press.characters)
                        selection = endOfCell
                        return .handled
                    }
                    .onGeometryChange(for: CGRect.self) { $0.frame(in: .named("sheet")) } action: { editedFrame = $0 }
            } else {
                Text(sheet.cells[index])
                    .overlay {
                        if sheet.cursor == index { Rectangle().stroke(.orange) }
                    }
            }
        }
        .frame(width: 160, height: 24, alignment: .leading)
        .contentShape(Rectangle())
        .onTapGesture {
            let event = NSApp.currentEvent
            let count = event?.clickCount ?? 1
            sheet.taps.append((index, count, event?.modifierFlags.contains(.command) ?? false))
            guard !(sheet.editing && sheet.cursor == index) else { return }
            sheet.select(index)
            if count >= 2 { sheet.edit() }
        }
    }

    private var keys: some View {
        Color.clear
            .focusable()
            .focusEffectDisabled()
            .focused($keysFocused)
            .onKeyPress(phases: .down) { press in
                switch press.key {
                case .leftArrow: sheet.move("left")
                case .rightArrow: sheet.move("right")
                case .return, KeyEquivalent("\u{F705}"): sheet.edit()
                case .escape: sheet.cancel()
                // The ⌫ key sends U+007F, which a handler for any key gets as
                // it is, not as `.delete` (U+0008).
                case .delete, .deleteForward, KeyEquivalent("\u{7F}"): sheet.type("")
                default:
                    guard Self.isPrintable(press) else { return .ignored }
                    sheet.type(press.characters)
                    placesCaret = true
                    selection = endOfCell
                }
                return .handled
            }
    }

    private static func isPrintable(_ press: KeyPress) -> Bool {
        press.modifiers.isDisjoint(with: [.command, .control])
            && !press.characters.isEmpty
            && press.characters.unicodeScalars.allSatisfy { !CharacterSet.controlCharacters.contains($0) && !(0xF700...0xF8FF).contains($0.value) }
    }
}

// MARK: - The real cells

@Observable
private final class TabProbe {
    /// The cell being typed into: the only one drawn as a field.
    var focus: CellFocus?
    var moves: [String] = []
    let completion = CategoryCompletionModel()
    let envelopes = EnvelopeChoiceModel()
}

/// The grid's CATEGORY and ENVELOPE cells as the grid holds them while they
/// are typed into, with the keys they leave going to the probe as they go to
/// the grid (`EditedCell.keys`).
private struct TabProbeView: View {
    let store: AppStore
    let probe: TabProbe

    @FocusState private var focus: CellFocus?
    @State private var caret: TextSelection?

    var body: some View {
        let edited = EditedCell(editor: store.ledgerEditor, focus: $focus, selection: $caret, typed: false) { press in
            guard let move = GridKey.move(for: press), move == .tab || move == .backTab else { return .ignored }
            probe.moves.append(move == .tab ? "tab" : "backTab")
            return .handled
        }
        HStack(spacing: 0) {
            if probe.focus?.field == .category {
                CategoryCell(
                    placeholder: "",
                    store: store,
                    completion: probe.completion,
                    key: CellFocus(row: nil, field: .category),
                    edited: edited
                )
                .frame(width: 160)
            }
            if probe.focus?.field == .flow {
                EnvelopeSelectCell(
                    placeholder: "",
                    store: store,
                    choices: probe.envelopes,
                    key: CellFocus(row: nil, field: .flow),
                    edited: edited
                )
                .frame(width: 160)
            }
            TextField("Next", text: .constant(""))
                .frame(width: 160)
        }
        .frame(width: 900, height: 420, alignment: .topLeading)
        .onChange(of: probe.focus) { _, new in focus = new }
    }
}

// MARK: - The click watcher

/// Sees every left click in its window before the window dispatches it, and
/// reports where it fell in its own frame, measured from the top left as
/// SwiftUI measures: placed behind a view, the point is in that view's
/// coordinate space.
private struct ClickWatcher: NSViewRepresentable {
    let onClick: (CGPoint) -> Void

    func makeNSView(context: Context) -> ClickWatcherView {
        let view = ClickWatcherView()
        view.onClick = onClick
        return view
    }

    func updateNSView(_ view: ClickWatcherView, context: Context) {
        view.onClick = onClick
    }

    static func dismantleNSView(_ view: ClickWatcherView, coordinator: ()) {
        view.stop()
    }
}

private final class ClickWatcherView: NSView {
    var onClick: ((CGPoint) -> Void)?
    private var monitor: Any?

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        stop()
        guard window != nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            if let self, event.window === self.window {
                self.onClick?(self.convert(event.locationInWindow, from: nil))
            }
            return event
        }
    }

    func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }
}
