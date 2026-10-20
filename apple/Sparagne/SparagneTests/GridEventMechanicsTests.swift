import SparagneCore
import SwiftUI
import Testing

@testable import Sparagne

/// Two things the Mastro's cell-by-cell editing stands on that the grid's
/// own tests (`LedgerGridKeyTests`) take for granted: of two key handlers
/// around a field, the one applied last hears a key first, which is how an
/// open list takes its keys before the grid; and ⇥ and ⇧⇥ pass through the
/// CATEGORY and ENVELOPE cells while their lists are closed.
///
/// Keys posted to the app's queue reach SwiftUI's `onKeyPress` and the field
/// editor as typed ones do (`HostedWindow`). ⇧⇥ comes as U+0019, not as ⇥
/// with ⇧.
@Suite(.serialized)
struct GridEventMechanicsTests {
    @Test("Of two key handlers around a field, the one applied last hears the key first")
    func handlerOrder() async throws {
        let log = HandlerLog()
        let host = HostedWindow(HandlerOrderView(log: log))
        defer { host.close() }
        await host.settle { host.fieldEditor != nil }

        host.post(.tab)
        host.post(.enter)
        await host.settle { log.heard.count == 2 }
        #expect(log.heard == ["outer \u{9}", "outer \u{D}"])
        // The inner handlers, and the field's own ↩, never heard them.
        #expect(!log.submitted)
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
        await host.settle { probe.envelopes.list.isOpen }
        #expect(probe.envelopes.list.isOpen)
        host.post(.escape)
        await host.settle { !probe.envelopes.list.isOpen }
        host.post(.tab)
        await host.settle { probe.moves.count == 3 }
        host.post(.backTab)
        await host.settle { probe.moves.count == 4 }
        #expect(probe.moves == ["tab", "backTab", "tab", "backTab"])
        #expect(probe.focus == CellFocus(row: nil, field: .flow))
    }
}

// MARK: - The order of key handlers

@Observable
private final class HandlerLog {
    var heard: [String] = []
    var submitted = false
}

private struct HandlerOrderView: View {
    let log: HandlerLog

    @FocusState private var focused: Bool
    @State private var text = ""

    var body: some View {
        TextField("", text: $text)
            .focused($focused)
            .onKeyPress(keys: [.tab, .return]) { press in
                log.heard.append("inner \(press.characters)")
                return .handled
            }
            .onSubmit { log.submitted = true }
            .onKeyPress(phases: .down) { press in
                log.heard.append("outer \(press.characters)")
                return .handled
            }
            .frame(width: 200)
            .frame(width: 900, height: 420)
            .task {
                try? await Task.sleep(for: .milliseconds(100))
                focused = true
            }
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
        let edited = EditedCell(
            editor: store.ledgerEditor,
            focus: $focus,
            selection: $caret,
            typed: false,
            keys: { press in
                guard let move = GridKey.move(for: press), move == .tab || move == .backTab else { return .ignored }
                probe.moves.append(move == .tab ? "tab" : "backTab")
                return .handled
            },
            appeared: { _ in }
        )
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
