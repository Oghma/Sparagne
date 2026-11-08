import CoreGraphics

/// Where the cash-flow Sankey's nodes and ribbons sit in a given size: pure
/// geometry, tested on its own, so the card only draws.
///
/// Prepared as a contract: `make` places nothing until it is written.
struct SankeyLayout: Equatable, Sendable {
    /// One link's band: the slice of its source node's right edge it leaves
    /// from and the slice of its target node's left edge it arrives at.
    struct Ribbon: Identifiable, Equatable, Sendable {
        let id: String
        let source: CGRect
        let target: CGRect
    }

    /// Node id → its bar.
    let nodes: [String: CGRect]
    let ribbons: [Ribbon]

    static func make(_ sankey: CashFlowSankey, in size: CGSize) -> SankeyLayout {
        SankeyLayout(nodes: [:], ribbons: [])
    }
}
