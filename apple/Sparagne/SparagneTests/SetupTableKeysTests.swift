import AppKit
import SparagneCore
import SwiftUI
import Testing

@testable import Sparagne

/// The Setup tables and the allocation plan in a window, driven as a person
/// drives them (`HostedWindow`): esc puts the stored value back, leaving the
/// cell saves it, the row's own context menu taking the focus does not, and
/// ⌥↑ ⌥↓ move the plan's open line.
@Suite(.serialized)
struct SetupTableKeysTests {
    enum Table: String, CaseIterable, CustomTestStringConvertible {
        case wallets, envelopes, categories, plan

        var testDescription: String { rawValue }
    }

    @Test("esc puts the stored value back, and leaving the row afterwards saves nothing", arguments: Table.allCases)
    func escape(_ table: Table) async throws {
        let sheet = try await TableUnderTest(table)
        defer { sheet.close() }

        await sheet.type(sheet.typed, into: sheet.edited)
        sheet.host.post(.escape)
        await sheet.host.settle { sheet.host.fieldEditor == nil }
        #expect(sheet.host.fieldEditor == nil)

        sheet.host.postClick(at: sheet.point(sheet.other))
        await sheet.settle()
        #expect(sheet.stored() == sheet.original)
    }

    @Test("Leaving the cell for another one, or for nothing with the pointer off the row, saves it", arguments: Table.allCases)
    func leaving(_ table: Table) async throws {
        let sheet = try await TableUnderTest(table)
        defer { sheet.close() }

        await sheet.type(sheet.typed, into: sheet.edited)
        sheet.host.postClick(at: sheet.point(sheet.other))
        await sheet.settle { sheet.stored() == sheet.typedValue }
        #expect(sheet.stored() == sheet.typedValue)
        // Off the line clicked, so the next click opens the row from no caret.
        sheet.host.post(.escape)
        await sheet.host.settle { sheet.host.fieldEditor == nil }

        await sheet.type(sheet.typedAgain, into: sheet.edited)
        sheet.host.hover(at: sheet.hoverPoint(sheet.other))
        sheet.host.window.makeFirstResponder(nil)
        await sheet.settle { sheet.stored() == sheet.typedAgainValue }
        #expect(sheet.stored() == sheet.typedAgainValue)
    }

    @Test("The row's context menu taking the focus keeps the row open on what was typed", arguments: Table.allCases)
    func contextMenu(_ table: Table) async throws {
        let sheet = try await TableUnderTest(table)
        defer { sheet.close() }

        await sheet.type(sheet.typed, into: sheet.edited)
        // What opening the menu does: the field resigns, the focus goes
        // nowhere, the pointer is over the row.
        sheet.host.hover(at: sheet.hoverPoint(sheet.edited))
        sheet.host.window.makeFirstResponder(nil)
        await sheet.settle()
        #expect(sheet.stored() == sheet.original)

        sheet.host.postClick(at: sheet.point(sheet.other))
        await sheet.settle { sheet.stored() == sheet.typedValue }
        #expect(sheet.stored() == sheet.typedValue)
    }

    @Test("⌥↑ and ⌥↓ move the plan's open line, which stays open")
    func optionArrowsMoveTheLine() async throws {
        let sheet = try await TableUnderTest(.plan)
        defer { sheet.close() }
        let order = sheet.store.allocationPlan?.lines.map(\.flowId) ?? []
        #expect(order.count == 2)

        sheet.host.postClick(at: sheet.point(1))
        await sheet.host.settle { sheet.host.fieldEditor != nil }
        sheet.host.post(.up, modifiers: .option)
        await sheet.settle { sheet.store.allocationPlan?.lines.map(\.flowId) == order.reversed() }
        #expect(sheet.store.allocationPlan?.lines.map(\.flowId) == order.reversed())
        #expect(sheet.host.fieldEditor != nil)

        sheet.host.post(.down, modifiers: .option)
        await sheet.settle { sheet.store.allocationPlan?.lines.map(\.flowId) == order }
        #expect(sheet.store.allocationPlan?.lines.map(\.flowId) == order)
    }
}

/// One table in a window, over a vault with something to edit in it: the
/// wallet Conto, the envelope Casa, the category Spesa, or a plan of two
/// fixed lines.
@MainActor
private final class TableUnderTest {
    let store: AppStore
    let host: HostedWindow
    /// The line edited, and another to click on to leave it.
    let edited: Int
    let other: Int
    let original: String
    let typed: String
    let typedValue: String
    let typedAgain: String
    let typedAgainValue: String
    /// The stored value of what `edited` shows.
    let stored: () -> String
    private let firstRow: CGFloat
    private let rowHeight: CGFloat
    private let x: CGFloat
    private let width: CGFloat

    init(_ table: SetupTableKeysTests.Table) async throws {
        let store = try await TestStore.make()
        self.store = store
        let view: AnyView
        switch table {
        case .wallets:
            let id = try #require(store.wallets.first).id
            edited = 0
            other = store.wallets.count
            stored = { store.wallets.first { $0.id == id }?.name ?? "" }
            view = AnyView(WalletTable(store: store))
            width = 640
        case .envelopes:
            await store.createEnvelope(name: "Casa", mode: .unlimited, allowNegative: false, openingAllocation: 0)
            let index = try #require(store.flows.firstIndex { $0.name == "Casa" })
            let id = store.flows[index].id
            edited = index
            other = store.flows.count
            stored = { store.flows.first { $0.id == id }?.name ?? "" }
            view = AnyView(EnvelopeTable(store: store))
            width = 640
        case .categories:
            await store.createCategory(name: "Spesa")
            await store.loadCategoryManagement()
            let active = store.windowCategories.filter { !$0.archived }
            let index = try #require(active.firstIndex { $0.name == "Spesa" })
            let id = active[index].id
            edited = index
            other = store.windowCategories.count
            stored = { store.windowCategories.first { $0.id == id }?.name ?? "" }
            view = AnyView(CategoryTable(store: store))
            width = 640
        case .plan:
            for name in ["Affitto", "Vacanze"] {
                await store.createEnvelope(name: name, mode: .unlimited, allowNegative: false, openingAllocation: 0)
            }
            let lines = try ["Affitto", "Vacanze"].map { name in
                AllocationLine(flowId: try #require(store.flows.first { $0.name == name }).id, rule: .fixed(amount: 80_000))
            }
            #expect(await store.saveAllocationLines(lines, schedule: ScheduleDraft(today: CoreDate.day(Date())).schedule))
            let id = lines[0].flowId
            edited = 0
            other = 1
            stored = {
                guard case .fixed(let amount)? = store.allocationPlan?.lines.first(where: { $0.flowId == id })?.rule
                else { return "" }
                return LedgerMoney.bare(amount)
            }
            view = AnyView(PlanHost(store: store))
            width = 1000
        }
        #expect(store.presentedError == nil)
        original = stored()
        if table == .plan {
            (typed, typedValue, typedAgain, typedAgainValue) = ("50", LedgerMoney.bare(5_000), "60", LedgerMoney.bare(6_000))
            // The card's heading, 40 pt with its inset, then the column
            // headings: rows of 26 pt, the Busta cell from 32 pt.
            (firstRow, rowHeight, x) = (40 + Metrics.headerHeight, 26, 80)
        } else {
            (typed, typedValue, typedAgain, typedAgainValue) = ("Nuovo", "Nuovo", "Altro", "Altro")
            // The card's inset and heading, 36 pt, then the column headings.
            (firstRow, rowHeight, x) = (36 + Metrics.headerHeight, Metrics.rowHeight, 60)
        }
        host = HostedWindow(
            view.frame(width: width).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading),
            size: CGSize(width: width, height: 480)
        )
        await host.settle(for: .milliseconds(300))
    }

    func close() { host.close() }

    /// The middle of the name, or Busta, cell of `line`.
    func point(_ line: Int) -> CGPoint {
        CGPoint(x: x, y: firstRow + CGFloat(line) * rowHeight + rowHeight / 2)
    }

    /// A point of `line` over plain text, away from its fields and menus.
    func hoverPoint(_ line: Int) -> CGPoint {
        CGPoint(x: width - 100, y: point(line).y)
    }

    /// Opens `line` with a click and types `text` over what the cell holds.
    func type(_ text: String, into line: Int) async {
        host.postClick(at: point(line))
        await host.settle { caretIsOn(line) }
        #expect(caretIsOn(line))
        host.fieldEditor?.selectAll(nil)
        host.post(typing: text)
        await host.settle { host.fieldEditor?.string == text }
        #expect(host.fieldEditor?.string == text)
    }

    /// Whether the field being typed into is on `line`: the field editor
    /// is there before the click lands, on the line left.
    private func caretIsOn(_ line: Int) -> Bool {
        guard let field = host.fieldEditor?.delegate as? NSView, let content = host.window.contentView else { return false }
        let frame = field.convert(field.bounds, to: content)
        return (frame.minY...frame.maxY).contains(point(line).y)
    }

    /// Waits for what the table sent to the core.
    func settle(until condition: () -> Bool = { false }) async {
        await host.settle(timeout: .seconds(2), until: condition)
        await store.settle()
        await host.settle()
    }
}

/// The plan's table as the Riparto tab hosts it, saving every list it sends.
private struct PlanHost: View {
    let store: AppStore
    @State private var rowOpen = false

    var body: some View {
        AllocationPlanTable(
            store: store,
            lines: store.allocationPlan?.lines ?? [],
            preview: nil,
            due: false,
            rowOpen: $rowOpen
        ) { lines in
            Task { await store.saveAllocationLines(lines, schedule: ScheduleDraft(today: CoreDate.day(Date())).schedule) }
        }
    }
}

extension HostedWindow {


    /// The pointer over `point`, as the tracking area of the hosting view
    /// hears it: a posted mouse-moved event never reaches it.
    func hover(at point: CGPoint) {
        guard let content = window.contentView else { return }
        let location = content.convert(point, to: nil)
        // Not a tooltip's (`.help`): the one that follows the pointer.
        let area = content.trackingAreas.first { $0.owner === content && $0.options.contains(.mouseMoved) }
        let enter = NSEvent.enterExitEvent(
            with: .mouseEntered,
            location: location,
            modifierFlags: [],
            timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber,
            context: nil,
            eventNumber: 0,
            // What `NSEvent.trackingArea` reads back: the hosting view
            // takes only the events of its own area.
            trackingNumber: area.map { Int(bitPattern: Unmanaged.passUnretained($0).toOpaque()) } ?? 0,
            userData: nil
        )
        if let enter { content.mouseEntered(with: enter) }
        let move = NSEvent.mouseEvent(
            with: .mouseMoved,
            location: location,
            modifierFlags: [],
            timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber,
            context: nil,
            eventNumber: 0,
            clickCount: 0,
            pressure: 0
        )
        if let move { content.mouseMoved(with: move) }
    }
}
