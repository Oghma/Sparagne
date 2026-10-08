import Foundation
import SparagneCore

/// One change to the ledger that Edit ▸ Undo can take back: what undoing it
/// writes and what redoing it writes.
struct LedgerStep: Sendable {
    /// What the Edit menu puts after "Undo" and "Redo", localized.
    let name: String
    let undo: @MainActor @Sendable () async -> Void
    let redo: @MainActor @Sendable () async -> Void

    /// The same step as the redo stack sees it.
    var reversed: LedgerStep { LedgerStep(name: name, undo: redo, redo: undo) }
}

/// A patch for one row: an edited row is one, a bulk re-categorize one per row.
struct RowPatch: Equatable, Sendable {
    let id: Uuid
    let patch: TransactionPatch
}

/// Undo and redo for the ledger, kept on the window's `UndoManager` so the
/// standard Edit ▸ Undo ⌘Z and Redo ⇧⌘Z reach rows as well as text.
///
/// The manager is the window's, the same one the text fields' editors type
/// into, so the two histories interleave in the order things happened: ⌘Z in a
/// cell being typed into takes back the typing first. A step is recorded only
/// after its write went through, never while a cell is open, so it never lands
/// between two keystrokes.
///
/// The writes are asynchronous and the manager is not: an undo handler must
/// register its redo before it returns. So the handler registers the reversed
/// step straight away and queues the write; the queue runs one write at a
/// time, in the order the user pressed the keys.
///
/// Every step has its own target, carrying the rows it touches. A void that
/// reaches the core cannot be taken back (there is no un-void), and it also
/// makes every step about its rows impossible, so `forget(rows:)` takes those
/// off both stacks rather than leave an Undo that can only fail.
final class LedgerHistory {
    /// What a step is registered under. The manager does not retain it, so the
    /// handler does, for as long as the step is on a stack.
    final class Target {
        /// Mutable because redoing an added row adds it under a new id.
        var rows: Set<Uuid>

        init(rows: Set<Uuid>) {
            self.rows = rows
        }
    }

    private weak var manager: UndoManager?
    /// The targets that may still be on a stack; the manager holds the only
    /// strong references, through the handlers.
    private var targets: [WeakTarget] = []
    /// The void waiting on the toast, if it was recorded: ⌘Z cancels it.
    private var pendingVoid: Target?

    /// The queue of undo and redo writes, and a counter that says whether a
    /// new one was queued while the last was running (`settle`).
    private var tail: Task<Void, Never>?
    private var queued = 0

    /// Hands over the window's manager. A different one (another window, or
    /// the first after none) starts from an empty history.
    func attach(_ manager: UndoManager?) {
        guard manager !== self.manager else { return }
        clear()
        self.manager = manager
    }

    // MARK: - Recording

    /// Records a change that has just been written. `build` gets the step's
    /// target, so a step whose rows change on redo can say so.
    func record(rows: Set<Uuid>, _ build: (Target) -> LedgerStep) {
        let target = Target(rows: rows)
        register(build(target), on: target)
    }

    private func register(_ step: LedgerStep, on target: Target) {
        targets.removeAll { $0.target == nil }
        if !targets.contains(where: { $0.target === target }) { targets.append(WeakTarget(target: target)) }
        group(step.name) { manager in
            manager.registerUndo(withTarget: target) { [weak self, target] _ in
                // Registered while the manager is undoing, the reversed step
                // goes on the redo stack; while redoing, back on the undo one.
                self?.register(step.reversed, on: target)
                self?.enqueue(step.undo)
            }
        }
    }

    /// Records the void the toast is counting down: ⌘Z while it is up does
    /// what the toast's Undo button does. Nothing to redo: the void was never
    /// written, and voiding again is one click away.
    func recordPendingVoid(name: String, cancel: @escaping @MainActor () -> Void) {
        forgetPendingVoid()
        guard manager != nil else { return }
        let target = Target(rows: [])
        pendingVoid = target
        group(name) { manager in
            manager.registerUndo(withTarget: target) { [weak self, target] _ in
                // Off the stack already: nothing left for `forgetPendingVoid`.
                if self?.pendingVoid === target { self?.pendingVoid = nil }
                cancel()
            }
        }
    }

    /// One registration, one group of its own. Steps are recorded after an
    /// `await`, outside any event, where the manager's grouping by event would
    /// otherwise put two of them in whatever group happens to be open, and
    /// one ⌘Z would take back both.
    private func group(_ name: String, _ register: (UndoManager) -> Void) {
        guard let manager else { return }
        manager.beginUndoGrouping()
        register(manager)
        manager.setActionName(name)
        manager.endUndoGrouping()
    }

    // MARK: - Forgetting

    /// The toast went away, by its button or because the void was written:
    /// ⌘Z has nothing left to cancel.
    func forgetPendingVoid() {
        guard let target = pendingVoid else { return }
        pendingVoid = nil
        manager?.removeAllActions(withTarget: target)
    }

    /// `rows` were voided in the core: every step about them could only be
    /// refused now, so none is offered.
    func forget(rows: Set<Uuid>) {
        // The handlers were the targets' only owners: the entries empty out
        // by themselves and `register` sweeps them.
        for target in targets.compactMap(\.target) where !target.rows.isDisjoint(with: rows) {
            manager?.removeAllActions(withTarget: target)
        }
    }

    /// Another vault is on screen: its rows are not the ones the steps name.
    func clear() {
        forgetPendingVoid()
        for target in targets.compactMap(\.target) {
            manager?.removeAllActions(withTarget: target)
        }
        targets = []
    }

    // MARK: - The queue

    private func enqueue(_ work: @escaping @MainActor @Sendable () async -> Void) {
        queued += 1
        let previous = tail
        tail = Task {
            await previous?.value
            await work()
        }
    }

    /// Waits until every queued undo and redo has been written. The window
    /// never needs this; a test that presses ⌘Z and reads the store does.
    func settle() async {
        while let tail {
            let generation = queued
            await tail.value
            if generation == queued {
                self.tail = nil
                return
            }
        }
    }
}

/// A target the history does not keep alive by itself.
private struct WeakTarget {
    weak var target: LedgerHistory.Target?
}
