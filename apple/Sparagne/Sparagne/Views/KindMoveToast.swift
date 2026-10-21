import SwiftUI

/// The toast after a kind change took rows off the list on screen, whose Mostra goes to them.
///
/// No Annulla: the change is one undo step, and ⌘Z takes it back as it takes
/// back any edit, toast or not.
struct KindMoveToast: View {
    let notice: KindMoveNotice
    let store: AppStore

    var body: some View {
        CountdownToast(
            text: Self.text(notice.direction, count: notice.count),
            undoTitle: String(localized: "Show"),
            progress: notice.progress(at:)
        ) {
            store.showMovedRow(notice)
        }
        .task(id: notice.id) {
            // What is left of the window: a toast drawn again, after another
            // toast stepped aside, does not start it over.
            let left = max(notice.deadline.timeIntervalSinceNow, 0)
            guard (try? await Task.sleep(for: .seconds(left))) != nil else { return }
            store.dismissKindMoveNotice(notice.id)
        }
    }

    /// "Spostata in Entrate", or for a selection "3 righe spostate in Uscite":
    /// the list the rows went to, named as the filter bar's switch names it.
    static func text(
        _ direction: LedgerDirection,
        count: Int,
        bundle: Bundle = .main,
        locale: Locale = .autoupdatingCurrent
    ) -> String {
        switch direction {
        case .income: String(localized: "\(count) rows moved to Income", bundle: bundle, locale: locale)
        case .expenses: String(localized: "\(count) rows moved to Expenses", bundle: bundle, locale: locale)
        }
    }
}
