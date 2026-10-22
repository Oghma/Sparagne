import Foundation
import Testing

@testable import Sparagne

/// What the toast after a kind change says (`KindMoveToast.text`): the list
/// the rows went to, the count only for more than one.
struct KindMoveToastTests {
    private static func italian() throws -> Bundle {
        try CatalogBundle.make(
            language: "it",
            plurals: [
                "%lld rows moved to Income": ("Spostata in Entrate", "%lld righe spostate in Entrate"),
                "%lld rows moved to Expenses": ("Spostata in Uscite", "%lld righe spostate in Uscite"),
            ],
            name: "kind-move"
        )
    }

    private static let locale = Locale(identifier: "it_IT")

    @Test("One row says where it went; several say how many")
    func text() throws {
        let bundle = try Self.italian()
        #expect(KindMoveToast.text(.income, count: 1, bundle: bundle, locale: Self.locale) == "Spostata in Entrate")
        #expect(KindMoveToast.text(.expenses, count: 1, bundle: bundle, locale: Self.locale) == "Spostata in Uscite")
        #expect(KindMoveToast.text(.income, count: 3, bundle: bundle, locale: Self.locale) == "3 righe spostate in Entrate")
        #expect(KindMoveToast.text(.expenses, count: 2, bundle: bundle, locale: Self.locale) == "2 righe spostate in Uscite")
    }
}
