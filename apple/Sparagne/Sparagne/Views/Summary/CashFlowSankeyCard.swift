import AppKit
import SwiftUI

/// The RIEPILOGO's «Flusso di cassa» card: who spent what, for the month on
/// screen or the year so far. The income by category flows into Entrate,
/// from there into what each flow spent and into the savings, and from each
/// flow into the categories it paid; a month that spent more than it earned
/// takes the rest from the balance instead of saving.
///
/// The graph is `CashFlowSankey` placed by `SankeyLayout`, both tested on
/// their own: this view only draws, labels and answers the pointer.
struct CashFlowSankeyCard: View {
    /// The month on screen, `nil` when it earned and spent nothing.
    let month: CashFlowSankey?
    /// January up to the month on screen.
    let year: CashFlowSankey?
    /// The month on screen, for the period in the heading.
    let upTo: MonthKey

    enum Period: String {
        case month, year
    }

    /// Kept between launches, like the Mastro's toggles.
    @AppStorage("summarySankeyPeriod") private var period: Period = .year

    static let height: CGFloat = 320

    private var sankey: CashFlowSankey? { period == .month ? month : year }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: 10) {
                header
                if let sankey {
                    SankeyGraph(sankey: sankey, name: "\(String(localized: "Cash flow")), \(periodTitle)")
                        .frame(height: Self.height)
                        // A redrawn period starts with nothing under the pointer.
                        .id(period)
                    Text(String(localized: "Figures match the cards above unless someone's refunds in a month exceed their expenses."))
                        .font(Face.label)
                        .foregroundStyle(Ink.text3)
                        .lineLimit(2)
                } else {
                    Text(period == .month
                        ? String(localized: "Nothing earned or spent this month")
                        : String(localized: "Nothing earned or spent this year"))
                        .font(Face.row)
                        .foregroundStyle(Ink.text3)
                }
            }
        }
    }

    private var header: some View {
        HStack(alignment: .center, spacing: 10) {
            SectionLabel(text: String(localized: "Cash flow"), tint: Ink.text2)
                .accessibilityAddTraits(.isHeader)
            Text(String(localized: "Who spent what; transfers between flows do not show"))
                .font(Face.label)
                .foregroundStyle(Ink.text3)
                .lineLimit(1)
                .layoutPriority(-1)
            Spacer(minLength: 8)
            Text(periodTitle)
                .font(Face.label)
                .foregroundStyle(Ink.text3)
                .lineLimit(1)
            SegmentedStrip(
                options: [Period.month, .year],
                selection: $period,
                label: { $0 == .month ? String(localized: "Month") : String(localized: "Year") },
                name: String(localized: "Period"),
                fixed: true
            )
        }
    }

    private var periodTitle: String { Self.periodTitle(period, upTo: upTo) }

    /// `"Ottobre 2026"` for the month, `"Gennaio–ottobre 2026"` for the
    /// year so far, `"Gennaio 2026"` when the two are one month.
    static func periodTitle(_ period: Period, upTo: MonthKey, locale: Locale = .autoupdatingCurrent) -> String {
        guard period == .year, upTo.month > 1 else {
            return SummaryText.monthTitle(upTo.month, year: upTo.year, locale: locale)
        }
        let january = SummaryText.capitalized(SummaryText.monthName(1, locale: locale))
        return "\(january)\u{2013}\(SummaryText.monthName(upTo.month, locale: locale)) \(upTo.year)"
    }
}

// MARK: - The graph

/// The Sankey itself: ribbons and bars on a `Canvas`, labels as text over
/// it, a tooltip on hover with the hovered ribbon or node lit and the rest
/// faded.
private struct SankeyGraph: View {
    let sankey: CashFlowSankey
    /// What VoiceOver calls the graph: the card's title and period.
    let name: String

    private let byId: [String: CashFlowSankey.Node]

    init(sankey: CashFlowSankey, name: String) {
        self.sankey = sankey
        self.name = name
        byId = Dictionary(sankey.nodes.map { ($0.id, $0) }) { first, _ in first }
    }

    private enum Target: Equatable {
        case node(String)
        case link(String)
    }

    private struct Hover: Equatable {
        let target: Target
        let point: CGPoint
    }

    @State private var hover: Hover?
    @State private var tipSize: CGSize = .zero

    private static let ribbonOpacity = 0.35
    private static let litOpacity = 0.7
    private static let fadedOpacity = 0.1
    /// One line of a label, and the room between two labels.
    private static let lineHeight: CGFloat = 14
    private static let labelSpacing: CGFloat = 2
    /// Between a bar and its label.
    private static let labelGap: CGFloat = 6
    /// Between a one-line label's name and its figure.
    private static let figureGap: CGFloat = 6

    var body: some View {
        GeometryReader { proxy in
            let layout = SankeyLayout.make(sankey, in: proxy.size)
            let lit = highlight(layout)
            ZStack(alignment: .topLeading) {
                Canvas { context, _ in
                    draw(layout, lit: lit, in: &context)
                }
                .accessibilityLabel(name)
                .accessibilityChildren {
                    // The summary first, so it is what VoiceOver reads on
                    // reaching the graph; a node at a time after it.
                    VStack {
                        ForEach(Array(spoken.enumerated()), id: \.offset) { _, element in
                            Rectangle()
                                .accessibilityLabel(element.label)
                                .accessibilityValue(element.value)
                        }
                    }
                }
                labels(layout, size: proxy.size, lit: lit)
                    .accessibilityHidden(true)
                if let hover, let content = tooltipContent(hover.target) {
                    tooltip(content, at: hover.point, size: proxy.size)
                }
            }
            .contentShape(Rectangle())
            .onContinuousHover { phase in
                switch phase {
                case .active(let point):
                    hover = Self.hit(point, layout: layout).map { Hover(target: $0, point: point) }
                case .ended:
                    hover = nil
                }
            }
        }
        // New figures under a pointer at rest (another month, a sync): what
        // it pointed at may be gone, and it has not moved to say where it is.
        .onChange(of: sankey) { hover = nil }
    }

    // MARK: - Drawing

    /// What the pointer lights: the hovered ribbon and its two ends, or the
    /// hovered node, its ribbons and the nodes at their other ends. `nil`
    /// with nothing hovered.
    private func highlight(_ layout: SankeyLayout) -> (ribbons: Set<String>, nodes: Set<String>)? {
        switch hover?.target {
        case nil:
            return nil
        case .link(let id):
            guard let ribbon = layout.ribbons.first(where: { $0.id == id }) else { return nil }
            return ([id], [ribbon.from, ribbon.to])
        case .node(let id):
            guard layout.nodes[id] != nil else { return nil }
            let touching = layout.ribbons.filter { $0.from == id || $0.to == id }
            return (Set(touching.map(\.id)), Set(touching.flatMap { [$0.from, $0.to] }).union([id]))
        }
    }

    private func draw(
        _ layout: SankeyLayout, lit: (ribbons: Set<String>, nodes: Set<String>)?, in context: inout GraphicsContext
    ) {
        // The lit ribbons last, over the faded ones they cross.
        let ordered = layout.ribbons.sorted { lhs, rhs in
            !(lit?.ribbons.contains(lhs.id) ?? false) && (lit?.ribbons.contains(rhs.id) ?? false)
        }
        for ribbon in ordered {
            let opacity: Double
            if let lit {
                opacity = lit.ribbons.contains(ribbon.id) ? Self.litOpacity : Self.fadedOpacity
            } else {
                opacity = Self.ribbonOpacity
            }
            context.fill(Self.path(ribbon), with: .color(tint(ribbon).opacity(opacity)))
        }
        for node in sankey.nodes {
            guard let rect = layout.nodes[node.id] else { continue }
            let faded = lit.map { !$0.nodes.contains(node.id) } ?? false
            context.fill(
                Path(roundedRect: rect, cornerRadius: 1),
                with: .color((node.role == .fromBalance ? Ink.negative : Ink.text2).opacity(faded ? 0.5 : 1))
            )
        }
    }

    /// A ribbon's band: two cubic curves from the source's right edge to
    /// the target's left edge, joined by the edges' slices.
    static func path(_ ribbon: SankeyLayout.Ribbon) -> Path {
        let start = ribbon.source.maxX
        let end = ribbon.target.minX
        let middle = (start + end) / 2
        var path = Path()
        path.move(to: CGPoint(x: start, y: ribbon.source.minY))
        path.addCurve(
            to: CGPoint(x: end, y: ribbon.target.minY),
            control1: CGPoint(x: middle, y: ribbon.source.minY),
            control2: CGPoint(x: middle, y: ribbon.target.minY)
        )
        path.addLine(to: CGPoint(x: end, y: ribbon.target.maxY))
        path.addCurve(
            to: CGPoint(x: start, y: ribbon.source.maxY),
            control1: CGPoint(x: middle, y: ribbon.target.maxY),
            control2: CGPoint(x: middle, y: ribbon.source.maxY)
        )
        path.closeSubpath()
        return path
    }

    /// Income and refunds blue into Entrate, the balance red, the savings
    /// green, every expense orange.
    private func tint(from: String, to: String) -> Color {
        switch (byId[from]?.role, byId[to]?.role) {
        case (.income?, _), (.refunds?, _): Ink.chartIncome
        case (.fromBalance?, _): Ink.negative
        case (_, .savings?): Ink.chartSavings
        default: Ink.chartExpense
        }
    }

    private func tint(_ ribbon: SankeyLayout.Ribbon) -> Color { tint(from: ribbon.from, to: ribbon.to) }

    /// A node's swatch in the tooltip: the colour of what reaches it, or of
    /// what leaves it for a node with nothing coming in.
    private func tint(_ node: CashFlowSankey.Node) -> Color {
        if let link = sankey.incoming(node.id).first { return tint(from: link.from, to: link.to) }
        switch node.role {
        case .fromBalance: return Ink.negative
        case .income, .refunds, .total: return Ink.chartIncome
        default: return Ink.chartExpense
        }
    }

    // MARK: - Pointer

    /// The node under `point`, a little wider than its bar so a thin one can
    /// be caught, else the topmost ribbon.
    private static func hit(_ point: CGPoint, layout: SankeyLayout) -> Target? {
        if let node = layout.nodes.first(where: { $0.value.insetBy(dx: -4, dy: -2).contains(point) }) {
            return .node(node.key)
        }
        return layout.ribbons.last { path($0).contains(point) }.map { .link($0.id) }
    }

    // MARK: - Labels

    /// Every node's name and figure: left of the income column, right of
    /// every other node. A short node's label is one line when it fits the
    /// room up to the next column, two otherwise, a tall node's always two;
    /// in a column whose labels would overlap they move apart, and those of
    /// the lightest nodes go when there is no room at all.
    private func labels(
        _ layout: SankeyLayout, size: CGSize, lit: (ribbons: Set<String>, nodes: Set<String>)?
    ) -> some View {
        let placed = placedLabels(layout, size: size)
        return ZStack(alignment: .topLeading) {
            ForEach(placed, id: \.node.id) { label in
                labelText(label.node, twoLines: label.twoLines, leading: label.leading)
                    .frame(width: max(label.width, 0), alignment: label.leading ? .trailing : .leading)
                    .offset(x: label.x, y: label.top)
                    .opacity((lit.map { $0.nodes.contains(label.node.id) } ?? true) ? 1 : 0.4)
            }
        }
        .allowsHitTesting(false)
    }

    private struct PlacedLabel {
        let node: CashFlowSankey.Node
        let twoLines: Bool
        /// Left of its bar, right-aligned.
        let leading: Bool
        let x: CGFloat
        let top: CGFloat
        let width: CGFloat
    }

    private func placedLabels(_ layout: SankeyLayout, size: CGSize) -> [PlacedLabel] {
        let firstIsIncome = sankey.columns.first == 0
        var placed: [PlacedLabel] = []
        for (index, ids) in layout.columns.enumerated() {
            let nodes = ids.compactMap { id in byId[id].flatMap { node in layout.nodes[id].map { (node, $0) } } }
            guard let bar = nodes.first?.1 else { continue }
            // The bars of a column share their x, so their labels share the
            // room: up to the next column, or to the edge.
            let leading = index == 0 && firstIsIncome
            let nextX = layout.columns.indices.contains(index + 1)
                ? layout.columns[index + 1].first.flatMap { layout.nodes[$0]?.minX } : nil
            let x = leading ? 0 : bar.maxX + Self.labelGap
            let width = leading ? bar.minX - Self.labelGap : (nextX.map { $0 - Self.labelGap } ?? size.width) - x
            let twoLines = nodes.map { node, rect in
                rect.height >= 2 * Self.lineHeight - 2 || oneLineWidth(node) > width - 2
            }
            let tops = SankeyLayout.labelTops(
                centres: nodes.map { $0.1.midY },
                heights: twoLines.map { ($0 ? 2 : 1) * Self.lineHeight + Self.labelSpacing },
                weights: nodes.map { $0.0.value },
                within: size.height
            )
            for ((node, _), (twoLines, top)) in zip(nodes, zip(twoLines, tops)) {
                guard let top else { continue }
                placed.append(
                    PlacedLabel(
                        node: node, twoLines: twoLines, leading: leading,
                        x: x, top: top + Self.labelSpacing / 2, width: width
                    )
                )
            }
        }
        return placed
    }

    @ViewBuilder
    private func labelText(_ node: CashFlowSankey.Node, twoLines: Bool, leading: Bool) -> some View {
        let name = Text(node.title)
            .font(Face.ui(11.5, .medium))
            .foregroundStyle(node.role == .fromBalance ? Ink.negative : Ink.text)
        let figure = Text(figure(node))
            .font(Face.ui(11))
            .foregroundStyle(Ink.text2)
        Group {
            if twoLines {
                VStack(alignment: leading ? .trailing : .leading, spacing: 0) {
                    name.frame(height: Self.lineHeight)
                    figure.frame(height: Self.lineHeight)
                }
            } else {
                HStack(spacing: Self.figureGap) {
                    name.layoutPriority(1)
                    figure
                }
                .frame(height: Self.lineHeight)
            }
        }
        .lineLimit(1)
        // The labels sit over the ribbons: a halo of the card's ground
        // keeps them legible.
        .shadow(color: Ink.card, radius: 1)
        .shadow(color: Ink.card, radius: 3)
    }

    /// The name and the figure side by side, as `labelText` sets them.
    private func oneLineWidth(_ node: CashFlowSankey.Node) -> CGFloat {
        let name = NSFont.monospacedDigitSystemFont(ofSize: 11.5, weight: .medium)
        let figure = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .regular)
        return (node.title as NSString).size(withAttributes: [.font: name]).width
            + Self.figureGap
            + (self.figure(node) as NSString).size(withAttributes: [.font: figure]).width
    }

    /// `"7.290,00 · 14,2%"`: what the node says and its share of the
    /// income; Entrate is the income, so it says only the figure, and with
    /// no income nothing has a share.
    private func figure(_ node: CashFlowSankey.Node) -> String {
        let amount = LedgerMoney.bare(sankey.shown(node))
        guard node.role != .total, let share = share(sankey.shown(node)) else { return amount }
        return "\(amount) \u{00B7} \(share)"
    }

    private func share(_ value: Int64) -> String? {
        sankey.shareText(value)
    }

    // MARK: - Tooltip

    /// The hovered ribbon's or node's figure and share of the income, on the
    /// palette's raised card as the income chart's tooltip, over the
    /// pointer, or under it near the top.
    private func tooltip(_ content: TipContent, at point: CGPoint, size: CGSize) -> some View {
        let x = min(max(point.x - tipSize.width / 2, 0), max(size.width - tipSize.width, 0))
        let above = point.y - tipSize.height - 14
        let y = above >= 0 ? above : point.y + 18
        return VStack(alignment: .leading, spacing: 3) {
            Text(content.title)
                .font(Face.ui(11, .semibold))
                .foregroundStyle(Ink.text2)
            tooltipRow(String(localized: "Amount"), LedgerMoney.bare(content.value), tint: content.swatch)
            if content.showsShare, let share = share(content.value) {
                tooltipRow(String(localized: "Share of income"), share, tint: content.swatch)
            }
        }
        .font(Face.ui(11.5))
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .frame(minWidth: 150)
        .background(Ink.line, in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Ink.line2, lineWidth: 1))
        .fixedSize()
        .onGeometryChange(for: CGSize.self) { $0.size } action: { tipSize = $0 }
        .offset(x: x, y: y)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private func tooltipRow(_ name: String, _ value: String, tint: Color) -> some View {
        HStack(spacing: 14) {
            HStack(spacing: 5) {
                Capsule().fill(tint).frame(width: 8, height: 2)
                Text(name).foregroundStyle(Ink.text2)
            }
            Spacer(minLength: 0)
            Text(value).foregroundStyle(Ink.text)
        }
    }

    private struct TipContent {
        let title: String
        let swatch: Color
        let value: Int64
        /// Not on Entrate, which is the income.
        let showsShare: Bool
    }

    /// "Casa → Utenze" and the ribbon's value, or the node's title and the
    /// figure its label says; `nil` when the graph no longer has it.
    private func tooltipContent(_ target: Target) -> TipContent? {
        switch target {
        case .link(let id):
            guard let tip = sankey.tip(forLink: id), let link = sankey.links.first(where: { $0.id == id })
            else { return nil }
            return TipContent(
                title: tip.title, swatch: tint(from: link.from, to: link.to), value: tip.value, showsShare: true
            )
        case .node(let id):
            guard let tip = sankey.tip(forNode: id), let node = byId[id] else { return nil }
            return TipContent(title: tip.title, swatch: tint(node), value: tip.value, showsShare: node.role != .total)
        }
    }

    // MARK: - VoiceOver

    /// The summary, then a node at a time (`AccessibilityText.sankey`).
    private var spoken: [(label: String, value: String)] {
        AccessibilityText.sankeyElements(sankey, name: name)
    }
}
