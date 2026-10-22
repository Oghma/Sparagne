import Foundation
import SparagneCore
import Testing

@testable import Sparagne

/// Turning an entry into another kind, worked out with no vault
/// (`KindChange`): the sign of the amount cell, the kinds a row may become,
/// and the void and the entry that make the change.
struct KindChangeTests {
    /// A row as the core lists it: an expense of 12.50 on its own wallet and
    /// envelope, filed under Spesa, recorded by matteo for himself, at noon
    /// in India, so a lost offset would show.
    private static func view(
        kind: TransactionKind = .expense,
        category: String = "Spesa",
        isSystem: Bool = false,
        note: String? = "coop",
        person: String = "matteo",
        createdBy: String = "matteo",
        voided: Bool = false
    ) -> TransactionView {
        let isTransfer = kind == .transferWallet || kind == .transferFlow
        return TransactionView(
            id: "old",
            kind: kind,
            occurredAt: "2026-10-12T12:00:00+05:30",
            amount: 1_250,
            categoryId: "category",
            category: category,
            categoryIsSystem: isSystem,
            note: note,
            person: person,
            createdBy: createdBy,
            voided: voided,
            walletId: isTransfer ? nil : "wallet",
            flowId: isTransfer ? nil : "flow",
            fromId: isTransfer ? "from" : nil,
            toId: isTransfer ? "to" : nil,
            legs: []
        )
    }

    /// The entry the batch creates, after checking the void comes first.
    private static func entry(
        _ result: Result<[Command], KindChange.Refusal>,
        sourceLocation: SourceLocation = #_sourceLocation
    ) throws -> (kind: TransactionKind, entry: Entry) {
        let commands = try result.get()
        try #require(commands.count == 2, sourceLocation: sourceLocation)
        #expect(commands[0] == .voidTransaction(transactionId: "old"), sourceLocation: sourceLocation)
        let created: (kind: TransactionKind, entry: Entry)? = switch commands[1] {
        case .expense(let entry): (.expense, entry)
        case .income(let entry): (.income, entry)
        case .refund(let entry): (.refund, entry)
        default: nil
        }
        return try #require(created, sourceLocation: sourceLocation)
    }

    // MARK: - The sign

    @Test("+ is income, - and − are expense, r and R are refund, with the number after them trimmed; no sign is no kind")
    func signTable() {
        let cases: [(text: String, kind: TransactionKind?, number: String)] = [
            ("+60", .income, "60"),
            (" + 60,50 ", .income, "60,50"),
            ("-60", .expense, "60"),
            ("\u{2212}60", .expense, "60"),
            ("r60", .refund, "60"),
            ("R 60", .refund, "60"),
            ("60", nil, "60"),
            (" 60.00 ", nil, "60.00"),
            // A sign alone: the number is missing, which the cell refuses.
            ("+", .income, ""),
            ("r", .refund, ""),
            ("\u{2212}", .expense, ""),
            ("", nil, ""),
        ]
        for (text, kind, number) in cases {
            let signed = KindChange.sign(of: text)
            #expect(signed.kind == kind, "\(text)")
            #expect(signed.number == number, "\(text)")
        }
    }

    // MARK: - The targets

    @Test("An entry may become the two other entry kinds, in order; a transfer, a deleted row and an opening balance none")
    func targets() {
        #expect(KindChange.targets(of: Self.view(kind: .expense)) == [.income, .refund])
        #expect(KindChange.targets(of: Self.view(kind: .income)) == [.expense, .refund])
        #expect(KindChange.targets(of: Self.view(kind: .refund)) == [.expense, .income])

        #expect(KindChange.targets(of: Self.view(kind: .transferWallet)).isEmpty)
        #expect(KindChange.targets(of: Self.view(kind: .transferFlow)).isEmpty)
        #expect(KindChange.targets(of: Self.view(voided: true)).isEmpty)
        #expect(KindChange.targets(of: Self.view(kind: .income, category: "Opening", isSystem: true)).isEmpty)
        // A category of the household's own that happens to be called so is
        // no opening balance.
        #expect(KindChange.targets(of: Self.view(category: "Opening")) == [.income, .refund])
    }

    @Test("The batch is refused for a transfer, a deleted row, an opening balance, the kind the row has, and a transfer as the target")
    func refusals() {
        let author = "matteo"
        #expect(KindChange.commands(from: Self.view(kind: .transferFlow), to: .income, author: author) == .failure(.transfer))
        #expect(KindChange.commands(from: Self.view(voided: true), to: .income, author: author) == .failure(.voided))
        let opening = Self.view(kind: .income, category: "Opening", isSystem: true)
        #expect(KindChange.commands(from: opening, to: .expense, author: author) == .failure(.opening))
        #expect(KindChange.commands(from: Self.view(), to: .expense, author: author) == .failure(.sameKind))
        #expect(KindChange.commands(from: Self.view(), to: .transferWallet, author: author) == .failure(.transfer))
    }

    // MARK: - The commands

    @Test("The void comes first, then the same entry as the other kind: amount, wallet, envelope, category, note and the date with its own offset")
    func sameFields() throws {
        let (kind, entry) = try Self.entry(KindChange.commands(from: Self.view(), to: .income, author: "matteo"))
        #expect(kind == .income)
        #expect(entry.amount == 1_250)
        #expect(entry.walletId == "wallet")
        #expect(entry.flowId == "flow")
        #expect(entry.category == "Spesa")
        #expect(entry.note == "coop")
        #expect(entry.occurredAt == "2026-10-12T12:00:00+05:30")
        // The author's own row names nobody: the core falls back to them.
        #expect(entry.person == nil)

        let refund = try Self.entry(KindChange.commands(from: Self.view(), to: .refund, amount: 6_000, author: "matteo"))
        #expect(refund.kind == .refund)
        #expect(refund.entry.amount == 6_000)
    }

    @Test("Uncategorized is written as a blank name, not as its label, and an empty note as none")
    func uncategorizedAndEmptyNote() throws {
        let view = Self.view(category: "Uncategorized", isSystem: true, note: "")
        let (_, entry) = try Self.entry(KindChange.commands(from: view, to: .refund, author: "matteo"))
        #expect(entry.category == "")
        #expect(entry.note == nil)
    }

    @Test("A row for another member names them; one for the author, in any case, names nobody")
    func person() throws {
        let forElisa = Self.view(person: "elisa", createdBy: "matteo")
        #expect(try Self.entry(KindChange.commands(from: forElisa, to: .income, author: "matteo")).entry.person == "elisa")

        // Recorded by elisa for herself, changed by matteo: still hers, and
        // the new row says so, since matteo is its author now.
        let elisas = Self.view(person: "elisa", createdBy: "elisa")
        #expect(try Self.entry(KindChange.commands(from: elisas, to: .income, author: "matteo")).entry.person == "elisa")

        // The local name and the synced username of one person.
        #expect(try Self.entry(KindChange.commands(from: Self.view(), to: .income, author: "Matteo")).entry.person == nil)
    }
}
