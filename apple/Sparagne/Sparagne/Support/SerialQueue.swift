import Foundation

/// Work run one piece at a time, in the order it was queued: the store's
/// reloads, the Mastro's cell writes, the undo and redo writes.
final class SerialQueue {
    private var tail: Task<Void, Never>?
    /// Tells `settle` whether more was queued while it waited.
    private var queued = 0

    nonisolated init() {}

    func enqueue(_ work: @escaping @MainActor () async -> Void) {
        queued += 1
        let previous = tail
        tail = Task {
            await previous?.value
            await work()
        }
    }

    /// Waits until everything queued has run, including what was queued
    /// meanwhile.
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
