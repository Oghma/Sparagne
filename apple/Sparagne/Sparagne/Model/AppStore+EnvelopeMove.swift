import Foundation
import SparagneCore

/// "Move between envelopes", the one-off sheet of the Riparto tab
/// (`EnvelopeMoveSheet`).
extension AppStore {
    /// Every line of `draft` as a transfer out of its source, in one batch,
    /// so either every envelope gets its amount or none does, on this Mac:
    /// on a shared vault the server still judges each transfer on its own,
    /// and one it refuses comes back among the refused changes. Checked again
    /// here against the envelopes as they are now, since a sync may have
    /// changed a balance since the sheet last drew. Dated on the draft's day
    /// at the time of the click, as a row moved to another day keeps its
    /// time. Not a step of Edit ▸ Undo: the transfers are rows of the
    /// ledger, deleted there like any other.
    ///
    /// Says whether it went through, for the sheet to close; a refusal is
    /// already on the alert.
    func moveBetweenEnvelopes(_ draft: EnvelopeMoveDraft, now: Date = Date()) async -> Bool {
        guard let vault = currentVault else { return false }
        let occurredAt = Self.stamp(day: CoreDate.localDay(draft.day) ?? now, likeTimeOf: now)
        let commands = draft.commands(flows: flows, currency: currency, occurredAt: occurredAt)
        guard !commands.isEmpty else { return false }
        return await apply(commands, in: vault.id)
    }
}
