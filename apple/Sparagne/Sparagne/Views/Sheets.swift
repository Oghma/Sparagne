import SwiftUI
import SparagneCore

/// First run, and "New Vault…": a vault plus its first wallet
/// (docs/v2/DISTILLATO_V1.md §3.6 introduces the three concepts here).
struct OnboardingSheet: View {
    let isFirstRun: Bool
    let create: (_ vaultName: String, _ walletName: String, _ openingBalance: Int64) -> Void
    /// The names of the other vaults the new one's owner already has
    /// (`VaultNaming.siblingNames`), for the duplicate-name warning.
    var takenNames: [String] = []

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
        SheetLayout(
            title: isFirstRun ? String(localized: "Welcome to Sparagne") : String(localized: "New Vault"),
            confirmTitle: String(localized: "Create"),
            canConfirm: canCreate,
            showsCancel: !isFirstRun,
            confirm: {
                create(
                    vaultName.trimmingCharacters(in: .whitespaces),
                    walletName.trimmingCharacters(in: .whitespaces),
                    openingMinor ?? 0
                )
                dismiss()
            },
            cancel: { dismiss() }
        ) {
            if isFirstRun {
                Text(String(localized: "A vault holds your wallets, envelopes and transactions. A wallet is where the money sits; an envelope is what it is meant for."))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Form {
                TextField(String(localized: "Vault name"), text: $vaultName)
                if let taken = VaultNaming.duplicate(of: vaultName, in: takenNames) {
                    DuplicateNameWarning(name: taken)
                }
                TextField(String(localized: "First wallet"), text: $walletName)
                TextField(String(localized: "Opening balance"), text: $openingBalance)
                    .monospacedDigit()
            }
            .formStyle(.grouped)
        }
    }
}

/// A wallet and, optionally, the money already in it.
struct NewWalletSheet: View {
    let currency: Currency
    let create: (_ name: String, _ openingBalance: Int64) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var openingBalance = ""

    private var openingMinor: Int64? {
        let trimmed = openingBalance.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { return 0 }
        return try? parseMoney(text: trimmed, currency: currency)
    }

    var body: some View {
        SheetLayout(
            title: String(localized: "New Wallet"),
            confirmTitle: String(localized: "Create"),
            canConfirm: !name.trimmingCharacters(in: .whitespaces).isEmpty && openingMinor != nil,
            showsCancel: true,
            confirm: {
                create(name.trimmingCharacters(in: .whitespaces), openingMinor ?? 0)
                dismiss()
            },
            cancel: { dismiss() }
        ) {
            Form {
                TextField(String(localized: "Name"), text: $name)
                TextField(String(localized: "Opening balance"), text: $openingBalance)
                    .monospacedDigit()
            }
            .formStyle(.grouped)
        }
    }
}

/// An envelope, its cap mode and how much to move into it from Unallocated.
struct NewEnvelopeSheet: View {
    let currency: Currency
    let create: (_ name: String, _ mode: FlowMode, _ allowNegative: Bool, _ openingAllocation: Int64) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var capKind: CapKind = .unlimited
    @State private var capAmount = ""
    @State private var allowNegative = false
    @State private var openingAllocation = ""

    /// `FlowMode` as a flat picker choice (docs/v2/DISTILLATO_V1.md §2.2:
    /// "mode as data, not as types").
    enum CapKind: String, CaseIterable, Identifiable {
        case unlimited
        case netCapped
        case incomeCapped

        var id: String { rawValue }

        var label: String {
            switch self {
            case .unlimited: String(localized: "Unlimited")
            case .netCapped: String(localized: "Net cap")
            case .incomeCapped: String(localized: "Income cap")
            }
        }
    }

    private func minorUnits(_ text: String) -> Int64? {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { return 0 }
        return try? parseMoney(text: trimmed, currency: currency)
    }

    private var mode: FlowMode? {
        switch capKind {
        case .unlimited: .unlimited
        case .netCapped: minorUnits(capAmount).map { .netCapped(cap: $0) }
        case .incomeCapped: minorUnits(capAmount).map { .incomeCapped(cap: $0) }
        }
    }

    private var canCreate: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
            && mode != nil
            && minorUnits(openingAllocation) != nil
    }

    var body: some View {
        SheetLayout(
            title: String(localized: "New Envelope"),
            confirmTitle: String(localized: "Create"),
            canConfirm: canCreate,
            showsCancel: true,
            confirm: {
                create(
                    name.trimmingCharacters(in: .whitespaces),
                    mode ?? .unlimited,
                    allowNegative,
                    minorUnits(openingAllocation) ?? 0
                )
                dismiss()
            },
            cancel: { dismiss() }
        ) {
            Form {
                TextField(String(localized: "Name"), text: $name)
                Picker(String(localized: "Cap"), selection: $capKind) {
                    ForEach(CapKind.allCases) { kind in
                        Text(kind.label).tag(kind)
                    }
                }
                if capKind != .unlimited {
                    TextField(String(localized: "Cap amount"), text: $capAmount)
                        .monospacedDigit()
                }
                Toggle(String(localized: "Allow negative balance"), isOn: $allowNegative)
                TextField(String(localized: "Opening allocation"), text: $openingAllocation)
                    .monospacedDigit()
            }
            .formStyle(.grouped)
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
        SheetLayout(
            title: title,
            confirmTitle: String(localized: "Save"),
            canConfirm: canSave,
            showsCancel: true,
            confirm: {
                save(trimmed)
                dismiss()
            },
            cancel: { dismiss() }
        ) {
            TextField(String(localized: "Name"), text: $text)
                .textFieldStyle(.roundedBorder)
            if trimmed != name, let taken = VaultNaming.duplicate(of: trimmed, in: takenNames) {
                DuplicateNameWarning(name: taken)
            }
        }
    }
}

/// The confirmation before `DeleteVault`, the one action no undo covers: the
/// vault goes with its wallets, envelopes, transactions and recurring
/// templates. It leaves each member's devices as they sync, while the server
/// keeps its log so the deletion can reach them (`docs/v2/SYNC.md` §3-§4.6).
/// Reached from the Vault menu, the palette and the management sheet, so the
/// wording lives in one place.
struct DeleteVaultSheet: View {
    let vault: VaultView
    let delete: () -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        SheetLayout(
            title: String(localized: "Delete Vault"),
            confirmTitle: String(localized: "Delete"),
            confirmRole: .destructive,
            canConfirm: true,
            showsCancel: true,
            confirm: {
                delete()
                dismiss()
            },
            cancel: { dismiss() }
        ) {
            Text(String(localized: "Delete \u{201C}\(vault.name)\u{201D}?"))
            Text(String(localized: "Its wallets, envelopes, transactions and recurring templates go with it. It disappears from every device of every member as they sync; the server keeps its history only to bring the deletion to them. This cannot be undone."))
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// Mode, cap and allow-negative for an existing envelope (`.updateFlow`).
/// Renaming is the separate, lighter `RenameSheet`.
struct EditEnvelopeSheet: View {
    let flow: FlowView
    let currency: Currency
    let save: (_ mode: FlowMode, _ allowNegative: Bool) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var capKind: NewEnvelopeSheet.CapKind
    @State private var capAmount: String
    @State private var allowNegative: Bool

    init(flow: FlowView, currency: Currency, save: @escaping (_ mode: FlowMode, _ allowNegative: Bool) -> Void) {
        self.flow = flow
        self.currency = currency
        self.save = save
        switch flow.mode {
        case .unlimited:
            _capKind = State(initialValue: .unlimited)
            _capAmount = State(initialValue: "")
        case .netCapped(let cap):
            _capKind = State(initialValue: .netCapped)
            _capAmount = State(initialValue: MoneyFormatter.editable(minorUnits: cap))
        case .incomeCapped(let cap):
            _capKind = State(initialValue: .incomeCapped)
            _capAmount = State(initialValue: MoneyFormatter.editable(minorUnits: cap))
        }
        _allowNegative = State(initialValue: flow.allowNegative)
    }

    private func minorUnits(_ text: String) -> Int64? {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { return 0 }
        return try? parseMoney(text: trimmed, currency: currency)
    }

    private var mode: FlowMode? {
        switch capKind {
        case .unlimited: .unlimited
        case .netCapped: minorUnits(capAmount).map { .netCapped(cap: $0) }
        case .incomeCapped: minorUnits(capAmount).map { .incomeCapped(cap: $0) }
        }
    }

    var body: some View {
        SheetLayout(
            title: String(localized: "Edit Envelope"),
            confirmTitle: String(localized: "Save"),
            canConfirm: mode != nil,
            showsCancel: true,
            confirm: {
                if let mode { save(mode, allowNegative) }
                dismiss()
            },
            cancel: { dismiss() }
        ) {
            Form {
                Picker(String(localized: "Cap"), selection: $capKind) {
                    ForEach(NewEnvelopeSheet.CapKind.allCases) { kind in
                        Text(kind.label).tag(kind)
                    }
                }
                if capKind != .unlimited {
                    TextField(String(localized: "Cap amount"), text: $capAmount)
                        .monospacedDigit()
                }
                Toggle(String(localized: "Allow negative balance"), isOn: $allowNegative)
            }
            .formStyle(.grouped)
        }
    }
}

// MARK: - Vault names

/// Vault names are labels (`docs/v2/SYNC.md` §3, "Nomi dei vault"): two
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
        Label(
            String(localized: "Another vault of the same owner is already called \u{201C}\(name)\u{201D}. The name is allowed, but the two will be hard to tell apart."),
            systemImage: "exclamationmark.triangle"
        )
        .font(.callout)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
    }
}

// MARK: - Shared chrome

/// Title, content and a Cancel/Confirm footer: the plain macOS sheet shape.
private struct SheetLayout<Content: View>: View {
    let title: String
    let confirmTitle: String
    /// `.destructive` for the one sheet that deletes: the button is drawn as
    /// such and ↩ no longer triggers it, so a stray return cannot.
    var confirmRole: ButtonRole? = nil
    let canConfirm: Bool
    let showsCancel: Bool
    let confirm: () -> Void
    let cancel: () -> Void
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.headline)
            content
            HStack {
                Spacer()
                if showsCancel {
                    Button(String(localized: "Cancel"), role: .cancel, action: cancel)
                        .keyboardShortcut(.cancelAction)
                }
                Button(confirmTitle, role: confirmRole, action: confirm)
                    .keyboardShortcut(confirmRole == .destructive ? nil : .defaultAction)
                    .disabled(!canConfirm)
            }
        }
        .padding(20)
        .frame(width: 420)
    }
}
