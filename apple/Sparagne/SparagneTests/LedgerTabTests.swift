import Testing

@testable import Sparagne

/// The sheet tabs at the bottom of the window (`docs/v2/UI.md` §2): their
/// order is the tab bar's and the View menu's ⌘1 to ⌘4, so it is checked
/// without a window.
struct LedgerTabTests {
    @Test("The tabs run Riepilogo, Mastro, Ricorrenze, Setup")
    func order() {
        #expect(LedgerTab.allCases == [.summary, .ledger, .recurring, .setup])
    }

    @Test("Every tab has a label of its own")
    func labels() {
        let labels = LedgerTab.allCases.map(\.label)
        #expect(labels.allSatisfy { !$0.isEmpty })
        #expect(Set(labels).count == labels.count)
    }

    @Test("⌘1 to ⌘4 select the tabs in the bar's order")
    func shortcuts() {
        #expect(LedgerTab(shortcut: 1) == .summary)
        #expect(LedgerTab(shortcut: 2) == .ledger)
        #expect(LedgerTab(shortcut: 3) == .recurring)
        #expect(LedgerTab(shortcut: 4) == .setup)
        for tab in LedgerTab.allCases {
            #expect(LedgerTab(shortcut: tab.shortcut) == tab)
        }
    }

    @Test("A digit outside the bar selects nothing")
    func shortcutsOutOfRange() {
        #expect(LedgerTab(shortcut: 0) == nil)
        #expect(LedgerTab(shortcut: 5) == nil)
        #expect(LedgerTab(shortcut: -1) == nil)
    }
}
