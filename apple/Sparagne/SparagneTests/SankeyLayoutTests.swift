import CoreGraphics
import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Where the cash-flow Sankey's bars and ribbons go: pure geometry, so it is
/// checked on hand-made graphs (`SankeyFixture`) without a window.
struct SankeyLayoutTests {
    private typealias F = SankeyFixture

    private static let size = CGSize(width: 1000, height: 320)

    private static func close(_ lhs: CGFloat, _ rhs: CGFloat) -> Bool { abs(lhs - rhs) < 1e-6 }

    private static func rects(_ layout: SankeyLayout, column: Int) -> [CGRect] {
        layout.columns[column].compactMap { layout.nodes[$0] }
    }

    /// The room a column takes: its bars and the gaps between them.
    private static func used(_ rects: [CGRect]) -> CGFloat {
        rects.reduce(0) { $0 + $1.height } + SankeyLayout.gap * CGFloat(max(rects.count - 1, 0))
    }

    /// A graph with a flow far too light to see: 0,50 of 10.000,00.
    private static var sliver: CashFlowSankey? {
        CashFlowSankey.build(
            income: [F.earned("Stipendio", 1_000_000), F.earned("Interessi", 20_000)],
            expenses: [F.spent("Casa", "Mutuo", 400_000), F.spent("Auto", "Parcheggio", 50)],
            flows: F.flows
        )
    }

    @Test("No two bars of a column overlap: each sits a gap under the one before, inside the height")
    func noOverlap() throws {
        let sankey = try #require(F.month)
        let layout = SankeyLayout.make(sankey, in: Self.size)
        #expect(layout.columns.count == 4)
        #expect(layout.nodes.count == sankey.nodes.count)
        for index in layout.columns.indices {
            let rects = Self.rects(layout, column: index)
            for (upper, lower) in zip(rects, rects.dropFirst()) {
                #expect(Self.close(lower.minY - upper.maxY, SankeyLayout.gap))
            }
            #expect(rects.allSatisfy { $0.minY >= -1e-6 && $0.maxY <= Self.size.height + 1e-6 })
            #expect(rects.allSatisfy { $0.width == SankeyLayout.nodeWidth })
            #expect(Set(rects.map(\.minX)).count == 1)
        }
    }

    @Test("The column that needs the most room fills the height but for its gaps; the others fit inside, centred")
    func fillsTheHeight() throws {
        for sankey in [try #require(F.month), try #require(Self.sliver)] {
            let layout = SankeyLayout.make(sankey, in: Self.size)
            let used = layout.columns.indices.map { Self.used(Self.rects(layout, column: $0)) }
            #expect(Self.close(used.max() ?? 0, Self.size.height))
            for index in layout.columns.indices {
                let rects = Self.rects(layout, column: index)
                #expect(used[index] <= Self.size.height + 1e-6)
                #expect(Self.close(rects.first?.minY ?? -1, (Self.size.height - used[index]) / 2))
            }
        }
    }

    @Test("Ribbons meet each bar's edge side by side and together cover it, on both sides")
    func ribbonsCoverTheEdges() throws {
        for sankey in [try #require(F.month), try #require(Self.sliver)] {
            let layout = SankeyLayout.make(sankey, in: Self.size)
            #expect(layout.ribbons.count == sankey.links.count)
            for node in sankey.nodes {
                let rect = try #require(layout.nodes[node.id])
                let leaving = layout.ribbons.filter { $0.from == node.id }.map(\.source).sorted { $0.minY < $1.minY }
                let arriving = layout.ribbons.filter { $0.to == node.id }.map(\.target).sorted { $0.minY < $1.minY }
                for slices in [leaving, arriving] where !slices.isEmpty {
                    #expect(Self.close(slices.first?.minY ?? -1, rect.minY), "\(node.id) starts at its top")
                    #expect(Self.close(slices.last?.maxY ?? -1, rect.maxY), "\(node.id) ends at its bottom")
                    for (upper, lower) in zip(slices, slices.dropFirst()) {
                        #expect(Self.close(upper.maxY, lower.minY), "\(node.id)'s ribbons touch")
                    }
                    #expect(Self.close(slices.reduce(0) { $0 + $1.height }, rect.height))
                    #expect(slices.allSatisfy { $0.minX == rect.minX && $0.width == rect.width })
                }
            }
        }
    }

    @Test("Without a bar raised to the minimum, a ribbon is as thick where it leaves as where it arrives, its share of the scale")
    func ribbonsKeepTheirThickness() throws {
        let sankey = try #require(F.month)
        let layout = SankeyLayout.make(sankey, in: Self.size)
        #expect(layout.nodes.values.allSatisfy { $0.height > SankeyLayout.minimumHeight })
        for ribbon in layout.ribbons {
            #expect(Self.close(ribbon.source.height, ribbon.target.height), "\(ribbon.id)")
        }
    }

    @Test("A bar too light to see keeps the minimum height, and its ribbons share it")
    func minimumHeight() throws {
        let sankey = try #require(Self.sliver)
        let layout = SankeyLayout.make(sankey, in: Self.size)
        #expect(layout.nodes["flow:Auto"]?.height == SankeyLayout.minimumHeight)
        #expect(layout.nodes["cat:Parcheggio"]?.height == SankeyLayout.minimumHeight)
        let ribbon = try #require(layout.ribbons.first { $0.id == "flow:Auto>cat:Parcheggio" })
        #expect(Self.close(ribbon.source.height, SankeyLayout.minimumHeight))
        #expect(Self.close(ribbon.target.height, SankeyLayout.minimumHeight))
    }

    @Test("Flows go heaviest first with the savings last; categories follow the flows that pay them, the rest last")
    func ordering() throws {
        let sankey = try #require(
            CashFlowSankey.build(
                income: [F.earned("Stipendio", 200_000), F.earned("Interessi", 10_000)],
                expenses: [
                    // By value Spesa (30.000) comes before Utenze (5.000),
                    // but Utenze is paid by Casa, which sits above Spesa.
                    F.spent("Casa", "Mutuo", 50_000),
                    F.spent("Casa", "Utenze", 5_000),
                    F.spent("Spesa", "Spesa", 30_000),
                    F.spent("Spesa", "Edicola", 1_000),
                    F.spent("Spesa", "Francobolli", 1_000),
                ],
                flows: F.flows
            )
        )
        let layout = SankeyLayout.make(sankey, in: Self.size)
        #expect(layout.columns[2] == ["flow:Casa", "flow:Spesa", CashFlowSankey.savingsId])
        #expect(layout.columns[3] == ["cat:Mutuo", "cat:Utenze", "cat:Spesa", "cat:other"])
    }

    @Test("The income column leaves room on its left for its labels and the categories on their right; without it Entrate starts at the edge")
    func horizontalRoom() throws {
        let month = SankeyLayout.make(try #require(F.month), in: Self.size)
        #expect(Self.rects(month, column: 0).first?.minX == SankeyLayout.leadingRoom)
        #expect(
            Self.rects(month, column: 3).first?.minX
                == Self.size.width - SankeyLayout.trailingRoom - SankeyLayout.nodeWidth
        )
        let xs = month.columns.indices.compactMap { Self.rects(month, column: $0).first?.minX }
        #expect(Self.close(xs[1] - xs[0], xs[2] - xs[1]))
        #expect(Self.close(xs[2] - xs[1], xs[3] - xs[2]))

        let single = try #require(
            CashFlowSankey.build(
                income: [F.earned("Stipendio", 100_000)], expenses: [F.spent("Casa", "Mutuo", 40_000)], flows: F.flows
            )
        )
        let layout = SankeyLayout.make(single, in: Self.size)
        #expect(layout.columns.first == [CashFlowSankey.totalId])
        #expect(layout.nodes[CashFlowSankey.totalId]?.minX == 0)
    }

    @Test("Nothing to place in no room")
    func noRoom() throws {
        let layout = SankeyLayout.make(try #require(F.month), in: .zero)
        #expect(layout.nodes.isEmpty)
        #expect(layout.ribbons.isEmpty)
    }

    // MARK: - Labels

    /// No two shown labels share a point, and all are inside the height.
    private static func expectApart(_ tops: [CGFloat?], heights: [CGFloat], within height: CGFloat) {
        let shown = zip(tops, heights).compactMap { top, height in top.map { ($0, height) } }
        for (upper, lower) in zip(shown, shown.dropFirst()) {
            #expect(upper.0 + upper.1 <= lower.0 + 1e-6)
        }
        #expect(shown.allSatisfy { $0.0 >= -1e-6 && $0.0 + $0.1 <= height + 1e-6 })
    }

    @Test("A label sits centred on its bar when there is room")
    func labelCentred() {
        let tops = SankeyLayout.labelTops(centres: [50, 150], heights: [16, 30], weights: [1, 1], within: 320)
        #expect(tops == [42, 135])
    }

    @Test("Labels that would overlap move apart as little as needed, and up from the bottom")
    func labelsMoveApart() {
        let crowded = SankeyLayout.labelTops(centres: [10, 12, 14], heights: [16, 16, 16], weights: [1, 1, 1], within: 100)
        #expect(crowded == [2, 18, 34])
        Self.expectApart(crowded, heights: [16, 16, 16], within: 100)

        let bottom = SankeyLayout.labelTops(centres: [95, 98], heights: [16, 16], weights: [1, 1], within: 100)
        #expect(bottom == [68, 84])
        Self.expectApart(bottom, heights: [16, 16], within: 100)
    }

    @Test("When the labels cannot all fit, the lightest bars' go until the rest do")
    func labelsDropTheLightest() {
        let heights: [CGFloat] = [30, 30, 30, 30]
        let tops = SankeyLayout.labelTops(centres: [10, 20, 30, 40], heights: heights, weights: [5, 1, 3, 4], within: 100)
        #expect(tops[1] == nil)
        #expect(tops.compactMap(\.self).count == 3)
        Self.expectApart(tops, heights: heights, within: 100)
    }
}
