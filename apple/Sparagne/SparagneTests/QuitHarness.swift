import AppKit
import Testing

@testable import Sparagne

/// The app's delegate over `store`, asked to quit the way ⌘Q asks it. Its
/// deferred reply is caught here: replying to a termination nobody asked
/// for would end the test host.
struct QuitHarness {
    let delegate = AppDelegate()

    init(store: AppStore) {
        delegate.store = store
    }

    /// What the app answers at once: `.terminateNow`, or `.terminateCancel`
    /// for a cell that refuses to be saved. A deferred reply is an issue.
    func answer(sourceLocation: SourceLocation = #_sourceLocation) -> NSApplication.TerminateReply {
        delegate.replyToTermination = { _ in
            Issue.record("no deferred reply was asked for", sourceLocation: sourceLocation)
        }
        return delegate.applicationShouldTerminate(NSApplication.shared)
    }

    /// Asks with something still to write: the app answers that it will
    /// quit later, and this returns its reply once the writes are done. An
    /// answer at once is an issue, and returns false.
    func deferredReply(sourceLocation: SourceLocation = #_sourceLocation) async -> Bool {
        let replied = await withCheckedContinuation { (continuation: CheckedContinuation<Bool, Never>) in
            delegate.replyToTermination = { continuation.resume(returning: $0) }
            let answer = delegate.applicationShouldTerminate(NSApplication.shared)
            guard answer == .terminateLater else {
                Issue.record("the quit did not wait for the writes: \(answer)", sourceLocation: sourceLocation)
                delegate.replyToTermination = { _ in }
                continuation.resume(returning: false)
                return
            }
        }
        // Spent: a second reply would resume the continuation twice.
        delegate.replyToTermination = { _ in
            Issue.record("no deferred reply was asked for", sourceLocation: sourceLocation)
        }
        return replied
    }
}
