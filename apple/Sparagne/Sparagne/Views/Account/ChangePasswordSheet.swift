import SwiftUI
import SparagneCore

/// Changing the account's password (`POST /auth/password`), opened from
/// Settings.
///
/// The server checks the current password under the same limits as a login
/// and signs out every other device of the account; this one keeps its
/// session. The new password is checked here first, against the server's
/// rule and the confirmation, so a typo never costs a failed attempt.
struct ChangePasswordSheet: View {
    let engine: SyncEngine
    @Environment(\.dismiss) private var dismiss

    @State private var current = ""
    @State private var newPassword = ""
    @State private var confirmation = ""
    @State private var busy = false
    @State private var failure: String?
    @State private var failureDetail: String?
    @State private var done = false

    private var newProblem: String? {
        guard !newPassword.isEmpty, !AccountRules.isValidPassword(newPassword) else { return nil }
        return String(localized: "The new password needs at least 8 characters.")
    }

    private var confirmationProblem: String? {
        guard !confirmation.isEmpty, confirmation != newPassword else { return nil }
        return String(localized: "The two new passwords do not match.")
    }

    private var canSubmit: Bool {
        !busy && !current.isEmpty && AccountRules.isValidPassword(newPassword) && confirmation == newPassword
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(String(localized: "Change Password")).font(.headline)
            if done {
                Text(String(localized: "Your password was changed. Your other devices were signed out and need to log in again with the new password."))
                    .font(.callout)
                    .fixedSize(horizontal: false, vertical: true)
                HStack {
                    Spacer()
                    Button(String(localized: "Done")) { dismiss() }
                        .keyboardShortcut(.defaultAction)
                }
            } else {
                form
            }
        }
        .padding(20)
        .frame(width: 420)
    }

    @ViewBuilder
    private var form: some View {
        Form {
            SecureField(String(localized: "Current password"), text: $current)
            SecureField(String(localized: "New password"), text: $newPassword)
            if let newProblem { Self.hint(newProblem) }
            SecureField(String(localized: "Confirm new password"), text: $confirmation)
            if let confirmationProblem { Self.hint(confirmationProblem) }
        }
        .formStyle(.grouped)

        Text(String(localized: "Your other devices will be signed out."))
            .font(.callout)
            .foregroundStyle(.secondary)

        if let failure {
            VStack(alignment: .leading, spacing: 2) {
                Text(failure).font(.callout).foregroundStyle(.red)
                if let failureDetail {
                    Text(failureDetail).font(.caption).foregroundStyle(.secondary)
                }
            }
            .fixedSize(horizontal: false, vertical: true)
        }

        HStack {
            if busy { ProgressView().controlSize(.small) }
            Spacer()
            Button(String(localized: "Cancel"), role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
            Button(String(localized: "Change Password")) { submit() }
                .keyboardShortcut(.defaultAction)
                .disabled(!canSubmit)
        }
    }

    private static func hint(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.secondary)
    }

    private func submit() {
        let old = current
        let replacement = newPassword
        busy = true
        failure = nil
        failureDetail = nil
        Task {
            do {
                try await engine.changePassword(current: old, new: replacement)
                done = true
            } catch let error as ServerError {
                // On this route a 401 is the current password, not the
                // session.
                failure = error.isUnauthorized ? String(localized: "The current password is not correct.") : error.summary
                failureDetail = error.isUnauthorized ? nil : error.detail
            } catch {
                failure = error.localizedDescription
            }
            busy = false
        }
    }
}
