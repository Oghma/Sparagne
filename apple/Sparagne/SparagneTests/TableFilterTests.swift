import Testing

@testable import Sparagne

/// The Ricorrenze and Setup tabs' local search.
struct TableFilterTests {
    @Test func aBlankQueryMatchesEverything() {
        #expect(TableFilter.matches("", ["Netflix"]))
        #expect(TableFilter.matches("  ", []))
    }

    @Test func caseAndAccentsDoNotMatter() {
        #expect(TableFilter.matches("caffe", ["Bar", "Caffè"]))
        #expect(TableFilter.matches("CAFFÈ", ["caffe"]))
    }

    @Test func anyFieldMayMatchAndPartOfOneIsEnough() {
        #expect(TableFilter.matches("flix", ["Abbonamenti", "Netflix"]))
        #expect(!TableFilter.matches("mutuo", ["Abbonamenti", "Netflix"]))
        #expect(!TableFilter.matches("x", []))
    }

    @Test func surroundingSpacesAreIgnored() {
        #expect(TableFilter.matches(" casa ", ["Casa"]))
    }
}
