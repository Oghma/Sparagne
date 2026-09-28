import Foundation

/// One category a new vault starts with, and the other words that file under
/// it: `#pizza` lands in Ristoranti without anyone having typed that name.
struct DefaultCategory: Equatable, Sendable {
    let name: String
    let aliases: [String]

    init(_ name: String, aliases: [String] = []) {
        self.name = name
        self.aliases = aliases
    }
}

/// The categories the onboarding sheet gives a new vault, so the first rows
/// have somewhere to go.
///
/// The names are ordinary user data once written: the app sends them as
/// `CreateCategory` and `AddAlias` commands right after `CreateVault`, and the
/// core knows nothing of this list. Changing it later changes only the vaults
/// made afterwards; a replay of an old log still ends with what it had.
///
/// Each name is one word, so the quick-add can reach it as `#Name`. The list
/// leaves out a catch-all (Uncategorized is one already), savings, which are
/// what the envelopes are for, and refunds: a refund is its own kind of
/// transaction, filed under the category of the expense it nets off, and an
/// income category by that name would invite recording it as income instead.
enum DefaultCategories {
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
