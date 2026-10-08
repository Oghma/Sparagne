import SwiftUI
import SparagneCore

/// "Da confermare" (`docs/v2/UI.md` §2.5): every period waiting for a
/// decision, oldest first, with Salta and Registra for each and Registra
/// tutte for the lot. A template never writes a transaction by itself; this
/// card is where the user does it.
///
/// Registra tutte sends every period listed as one batch
/// (`AppStore.executeAllDueRecurring`), so a backlog lands whole or not at
/// all. A vault the account only reads shows its periods without the
/// buttons: nothing there can be recorded or skipped. A period whose
/// template's owner left the vault can be skipped only, and Registra tutte
/// leaves it out and says so (`AppStore.ownerHasLeft`).
struct DueRecurringCard: View {
    let store: AppStore
    let today: NaiveDate

    /// A command in flight. The buttons wait for it: a double click would
    /// record the same period twice, and the core would refuse the second.
    @State private var working = false

    var body: some View {
        let periods = store.duePeriods
        let names = NameBook(snapshot: store.snapshot)
        Panel(padding: 0) {
            VStack(alignment: .leading, spacing: 0) {
                RecurringCardHeader(
                    title: String(localized: "To confirm"),
                    count: periods.isEmpty ? nil : periods.count
                ) {
                    if store.canWrite, !periods.isEmpty {
                        Button(String(localized: "Record all")) {
                            run { await store.executeAllDueRecurring() }
                        }
                        .buttonStyle(.chrome(.primary, small: true))
                        .disabled(working)
                    }
                }
                if periods.isEmpty {
                    Text(String(localized: "Nothing to confirm"))
                        .font(Face.ui(12))
                        .foregroundStyle(Ink.text3)
                        .padding(.bottom, 2)
                } else {
                    ForEach(Array(periods.enumerated()), id: \.element.id) { index, period in
                        if index > 0 { Hairline() }
                        row(period, names: names)
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
        }
    }

    private func row(_ period: DuePeriod, names: NameBook) -> some View {
        let template = period.template
        return HStack(spacing: 10) {
            Image(systemName: "arrow.triangle.2.circlepath")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(Ink.accent)
                .frame(width: 18)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 1) {
                Text(RecurringDayText.weekday(period.date))
                    .foregroundStyle(Ink.text)
                Text(CountText.daysAgo(NaiveDay.days(from: period.date, to: today) ?? 0))
                    .font(Face.ui(11))
                    .foregroundStyle(Ink.text3)
            }
            .frame(width: 84, alignment: .leading)
            VStack(alignment: .leading, spacing: 1) {
                Text(RecurringTitle.of(template))
                    .foregroundStyle(Ink.text)
                Text(DueRecurringText.detail(template, names: names))
                    .font(Face.ui(11))
                    .foregroundStyle(Ink.text3)
            }
            .lineLimit(1)
            .frame(minWidth: 160, maxWidth: .infinity, alignment: .leading)
            Text(RecurringAmount.text(template))
                .fontWeight(.semibold)
                .foregroundStyle(RecurringAmount.color(template))
                .frame(width: 100, alignment: .trailing)
                .accessibilityLabel("\(RecurringAmount.text(template)), \(DueRecurringText.kind(template.kind))")
            if store.canWrite {
                let ownerLeft = store.ownerHasLeft(template)
                HStack(spacing: 6) {
                    Button(String(localized: "Skip")) {
                        run { await store.skipRecurring(template.id, periodDate: period.date) }
                    }
                    .buttonStyle(.chrome(.ghost, small: true))
                    // An owner who left the vault: skipping is all that can
                    // be done until the template has another one.
                    Button(String(localized: "Record")) {
                        run { await store.executeRecurring(template.id, periodDate: period.date) }
                    }
                    .buttonStyle(AccentOutlineButtonStyle())
                    .disabled(ownerLeft)
                    .help(ifAny: ownerLeft ? AppStore.ownerLeftExplanation : nil)
                }
                .disabled(working)
            }
        }
        .font(Face.ui(12.5))
        .padding(.vertical, 3)
        .frame(minHeight: 36)
        .accessibilityElement(children: .contain)
    }

    private func run(_ work: @escaping @MainActor () async -> Void) {
        working = true
        Task {
            await work()
            working = false
        }
    }
}
