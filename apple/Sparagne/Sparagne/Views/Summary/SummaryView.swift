import SwiftUI
import SparagneCore

/// The RIEPILOGO view (`docs/v2/UI.md` §2.2): the year up to the month on
/// screen, the way the household's spreadsheet reads it.
struct SummaryView: View {
    let year: YearSummary
    let store: AppStore

    var body: some View {
        // Placeholder until the summary work lands.
        Text("\(year.year)")
            .font(Face.row)
            .foregroundStyle(Ink.dim)
            .padding(10)
    }
}
