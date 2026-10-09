import SwiftUI

/// The Riparto tab: the allocation plan, the period waiting to be shared out
/// of Unallocated and the periods already decided.
///
/// A stub for now.
struct AllocationTab: View {
    let store: AppStore

    var body: some View {
        VStack {
            Spacer()
            Text(String(localized: "Allocation"))
                .font(Face.row)
                .foregroundStyle(Ink.text3)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }
}
