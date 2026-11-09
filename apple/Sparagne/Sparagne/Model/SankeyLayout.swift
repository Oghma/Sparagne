import CoreGraphics

/// Where the cash-flow Sankey's nodes and ribbons sit in a given size: pure
/// geometry, tested on its own, so the card only draws.
///
/// One scale for the whole graph, so a ribbon is as thick where it leaves as
/// where it arrives: the column that needs the most room (its nodes and its
/// gaps) fills the height, the others are centred on it. A node too light to
/// see is raised to `minimumHeight`, and its ribbons share that height.
/// Flows go heaviest first with the savings last; the categories are ordered
/// by the height of the flows that pay them, so the ribbons cross as little
/// as they can, the folded rest last.
struct SankeyLayout: Equatable, Sendable {
    /// One link's band: the slice of its source node it leaves from and the
    /// slice of its target node it arrives at, each as wide as the node.
    struct Ribbon: Identifiable, Equatable, Sendable {
        /// The link's id.
        let id: String
        let from: String
        let to: String
        let source: CGRect
        let target: CGRect
    }

    /// Node id → its bar.
    let nodes: [String: CGRect]
    let ribbons: [Ribbon]
    /// The drawn columns left to right, each its node ids top to bottom.
    let columns: [[String]]

    static let nodeWidth: CGFloat = 8
    static let gap: CGFloat = 8
    static let minimumHeight: CGFloat = 2
    /// Left of the income column, for its labels, which sit on the left.
    static let leadingRoom: CGFloat = 180
    /// Right of the categories, for theirs.
    static let trailingRoom: CGFloat = 180

    static func make(
        _ sankey: CashFlowSankey,
        in size: CGSize,
        leadingRoom: CGFloat = SankeyLayout.leadingRoom,
        trailingRoom: CGFloat = SankeyLayout.trailingRoom
    ) -> SankeyLayout {
        let present = sankey.columns
        guard !present.isEmpty, size.width > 0, size.height > 0 else {
            return SankeyLayout(nodes: [:], ribbons: [], columns: [])
        }
        let height = size.height
        var columns = present.map { sankey.nodes(in: $0) }
        // Flows heaviest first, the savings under them; a tie keeps the
        // model's order.
        if let index = present.firstIndex(of: 2) {
            columns[index] = columns[index].enumerated()
                .sorted { lhs, rhs in
                    let left = (lhs.element.role == .savings ? 1 : 0, -lhs.element.value, lhs.offset)
                    let right = (rhs.element.role == .savings ? 1 : 0, -rhs.element.value, rhs.offset)
                    return left < right
                }
                .map(\.element)
        }
        let scale = columns.map { Self.scale($0.map(\.value), height: height) }.min() ?? 0

        // The income column's labels sit left of it; any other first column
        // has them on its right like the rest.
        let left = present.first == 0 ? min(leadingRoom, size.width) : 0
        let right = max(left, size.width - trailingRoom - nodeWidth)
        func x(_ index: Int) -> CGFloat {
            present.count == 1 ? left : left + (right - left) * CGFloat(index) / CGFloat(present.count - 1)
        }

        var rects: [String: CGRect] = [:]
        for index in columns.indices {
            if present[index] == 3 {
                columns[index] = byBarycentre(columns[index], sankey: sankey, placed: rects)
            }
            let heights = columns[index].map { max(CGFloat($0.value) * scale, minimumHeight) }
            let used = heights.reduce(0, +) + gap * CGFloat(max(heights.count - 1, 0))
            var y = max((height - used) / 2, 0)
            for (node, nodeHeight) in zip(columns[index], heights) {
                rects[node.id] = CGRect(x: x(index), y: y, width: nodeWidth, height: nodeHeight)
                y += nodeHeight + gap
            }
        }

        // Each node's edge is shared by its ribbons in the order of the
        // nodes at their other end, so two ribbons of one node never cross
        // where they meet it.
        var sources: [String: CGRect] = [:]
        var targets: [String: CGRect] = [:]
        for node in sankey.nodes {
            guard let rect = rects[node.id] else { continue }
            let outgoing = sankey.outgoing(node.id).sorted { (rects[$0.to]?.midY ?? 0) < (rects[$1.to]?.midY ?? 0) }
            for (id, slice) in slices(of: rect, by: outgoing) { sources[id] = slice }
            let incoming = sankey.incoming(node.id).sorted { (rects[$0.from]?.midY ?? 0) < (rects[$1.from]?.midY ?? 0) }
            for (id, slice) in slices(of: rect, by: incoming) { targets[id] = slice }
        }
        let ribbons = sankey.links.compactMap { link -> Ribbon? in
            guard let source = sources[link.id], let target = targets[link.id] else { return nil }
            return Ribbon(id: link.id, from: link.from, to: link.to, source: source, target: target)
        }
        return SankeyLayout(nodes: rects, ribbons: ribbons, columns: columns.map { $0.map(\.id) })
    }

    // MARK: - Labels

    /// Where the labels of one column go, top to bottom in its nodes' order:
    /// each wants to be centred on `centres[i]` and is `heights[i]` tall
    /// (its spacing included). Labels that would overlap make a group, which
    /// is centred on its bars, spreading up as much as down, so the labels of
    /// a run of thin bars stay by them; no two overlap and all stay inside
    /// `0...height`. When they cannot all fit, the labels of the lightest
    /// `weights` are left out (`nil`) until the others do.
    static func labelTops(
        centres: [CGFloat], heights: [CGFloat], weights: [Int64], within height: CGFloat
    ) -> [CGFloat?] {
        var shown = Array(centres.indices)
        while !shown.isEmpty, shown.reduce(CGFloat(0), { $0 + heights[$1] }) > height {
            // The lightest goes; of two as light, the lower one.
            guard let lightest = shown.min(by: { weights[$0] != weights[$1] ? weights[$0] < weights[$1] : $0 > $1 })
            else { break }
            shown.removeAll { $0 == lightest }
        }

        // A group of labels stacked without gaps: where it would start for
        // each member to sit on its own bar, the mean of those is the top
        // that moves them least.
        struct Group {
            var members: [Int] = []
            var size: CGFloat = 0
            var wanted: CGFloat = 0
            var top: CGFloat = 0
        }
        func place(_ group: inout Group) {
            group.top = min(max(group.wanted / CGFloat(group.members.count), 0), max(height - group.size, 0))
        }
        var groups: [Group] = []
        for index in shown {
            var group = Group(
                members: [index], size: heights[index], wanted: centres[index] - heights[index] / 2
            )
            place(&group)
            // Merged with the one above as long as the two overlap.
            while let above = groups.last, above.top + above.size > group.top + 1e-9 {
                groups.removeLast()
                var merged = above
                for member in group.members {
                    merged.wanted += centres[member] - heights[member] / 2 - merged.size
                    merged.size += heights[member]
                    merged.members.append(member)
                }
                place(&merged)
                group = merged
            }
            groups.append(group)
        }

        var result = [CGFloat?](repeating: nil, count: centres.count)
        for group in groups {
            var top = group.top
            for member in group.members {
                result[member] = top
                top += heights[member]
            }
        }
        return result
    }

    // MARK: - Helpers

    /// The points per minor unit at which `values`, each at least
    /// `minimumHeight` tall and `gap` apart, fill `height`: a node raised
    /// to the minimum leaves the others less room. 0 when even the minimums
    /// do not fit.
    static func scale(_ values: [Int64], height: CGFloat) -> CGFloat {
        let room = height - gap * CGFloat(max(values.count - 1, 0))
        let total = values.reduce(0, +)
        guard room > 0, total > 0 else { return 0 }
        var scale = room / CGFloat(total)
        var raised = 0
        // The scale only shrinks, so the raised nodes only grow in number:
        // once their count holds, the scale does.
        while true {
            let small = values.filter { CGFloat($0) * scale < minimumHeight }
            if small.count == raised { return scale }
            raised = small.count
            let rest = total - small.reduce(0, +)
            let left = room - minimumHeight * CGFloat(raised)
            guard rest > 0, left > 0 else { return 0 }
            scale = left / CGFloat(rest)
        }
    }

    /// The categories by the height of the flows that pay them, weighted by
    /// what each paid; the folded rest last, a tie to the heavier.
    private static func byBarycentre(
        _ nodes: [CashFlowSankey.Node], sankey: CashFlowSankey, placed: [String: CGRect]
    ) -> [CashFlowSankey.Node] {
        func centre(_ node: CashFlowSankey.Node) -> CGFloat {
            let links = sankey.incoming(node.id)
            let weight = links.reduce(Int64(0)) { $0 + $1.value }
            guard weight > 0 else { return .greatestFiniteMagnitude }
            let sum = links.reduce(CGFloat(0)) { $0 + CGFloat($1.value) * (placed[$1.from]?.midY ?? 0) }
            // To a thousandth of a point: two categories of one flow tie
            // whatever the rounding of their own weights, and the heavier
            // goes first.
            return (sum / CGFloat(weight) * 1000).rounded() / 1000
        }
        return nodes.enumerated()
            .map { (offset: $0.offset, node: $0.element, centre: centre($0.element)) }
            .sorted { lhs, rhs in
                if lhs.node.isOther != rhs.node.isOther { return rhs.node.isOther }
                if lhs.centre != rhs.centre { return lhs.centre < rhs.centre }
                if lhs.node.value != rhs.node.value { return lhs.node.value > rhs.node.value }
                return lhs.offset < rhs.offset
            }
            .map(\.node)
    }

    /// `rect` cut top to bottom into one slice per link, each its share of
    /// the links' total: together they cover the node's edge exactly.
    private static func slices(of rect: CGRect, by links: [CashFlowSankey.Link]) -> [(String, CGRect)] {
        let total = links.reduce(Int64(0)) { $0 + $1.value }
        guard total > 0 else { return [] }
        func edge(_ before: Int64) -> CGFloat {
            before == total ? rect.maxY : rect.minY + rect.height * CGFloat(before) / CGFloat(total)
        }
        var before: Int64 = 0
        return links.map { link in
            let top = edge(before)
            before += link.value
            return (link.id, CGRect(x: rect.minX, y: top, width: rect.width, height: edge(before) - top))
        }
    }
}
