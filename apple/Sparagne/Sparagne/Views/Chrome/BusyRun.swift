import SwiftUI

extension Binding<Bool> {
    /// Runs `work` with the flag up, for the buttons that wait for a command
    /// in flight: a double click would send it twice, and the core would
    /// refuse the second.
    func run(_ work: @escaping @MainActor () async -> Void) {
        wrappedValue = true
        Task {
            await work()
            wrappedValue = false
        }
    }
}
