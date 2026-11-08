import SwiftUI

/// The RIEPILOGO's «Flusso di cassa» card: who spent what, for the month on
/// screen or the year so far.
///
/// Prepared as a contract: it draws nothing until it is written.
struct CashFlowSankeyCard: View {
    /// The month on screen, `nil` when it earned and spent nothing.
    let month: CashFlowSankey?
    /// January up to the month on screen.
    let year: CashFlowSankey?
    /// The month on screen, for the hint.
    let upTo: MonthKey

    var body: some View {
        EmptyView()
    }
}
