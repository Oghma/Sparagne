import Foundation
import SparagneCore

/// Changing a recurring template's kind (`RecurringKindChange`).
extension AppStore {
    /// Why the template with `templateId` cannot change kind now; `nil` when
    /// it can.
    func recurringKindRefusal(_ templateId: Uuid) async -> RecurringKindChange.Refusal? {
        .readOnly
    }

    /// Archives the template and creates its copy of the other kind, paused
    /// if it was, in one batch. Returns the copy's id; `nil` when refused,
    /// with the alert up.
    @discardableResult
    func changeRecurringKind(_ templateId: Uuid) async -> Uuid? {
        nil
    }
}
