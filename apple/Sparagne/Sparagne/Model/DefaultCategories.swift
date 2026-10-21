import Foundation

/// A category a new vault starts with, and its aliases: `#pizza` lands in Ristoranti.
struct DefaultCategory: Equatable, Sendable {
    let name: String
    let aliases: [String]

    init(_ name: String, aliases: [String] = []) {
        self.name = name
        self.aliases = aliases
    }
}

/// The categories onboarding gives a new vault, sent as ordinary commands right after `CreateVault`.
enum DefaultCategories {
    // One word each, so `#Name` reaches it. No catch-all (Uncategorized is one), no savings
    // (the envelopes are), no refunds (a refund is filed under its expense's category).

    /// The list in the language the app's own strings are shown in, so an
    /// Italian window makes a vault with Italian names; English for any
    /// language the app is not translated into.
    static func forAppLanguage(bundle: Bundle = .main) -> [DefaultCategory] {
        list(for: bundle.preferredLocalizations.first ?? "en")
    }

    static func list(for language: String) -> [DefaultCategory] {
        Locale(identifier: language).language.languageCode == .italian ? italian : english
    }

    static let italian: [DefaultCategory] = [
        // Uscite
        DefaultCategory("Spesa", aliases: ["supermercato", "alimentari"]),
        DefaultCategory("Casa", aliases: ["affitto", "mutuo"]),
        DefaultCategory("Bollette", aliases: ["luce", "gas", "internet"]),
        DefaultCategory("Trasporti", aliases: ["benzina", "treno", "auto"]),
        DefaultCategory("Ristoranti", aliases: ["bar", "caffè", "pizza"]),
        DefaultCategory("Salute", aliases: ["farmacia", "medico"]),
        DefaultCategory("Abbigliamento", aliases: ["vestiti", "scarpe"]),
        DefaultCategory("Svago", aliases: ["cinema", "hobby"]),
        DefaultCategory("Viaggi", aliases: ["vacanze", "hotel"]),
        DefaultCategory("Abbonamenti", aliases: ["streaming", "palestra"]),
        DefaultCategory("Istruzione", aliases: ["libri", "corsi"]),
        DefaultCategory("Tasse", aliases: ["imposte", "multe"]),
        DefaultCategory("Assicurazioni"),
        DefaultCategory("Regali", aliases: ["donazioni"]),
        // Entrate
        DefaultCategory("Stipendio"),
        DefaultCategory("Interessi"),
    ]

    static let english: [DefaultCategory] = [
        // Expenses
        DefaultCategory("Groceries", aliases: ["supermarket", "grocery"]),
        DefaultCategory("Home", aliases: ["rent", "mortgage"]),
        DefaultCategory("Utilities", aliases: ["electricity", "gas", "internet"]),
        DefaultCategory("Transport", aliases: ["fuel", "train", "car"]),
        DefaultCategory("Dining", aliases: ["restaurant", "coffee", "pizza"]),
        DefaultCategory("Health", aliases: ["pharmacy", "doctor"]),
        DefaultCategory("Clothing", aliases: ["clothes", "shoes"]),
        DefaultCategory("Entertainment", aliases: ["cinema", "hobby"]),
        DefaultCategory("Travel", aliases: ["holiday", "hotel"]),
        DefaultCategory("Subscriptions", aliases: ["streaming", "gym"]),
        DefaultCategory("Education", aliases: ["books", "courses"]),
        DefaultCategory("Taxes", aliases: ["tax", "fines"]),
        DefaultCategory("Insurance"),
        DefaultCategory("Gifts", aliases: ["donations"]),
        // Income
        DefaultCategory("Salary"),
        DefaultCategory("Interest"),
    ]
}
