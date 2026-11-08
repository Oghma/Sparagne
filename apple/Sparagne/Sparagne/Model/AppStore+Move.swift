import Foundation
import SparagneCore

/// "Move between flows" and "Move between wallets", the Riparto tab's one-off sheet (`MoveSheet`).
extension AppStore {
    /// The active flows or wallets a move of `side` goes between, in the
    /// pickers' order.
    func moveTargets(_ side: TargetKind) -> [MoveTarget] {
        switch side {
        case .flow: flows.map(MoveTarget.init)
        case .wallet: wallets.map(MoveTarget.init)
        }
    }

    /// What every flow, or every wallet by `side`, holds at the end of `day`
    /// and how low and how high it goes from then on, by id: the figures a
    /// move dated that day is checked against (`MoveBalance`). `nil` when
    /// the core cannot say, with its error on the alert.
    func moveBalances(_ side: TargetKind, on day: NaiveDate, calendar: Calendar = .current) async -> [Uuid: MoveBalance]? {
        guard let vault = currentVault,
              let start = CoreDate.localDay(day, timeZone: calendar.timeZone),
              let end = calendar.date(byAdding: .day, value: 1, to: start)
        else { return nil }
        do {
            let from = CoreDate.utcString(end)
            let ranges = try await core.read { try $0.balanceRanges(vaultId: vault.id, from: from, target: side) }
            return Dictionary(
                ranges.map { ($0.targetId, MoveBalance($0, timeZone: calendar.timeZone)) },
                uniquingKeysWith: { first, _ in first }
            )
        } catch {
            report(error)
            return nil
        }
    }

    /// Every line of `draft` as a transfer out of its source, in one batch,
    /// so either every flow or wallet gets its amount or none does, on this
    /// Mac: on a shared vault the server still judges each transfer on its
    /// own, and one it refuses comes back among the refused changes. Checked
    /// again here against the targets as they are now and as they were from
    /// the draft's day on, since a sync may have changed a balance since the
    /// sheet last drew. Dated on the draft's day at the time of the click, as
    /// a row moved to another day keeps its time. Not a step of Edit ▸ Undo:
    /// the transfers are rows of the ledger, deleted there like any other.
    ///
    /// Says whether it went through, for the sheet to close; a refusal is
    /// already on the alert. A vault that is only read is refused before the
    /// core is asked anything.
    func move(_ draft: MoveDraft, now: Date = Date()) async -> Bool {
        guard let vault = currentVault, !refusedAsReadOnly(),
              let balances = await moveBalances(draft.side, on: draft.day)
        else { return false }
        let occurredAt = Self.stamp(day: CoreDate.localDay(draft.day) ?? now, likeTimeOf: now)
        let commands = draft.commands(targets: moveTargets(draft.side), currency: currency, balances: balances, occurredAt: occurredAt)
        guard !commands.isEmpty else { return false }
        return await apply(commands, in: vault.id)
    }
}
