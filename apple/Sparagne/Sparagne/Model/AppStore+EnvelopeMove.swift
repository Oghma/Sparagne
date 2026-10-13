import Foundation
import SparagneCore

/// "Move between envelopes", the one-off sheet of the Riparto tab
/// (`EnvelopeMoveSheet`).
extension AppStore {
    /// What every envelope holds at the end of `day` and how low and how
    /// high it goes from then on, by envelope: the figures a move dated that
    /// day is checked against (`EnvelopeMoveBalance`). `nil` when the core
    /// cannot say, with its error on the alert.
    func envelopeBalances(on day: NaiveDate, calendar: Calendar = .current) async -> [Uuid: EnvelopeMoveBalance]? {
        guard let vault = currentVault,
              let start = CoreDate.localDay(day, timeZone: calendar.timeZone),
              let end = calendar.date(byAdding: .day, value: 1, to: start)
        else { return nil }
        do {
            let ranges = try await core.flowBalanceRanges(vaultId: vault.id, from: CoreDate.utcString(end))
            return Dictionary(
                ranges.map { ($0.flowId, EnvelopeMoveBalance($0, timeZone: calendar.timeZone)) },
                uniquingKeysWith: { first, _ in first }
            )
        } catch {
            report(error)
            return nil
        }
    }

    /// Every line of `draft` as a transfer out of its source, in one batch,
    /// so either every envelope gets its amount or none does, on this Mac:
    /// on a shared vault the server still judges each transfer on its own,
    /// and one it refuses comes back among the refused changes. Checked again
    /// here against the envelopes as they are now and as they were from the
    /// draft's day on, since a sync may have changed a balance since the
    /// sheet last drew. Dated on the draft's day at the time of the click, as
    /// a row moved to another day keeps its time. Not a step of Edit ▸ Undo:
    /// the transfers are rows of the ledger, deleted there like any other.
    ///
    /// Says whether it went through, for the sheet to close; a refusal is
    /// already on the alert. A vault that is only read is refused before the
    /// core is asked anything.
    func moveBetweenEnvelopes(_ draft: EnvelopeMoveDraft, now: Date = Date()) async -> Bool {
        guard let vault = currentVault, !refusedAsReadOnly(),
              let balances = await envelopeBalances(on: draft.day)
        else { return false }
        let occurredAt = Self.stamp(day: CoreDate.localDay(draft.day) ?? now, likeTimeOf: now)
        let commands = draft.commands(flows: flows, currency: currency, balances: balances, occurredAt: occurredAt)
        guard !commands.isEmpty else { return false }
        return await apply(commands, in: vault.id)
    }
}
