import SwiftUI
import SparagneCore

/// What the server refused, kept by the core until it is dismissed
/// (`docs/v2/SYNC.md` §4.5).
struct RejectedChangesSheet: View {
    let engine: SyncEngine
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        FormSheet(
            String(localized: "Refused changes"),
            subtitle: String(localized: "The server did not accept these changes, so they are not in your balances."),
            width: 480
        ) {
            if engine.rejected.isEmpty {
                Panel {
                    VStack(spacing: 6) {
                        Image(systemName: "checkmark.circle")
                            .font(.system(size: 20))
                            .foregroundStyle(Ink.text3)
                            .accessibilityHidden(true)
                        Text(String(localized: "Nothing was refused"))
                            .font(Face.ui(12.5))
                            .foregroundStyle(Ink.text2)
                    }
                    .frame(maxWidth: .infinity)
                    .frame(height: 116)
                }
            } else {
                Panel(padding: 0) {
                    ScrollView {
                        VStack(spacing: 0) {
                            ForEach(Array(engine.rejected.enumerated()), id: \.element.id) { index, change in
                                if index > 0 { Hairline() }
                                RejectedRow(change: change) { Task { await engine.dismiss(change) } }
                            }
                        }
                    }
                    .frame(height: 220)
                }
            }
        } footer: {
            Button(String(localized: "Dismiss All")) { Task { await engine.dismissAllRejected() } }
                .buttonStyle(.chrome(.bordered))
                .disabled(engine.rejected.isEmpty)
            FormPrimaryButton(title: String(localized: "Close"), role: .cancel) { dismiss() }
        }
    }
}

/// One refused change: what it was, the server's reason, and where.
private struct RejectedRow: View {
    let change: SyncEngine.RejectedChange
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(change.summary)
                    .font(Face.ui(13))
                    .foregroundStyle(Ink.text)
                Text(change.command.message)
                    .font(Face.ui(11.5))
                    .foregroundStyle(Ink.text2)
                    .fixedSize(horizontal: false, vertical: true)
                Text("\(change.vaultName) · \(change.kindName)")
                    .font(Face.ui(11))
                    .foregroundStyle(Ink.text3)
            }
            Spacer(minLength: 8)
            Button(String(localized: "Dismiss"), action: dismiss)
                .buttonStyle(.chrome(.ghost, small: true))
        }
        .padding(.horizontal, Metrics.cardPad)
        .padding(.vertical, 8)
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
        FormSheet(String(localized: "Share Vault"), subtitle: vault.name, width: 460) {
            FormGroup(String(localized: "Members")) {
                Panel(padding: 0) {
                    ScrollView {
                        VStack(spacing: 0) {
                            ForEach(Array(members.enumerated()), id: \.element.id) { index, member in
                                if index > 0 { Hairline() }
                                memberRow(member)
                            }
                        }
                    }
                    .frame(height: 160)
                }
            }

            HStack(spacing: 8) {
                // No label column here: the field says what it is in its
                // empty state.
                FormTextField(label: String(localized: "Username"), text: $newUsername, prompt: String(localized: "Username"))
                FormPicker(
                    label: String(localized: "Role"),
                    selection: $newRole,
                    options: [MemberRole.editor, .viewer],
                    title: \.label
                )
                .frame(width: 120)
                Button(String(localized: "Add")) {
                    let username = trimmedUsername
                    act {
                        try await engine.setMember(vaultId: vault.id, username: username, role: newRole)
                        newUsername = ""
                    }
                }
                .buttonStyle(.chrome(.bordered))
                .disabled(trimmedUsername.isEmpty || busy)
            }

            if let message {
                FormNote(message, tone: .negative, indented: false)
            }
        } footer: {
            FormPrimaryButton(title: String(localized: "Done"), role: .cancel) { dismiss() }
        }
        .task { await load() }
    }

    private func memberRow(_ member: MemberEntry) -> some View {
        HStack(spacing: 8) {
            Text(member.username)
                .foregroundStyle(Ink.text)
            Spacer(minLength: 8)
            Text(member.role.label)
                .foregroundStyle(Ink.text2)
            if member.role != .owner {
                FormDestructiveButton(title: String(localized: "Remove"), small: true) {
                    act { try await engine.removeMember(vaultId: vault.id, username: member.username) }
                }
            }
        }
        .font(Face.ui(13))
        .padding(.horizontal, Metrics.cardPad)
        .frame(minHeight: 32)
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
/// Drawn with the form kit at a sheet's density, so the window reads as one
/// more panel of the app rather than the system's grouped form.
struct AccountSettingsView: View {
    let engine: SyncEngine

    @State private var username = ""
    @State private var password = ""
    @State private var busy = false
    @State private var changingPassword = false

    var body: some View {
        @Bindable var account = engine.account
        VStack(alignment: .leading, spacing: 20) {
            FormGroup(String(localized: "Server")) {
                FormRow(String(localized: "Server address")) {
                    FormTextField(label: String(localized: "Server address"), text: $account.serverURLText)
                        .textContentType(.URL)
                }
                .disabled(engine.isLoggedIn)
            }

            if let name = account.username {
                FormGroup(String(localized: "Account")) {
                    FormValueRow(String(localized: "Signed in as"), value: name)
                    if let expiry = account.expiresAt {
                        FormNote(String(localized: "Signed in until \(expiry.formatted(date: .long, time: .shortened))"))
                    }
                    if let failure = account.tokenSaveFailure {
                        // Not an error: this session works, the next launch
                        // will ask for the password again.
                        FormNote(
                            String(localized: "Your login could not be saved in the Keychain; you'll need to log in again next time."),
                            tone: .warning,
                            detail: failure
                        )
                    }
                    FormRow("") {
                        HStack(spacing: 8) {
                            Button(String(localized: "Change Password…")) { changingPassword = true }
                            Button(String(localized: "Log Out")) {
                                busy = true
                                Task {
                                    await engine.logOut()
                                    busy = false
                                }
                            }
                        }
                        .buttonStyle(.chrome(.bordered))
                        .disabled(busy)
                    }
                }
            } else {
                FormGroup(String(localized: "Person")) {
                    FormRow(String(localized: "Name in the ledger")) {
                        FormTextField(
                            label: String(localized: "Name in the ledger"),
                            text: $account.localAuthor,
                            prompt: AccountStore.systemAuthor
                        )
                    }
                    .onChange(of: account.localAuthor) { _, _ in
                        Task { await engine.adoptLocalAuthor() }
                    }
                    FormNote(String(localized: "Signs the rows you add while logged out; an account's name takes over when you log in."))
                }

                FormGroup(String(localized: "Account")) {
                    FormRow(String(localized: "Username")) {
                        FormTextField(label: String(localized: "Username"), text: $username)
                    }
                    if let hint = usernameHint { FormNote(hint) }
                    FormRow(String(localized: "Password")) {
                        FormTextField(label: String(localized: "Password"), text: $password, secure: true)
                    }
                    if let hint = passwordHint { FormNote(hint) }
                    FormRow("") {
                        HStack(spacing: 8) {
                            Button(String(localized: "Log In")) { authenticate(registering: false) }
                                .buttonStyle(.chrome(.primary))
                                .keyboardShortcut(.defaultAction)
                                .disabled(!canLogIn)
                            Button(String(localized: "Register")) { authenticate(registering: true) }
                                .buttonStyle(.chrome(.bordered))
                                .disabled(!canRegister)
                            if busy { ProgressView().controlSize(.small) }
                        }
                        .disabled(busy)
                    }
                }
            }

            FormGroup(String(localized: "Status")) {
                FormValueRow(String(localized: "Sync"), value: statusText)
                if engine.pendingCount > 0 {
                    FormValueRow(
                        String(localized: "Waiting to sync"),
                        value: engine.pendingCount.formatted(.number)
                    )
                }
                if let message = engine.authMessage {
                    // The server's own English, as the detail.
                    FormNote(message, tone: .negative, detail: engine.authDetail)
                }
            }
        }
        .padding(20)
        .frame(width: 460, alignment: .topLeading)
        .environment(\.formMetrics, .sheet)
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

    /// The top bar's pill says the same, in the same words.
    private var statusText: String {
        SyncPillState(engine: engine).settingsText
    }
}
