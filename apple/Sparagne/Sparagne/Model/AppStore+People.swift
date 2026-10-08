import Foundation
import SparagneCore

/// Who a row may be for (`docs/v2/UI.md` §3, PERSONA): the names the person
/// cells, the quick-add line's `!name` and the owner picker choose among.
extension AppStore {
    /// The members of the vault on screen, when the sync engine has heard
    /// them from the server: the server refuses a row or a template put on
    /// anyone else (`not_a_member`), so nothing else is offered.
    ///
    /// Otherwise (logged out, a demo database, a vault not pushed yet) there
    /// is no list to check against, and the names the vault already knows
    /// stand in: the current author first, then the persons of its rows and
    /// the owners of its templates, each once.
    var assignablePeople: [String] {
        if let vault = currentVault, let members = vaultMembers[vault.id], !members.isEmpty {
            return members
        }
        let owners = (recurringTemplates + pendingRecurringItems.map(\.template)).map(\.owner)
        return Self.distinct([currentAuthor] + peopleInRows + owners)
    }

    /// Resolves what was typed in a PERSONA cell to one of
    /// `assignablePeople`, with the precedence of `resolveFlow` and of the
    /// core's `!name` (exact, then unique prefix, then unique substring), so
    /// `eli` is Elisa wherever it is typed. Returns `nil` for blank text,
    /// meaning "leave it to the default".
    ///
    /// Synchronous, like `resolveFlow`: the names are already loaded.
    func resolvePerson(named text: String) throws -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let needle = trimmed.lowercased()
        guard !needle.isEmpty else { return nil }
        let people = assignablePeople
        // The name as written wins over one that differs only in case: a
        // logged-out vault can know both "Elisa" and the username "elisa".
        if let exact = people.first(where: { $0 == trimmed })
            ?? people.first(where: { $0.lowercased() == needle })
        {
            return exact
        }
        for matches in [
            people.filter { $0.lowercased().hasPrefix(needle) },
            people.filter { $0.lowercased().contains(needle) },
        ] {
            if matches.count == 1 { return matches[0] }
            if matches.count > 1 {
                throw DomainError.InvalidCommand(
                    message: String(localized: "More than one person matches \u{201C}\(trimmed)\u{201D}")
                )
            }
        }
        throw DomainError.NotFound(message: ErrorMessages.unknownPerson(trimmed))
    }

    /// `names` in their first order, blanks and repeats dropped.
    static func distinct(_ names: [String]) -> [String] {
        var seen = Set<String>()
        return names.filter { name in
            !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && seen.insert(name).inserted
        }
    }
}
