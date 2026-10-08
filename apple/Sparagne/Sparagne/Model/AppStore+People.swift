import Foundation
import SparagneCore

/// Who a row may be for (the PERSONA column): the names the person
/// cells, the quick-add line's `!name` and the owner picker choose among.
extension AppStore {
    /// The members of the vault on screen, when the sync engine has heard
    /// them from the server: the server refuses a row or a template put on
    /// anyone else (`not_a_member`), so nothing else is offered.
    ///
    /// Logged in before the list is heard (a vault not pushed yet, a first
    /// launch offline), only the author is certain to pass: the persons of
    /// old rows may be members who left, and offering them would only earn
    /// a refusal at the next sync.
    ///
    /// Logged out or in a demo database no server checks anyone, and the
    /// names the vault already knows stand in: the current author first,
    /// then the persons of its rows and the owners of its templates, each
    /// once.
    var assignablePeople: [String] {
        if let known = vaultMembers, let vault = currentVault {
            guard let members = known[vault.id], !members.isEmpty else { return [currentAuthor] }
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
        // The name as written wins over one that differs only in case: a
        // logged-out vault can know both "Elisa" and the username "elisa".
        // Past that the two are one person, as the core's `!name` counts
        // them, or no prefix of theirs would ever be unique.
        if let exact = assignablePeople.first(where: { $0 == trimmed }) { return exact }
        let people = Self.folded(assignablePeople)
        if let same = people.first(where: { $0.lowercased() == needle }) { return same }
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
        // The headline the quick-add line gives the same name, and who may
        // be named instead.
        throw AppError(
            code: "not_found",
            message: String(localized: "A row may be for \(ListFormatter.localizedString(byJoining: assignablePeople))"),
            headline: ErrorMessages.unknownPerson(trimmed)
        )
    }

    /// The members of the vault on screen, when the server's list of them is
    /// known: `nil` logged out, in a demo database, or before it is heard.
    var currentMembers: [String]? {
        currentVault.flatMap { vaultMembers?[$0.id] }.flatMap { $0.isEmpty ? nil : $0 }
    }

    /// Whether `template`'s owner has left the vault on screen. Its periods
    /// can be skipped but not recorded: the server would refuse the row
    /// (`not_a_member`) and the period would fall due again, every time,
    /// until the template is given another owner.
    func ownerHasLeft(_ template: RecurringView) -> Bool {
        Self.ownerHasLeft(template.owner, members: currentMembers, author: currentAuthor)
    }

    /// The rule behind `ownerHasLeft`, apart from the store: the members are
    /// known, and `owner` is none of them. The author always passes, since a
    /// command leaves them out (`explicitPerson`), and so does a blank. With
    /// no list (logged out, a demo database, a list not heard yet) nothing
    /// is held back: no server will check, or the next round will tell.
    nonisolated static func ownerHasLeft(_ owner: String, members: [String]?, author: String) -> Bool {
        let name = owner.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let members, !members.isEmpty, !name.isEmpty else { return false }
        return name.lowercased() != author.lowercased() && !members.contains(name)
    }

    /// The note a template whose owner left carries in the Ricorrenze tab.
    static var noLongerAMember: String { String(localized: "no longer a member") }

    /// Why a period of such a template is not recorded: what the Record
    /// buttons say on hover, and the alert when one is pressed anyway.
    static var ownerLeftExplanation: String {
        String(localized: "A template whose owner is no longer a member of this vault records nothing until it has another owner. Its periods can still be skipped.")
    }

    /// The headline when Registra tutte leaves out `count` such periods.
    static func periodsNotRecorded(_ count: Int) -> String {
        String(localized: "\(count) periods not recorded")
    }

    /// `names` in their first order, blanks and repeats dropped.
    static func distinct(_ names: [String]) -> [String] {
        var seen = Set<String>()
        return names.filter { name in
            !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && seen.insert(name).inserted
        }
    }

    /// `names` with the spellings that differ only in case counted once, at
    /// the place of the first: the lowercase one when there is one, since
    /// usernames are lowercase and that is the name the server knows. What
    /// the quick-add line resolves its `!name` among, so the local "Matteo"
    /// and the synced "matteo" never tie.
    static func folded(_ names: [String]) -> [String] {
        var spellings: [String] = []
        var slots: [String: Int] = [:]
        for name in distinct(names) {
            let key = name.lowercased()
            if let slot = slots[key] {
                if name == key { spellings[slot] = name }
            } else {
                slots[key] = spellings.count
                spellings.append(name)
            }
        }
        return spellings
    }
}
