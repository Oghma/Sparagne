import SwiftUI
import SparagneCore

/// First run, and "New Vault…": a vault plus its first wallet.
struct OnboardingSheet: View {
    let isFirstRun: Bool
    /// The names of the other vaults the new one's owner already has
    /// (`VaultNaming.siblingNames`), for the duplicate-name warning. Before
    /// `create`, so a caller can pass it and keep `create` as the trailing
    /// closure.
    var takenNames: [String] = []
    let create: (_ vaultName: String, _ walletName: String, _ openingBalance: Int64) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var vaultName = "Main"
    @State private var walletName = "Cash"
    @State private var openingBalance = ""

    private var openingMinor: Int64? {
        let trimmed = openingBalance.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { return 0 }
        return try? parseMoney(text: trimmed, currency: .eur)
    }

    private var canCreate: Bool {
        !vaultName.trimmingCharacters(in: .whitespaces).isEmpty
            && !walletName.trimmingCharacters(in: .whitespaces).isEmpty
            && openingMinor != nil
    }

    var body: some View {
        FormSheet(
            isFirstRun ? String(localized: "Welcome to Sparagne") : String(localized: "New Vault"),
            // The three concepts, once, before the first one is named.
            subtitle: isFirstRun
                ? String(localized: "A vault holds your wallets, envelopes and transactions. A wallet is where the money sits; an envelope is what it is meant for.")
                : nil
        ) {
            FormGroup {
                FormRow(String(localized: "Vault name")) {
                    FormTextField(label: String(localized: "Vault name"), text: $vaultName)
                }
                if let taken = VaultNaming.duplicate(of: vaultName, in: takenNames) {
                    DuplicateNameWarning(name: taken)
                }
                FormRow(String(localized: "First wallet")) {
                    FormTextField(label: String(localized: "First wallet"), text: $walletName)
                }
                FormRow(String(localized: "Opening balance")) {
                    FormTextField(label: String(localized: "Opening balance"), text: $openingBalance, prompt: MoneyPrompt.zero)
                }
            }
        } footer: {
            if !isFirstRun {
                FormCancelButton { dismiss() }
            }
            FormPrimaryButton(title: String(localized: "Create")) {
                create(
                    vaultName.trimmingCharacters(in: .whitespaces),
                    walletName.trimmingCharacters(in: .whitespaces),
                    openingMinor ?? 0
                )
                dismiss()
            }
            .disabled(!canCreate)
        }
    }
}

/// A single text field with Cancel/Save: vault and wallet rename and the
/// envelope quick-rename. Fuller envelope edits go through
/// `EditEnvelopeSheet`.
struct RenameSheet: View {
    let title: String
    let name: String
    /// Names the new one would be confused with, for a warning that never
    /// blocks: the owner's other vaults when a vault is renamed
    /// (`VaultNaming.siblingNames`), nothing otherwise.
    let takenNames: [String]
    let save: (String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var text: String

    init(title: String, name: String, takenNames: [String] = [], save: @escaping (String) -> Void) {
        self.title = title
        self.name = name
        self.takenNames = takenNames
        self.save = save
        _text = State(initialValue: name)
    }

    private var trimmed: String { text.trimmingCharacters(in: .whitespaces) }
    private var canSave: Bool { !trimmed.isEmpty && trimmed != name }

    var body: some View {
        FormSheet(title, subtitle: name, width: 420) {
            FormGroup {
                FormRow(String(localized: "Name")) {
                    FormTextField(label: String(localized: "Name"), text: $text)
                }
                if trimmed != name, let taken = VaultNaming.duplicate(of: trimmed, in: takenNames) {
                    DuplicateNameWarning(name: taken)
                }
            }
        } footer: {
            FormCancelButton { dismiss() }
            FormPrimaryButton(title: String(localized: "Save")) {
                save(trimmed)
                dismiss()
            }
            .disabled(!canSave)
        }
    }
}

/// The confirmation before `DeleteVault`, the one action no undo covers: the
/// vault goes with its wallets, envelopes, transactions and recurring
/// templates. It leaves each member's devices as they sync, while the server
/// keeps its log so the deletion can reach them.
/// Reached from the Vault menu, the palette and the management sheet, so the
/// wording lives in one place.
struct DeleteVaultSheet: View {
    let vault: VaultView
    let delete: () -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        FormSheet(String(localized: "Delete Vault")) {
            ConfirmationText(
                question: String(localized: "Delete \u{201C}\(vault.name)\u{201D}?"),
                consequences: String(localized: "Its wallets, envelopes, transactions and recurring templates go with it. It disappears from every device of every member as they sync; the server keeps its history only to bring the deletion to them. This cannot be undone.")
            )
        } footer: {
            FormCancelButton { dismiss() }
            // Destructive, so ↩ does not trigger it.
            FormDestructiveButton(title: String(localized: "Delete")) {
                delete()
                dismiss()
            }
        }
    }
}

// MARK: - Vault names

/// Vault names are labels: two
/// vaults may share one, even with the same owner. Nothing refuses a repeated
/// name; the sheets warn, and the lists tell the two apart.
enum VaultNaming {
    /// The name as a list shows it, with the owner appended when another
    /// listed vault has the same name.
    static func label(for vault: VaultView, among vaults: [VaultView]) -> String {
        let clashes = vaults.contains { other in
            other.id != vault.id && same(other.name, vault.name)
        }
        return clashes ? "\(vault.name) (\(vault.owner))" : vault.name
    }

    /// The names of `owner`'s vaults, but `excluding`'s own: what a new or
    /// renamed vault of theirs would be confused with.
    static func siblingNames(owner: String, excluding vaultId: Uuid? = nil, in vaults: [VaultView]) -> [String] {
        vaults.filter { $0.owner == owner && $0.id != vaultId }.map(\.name)
    }

    /// The name among `names` that `name` repeats, ignoring case and the
    /// spaces the sheets trim anyway.
    static func duplicate(of name: String, in names: [String]) -> String? {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return nil }
        return names.first { same($0, trimmed) }
    }

    private static func same(_ a: String, _ b: String) -> Bool {
        a.trimmingCharacters(in: .whitespaces).caseInsensitiveCompare(b.trimmingCharacters(in: .whitespaces))
            == .orderedSame
    }
}

/// The non-blocking note under a vault name another vault already has.
private struct DuplicateNameWarning: View {
    let name: String

    var body: some View {
        FormNote(
            String(localized: "Another vault of the same owner is already called \u{201C}\(name)\u{201D}. The name is allowed, but the two will be hard to tell apart."),
            tone: .warning
        )
    }
}

// MARK: - Shared pieces

/// The body of a sheet that confirms something it cannot take back: the
/// question in full-strength text, then what goes with it in `text2`.
/// Leaving a vault says it the same way.
struct ConfirmationText: View {
    let question: String
    let consequences: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(question)
                .font(Face.ui(13, .medium))
                .foregroundStyle(Ink.text)
            Text(consequences)
                .font(Face.ui(12.5))
                .foregroundStyle(Ink.text2)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The grey `0,00` of an empty amount field, as the inspector's amount
/// shows it: the format the field reads.
enum MoneyPrompt {
    static let zero = "0,00"
}
