import SwiftUI
import SparagneCore

/// The toolbar's sync state: an icon, a tooltip, and the number of changes
/// still waiting to reach the server.
///
/// Clicking it syncs; when the core is holding rejected commands it opens the
/// list instead, because that is what needs a decision, and while nobody is
/// logged in it opens Settings, because there is nothing to sync with yet.
struct SyncStatusButton: View {
    let engine: SyncEngine
    let showRejected: () -> Void
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        Button {
            if !engine.rejected.isEmpty {
                showRejected()
            } else if !engine.isLoggedIn {
                openSettings()
            } else {
                Task { await engine.syncNow() }
            }
        } label: {
            Image(systemName: symbol)
                .foregroundStyle(tint)
                .overlay(alignment: .topTrailing) { badge }
        }
        .help(tooltip)
        .accessibilityLabel(Text(String(localized: "Sync")))
        .accessibilityValue(Text(tooltip))
    }

    @ViewBuilder
    private var badge: some View {
        if engine.pendingCount > 0 {
            Text(engine.pendingCount, format: .number)
                .font(.system(size: 9, weight: .semibold))
                .monospacedDigit()
                .padding(.horizontal, 3)
                .background(Capsule().fill(Ink.accent))
                .foregroundStyle(Ink.bg)
                .offset(x: 8, y: -6)
        }
    }

    private var symbol: String {
        if !engine.rejected.isEmpty { return "exclamationmark.triangle" }
        switch engine.status {
        case .idle: return engine.isLoggedIn ? "checkmark.icloud" : "person.crop.circle.badge.questionmark"
        case .syncing: return "arrow.triangle.2.circlepath"
        case .offline: return "icloud.slash"
        case .error: return "exclamationmark.icloud"
        }
    }

    private var tint: Color {
        if !engine.rejected.isEmpty { return Ink.warning }
        switch engine.status {
        case .error: return Ink.negative
        case .offline: return Ink.dim
        case .idle, .syncing: return Ink.text
        }
    }

    private var tooltip: String {
        if !engine.rejected.isEmpty { return String(localized: "Some changes were refused") }
        switch engine.status {
        case .syncing: return String(localized: "Syncing…")
        case .offline: return String(localized: "Offline")
        case .error(let message): return message
        case .idle:
            if !engine.isLoggedIn {
                return engine.account.sessionExpired ? ErrorMessages.sessionExpired : String(localized: "Not signed in")
            }
            if engine.pendingCount > 0 {
                return String(localized: "\(engine.pendingCount) changes waiting")
            }
            if let last = engine.lastSyncAt {
                return String(localized: "Last synced at \(last.formatted(date: .omitted, time: .shortened))")
            }
            return String(localized: "In sync")
        }
    }
}

/// What the server refused, kept by the core until it is dismissed
/// (`docs/v2/SYNC.md` §4.5).
struct RejectedChangesSheet: View {
    let engine: SyncEngine
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(String(localized: "Refused changes")).font(.headline)
            Text(String(localized: "The server did not accept these changes, so they are not in your balances."))
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            if engine.rejected.isEmpty {
                ContentUnavailableView(
                    String(localized: "Nothing was refused"),
                    systemImage: "checkmark.circle"
                )
                .frame(height: 140)
            } else {
                List(engine.rejected) { change in
                    RejectedRow(change: change) { Task { await engine.dismiss(change) } }
                }
                .frame(height: 220)
            }

            HStack {
                Button(String(localized: "Dismiss All")) { Task { await engine.dismissAllRejected() } }
                    .disabled(engine.rejected.isEmpty)
                Spacer()
                Button(String(localized: "Close"), role: .cancel) { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 460)
    }
}

private struct RejectedRow: View {
    let change: SyncEngine.RejectedChange
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 2) {
                Text(change.summary).font(.body)
                Text(change.command.message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Text("\(change.vaultName) · \(change.kindName)")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Spacer()
            Button(String(localized: "Dismiss"), action: dismiss)
                .buttonStyle(.link)
        }
        .padding(.vertical, 2)
    }
}

/// Who else can see and write a vault. Owner only: the button that opens it
/// is hidden otherwise (`docs/v2/SYNC.md` §3, members endpoints).
struct ShareVaultSheet: View {
    let engine: SyncEngine
    let vault: VaultView

    @Environment(\.dismiss) private var dismiss
    @State private var members: [MemberEntry] = []
    @State private var newUsername = ""
    @State private var newRole: MemberRole = .editor
    @State private var message: String?
    @State private var busy = false

    private var trimmedUsername: String {
        newUsername.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(String(localized: "Share Vault")).font(.headline)
            Text(vault.name).foregroundStyle(.secondary)

            List(members) { member in
                HStack {
                    Text(member.username)
                    Spacer()
                    Text(member.role.label).foregroundStyle(.secondary)
                    if member.role != .owner {
                        Button(String(localized: "Remove"), role: .destructive) {
                            act { try await engine.removeMember(vaultId: vault.id, username: member.username) }
                        }
                        .buttonStyle(.link)
                    }
                }
            }
            .frame(height: 160)

            HStack {
                TextField(String(localized: "Username"), text: $newUsername)
                Picker(String(localized: "Role"), selection: $newRole) {
                    Text(MemberRole.editor.label).tag(MemberRole.editor)
                    Text(MemberRole.viewer.label).tag(MemberRole.viewer)
                }
                .labelsHidden()
                .frame(width: 110)
                Button(String(localized: "Add")) {
                    let username = trimmedUsername
                    act {
                        try await engine.setMember(vaultId: vault.id, username: username, role: newRole)
                        newUsername = ""
                    }
                }
                .disabled(trimmedUsername.isEmpty || busy)
            }

            if let message {
                Text(message).font(.callout).foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack {
                Spacer()
                Button(String(localized: "Done"), role: .cancel) { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 440)
        .task { await load() }
    }

    @MainActor
    private func load() async {
        do {
            members = try await engine.members(ofVault: vault.id)
            message = nil
        } catch {
            message = Self.describe(error)
        }
    }

    /// Runs a member change, then reloads the list from the server so the
    /// sheet never guesses what the server did.
    @MainActor
    private func act(_ work: @MainActor @escaping () async throws -> Void) {
        busy = true
        Task {
            do {
                try await work()
                message = nil
            } catch {
                message = Self.describe(error)
            }
            await load()
            busy = false
        }
    }

    private static func describe(_ error: some Error) -> String {
        guard let server = error as? ServerError else { return error.localizedDescription }
        return server.detail.map { "\(server.summary): \($0)" } ?? server.summary
    }
}

/// The Settings scene: which server, who I am, and where the sync stands.
struct AccountSettingsView: View {
    let engine: SyncEngine

    @State private var username = ""
    @State private var password = ""
    @State private var busy = false
    @State private var changingPassword = false

    var body: some View {
        @Bindable var account = engine.account
        Form {
            Section(String(localized: "Server")) {
                TextField(String(localized: "Server address"), text: $account.serverURLText)
                    .disabled(engine.isLoggedIn)
                    .textContentType(.URL)
            }

            if let name = account.username {
                Section(String(localized: "Account")) {
                    LabeledContent(String(localized: "Signed in as"), value: name)
                    if let expiry = account.expiresAt {
                        Text(String(localized: "Signed in until \(expiry.formatted(date: .long, time: .shortened))"))
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    if let failure = account.tokenSaveFailure {
                        // Not an error: this session works, the next launch
                        // will ask for the password again.
                        VStack(alignment: .leading, spacing: 2) {
                            Label(
                                String(localized: "Your login could not be saved in the Keychain; you'll need to log in again next time."),
                                systemImage: "exclamationmark.triangle"
                            )
                            .font(.callout)
                            .foregroundStyle(Ink.warning)
                            Text(failure)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .fixedSize(horizontal: false, vertical: true)
                    }
                    HStack {
                        Button(String(localized: "Change Password…")) { changingPassword = true }
                        Spacer()
                        Button(String(localized: "Log Out")) {
                            busy = true
                            Task {
                                await engine.logOut()
                                busy = false
                            }
                        }
                    }
                    .disabled(busy)
                }
            } else {
                Section(String(localized: "Person")) {
                    TextField(
                        String(localized: "Name in the ledger"),
                        text: $account.localAuthor,
                        prompt: Text(AccountStore.systemAuthor)
                    )
                    .onChange(of: account.localAuthor) { _, _ in
                        Task { await engine.adoptLocalAuthor() }
                    }
                    Text(String(localized: "Signs the rows you add while logged out; an account's name takes over when you log in."))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }

                Section(String(localized: "Account")) {
                    TextField(String(localized: "Username"), text: $username)
                    if let hint = usernameHint { Self.hint(hint) }
                    SecureField(String(localized: "Password"), text: $password)
                    if let hint = passwordHint { Self.hint(hint) }
                    HStack {
                        Button(String(localized: "Log In")) { authenticate(registering: false) }
                            .keyboardShortcut(.defaultAction)
                            .disabled(!canLogIn)
                        Button(String(localized: "Register")) { authenticate(registering: true) }
                            .disabled(!canRegister)
                        Spacer()
                        if busy { ProgressView().controlSize(.small) }
                    }
                    .disabled(busy)
                }
            }

            Section(String(localized: "Status")) {
                LabeledContent(String(localized: "Sync"), value: statusText)
                if engine.pendingCount > 0 {
                    LabeledContent(
                        String(localized: "Waiting to sync"),
                        value: engine.pendingCount.formatted(.number)
                    )
                }
                if let message = engine.authMessage {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(message).font(.callout).foregroundStyle(.red)
                        // The server's own English, as the detail.
                        if let detail = engine.authDetail {
                            Text(detail).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 420)
        .onAppear { username = account.lastUsername }
        .sheet(isPresented: $changingPassword) { ChangePasswordSheet(engine: engine) }
    }

    /// The name as the server will read it: trimmed and lowercased.
    private var normalizedUsername: String { AccountRules.normalize(username: username) }

    private var canLogIn: Bool { !normalizedUsername.isEmpty && !password.isEmpty }

    /// Register holds the server's rules up front; logging in only needs
    /// something in both fields.
    private var canRegister: Bool {
        AccountRules.isValidUsername(normalizedUsername) && AccountRules.isValidPassword(password)
    }

    /// Shown once something is typed, as a rule for a new account: an
    /// existing one logs in whatever the form thinks of its name.
    private var usernameHint: String? {
        guard !normalizedUsername.isEmpty, !AccountRules.isValidUsername(normalizedUsername) else { return nil }
        return String(localized: "New accounts need 3 to 32 characters: a–z, 0–9, “_”, “.” or “-”.")
    }

    private var passwordHint: String? {
        guard !password.isEmpty, !AccountRules.isValidPassword(password) else { return nil }
        return String(localized: "New accounts need a password of at least 8 characters.")
    }

    private static func hint(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }

    @MainActor
    private func authenticate(registering: Bool) {
        let name = normalizedUsername
        let secret = password
        busy = true
        Task {
            if registering {
                await engine.register(username: name, password: secret)
            } else {
                await engine.logIn(username: name, password: secret)
            }
            if engine.isLoggedIn { password = "" }
            busy = false
        }
    }

    private var statusText: String {
        switch engine.status {
        case .idle:
            if engine.isLoggedIn {
                String(localized: "In sync")
            } else if engine.account.sessionExpired {
                ErrorMessages.sessionExpired
            } else {
                String(localized: "Not signed in")
            }
        case .syncing: String(localized: "Syncing…")
        case .offline: String(localized: "Offline")
        case .error(let message): message
        }
    }
}

