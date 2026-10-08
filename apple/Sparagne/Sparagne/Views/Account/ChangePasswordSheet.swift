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
        FormSheet(
            String(localized: "Change Password"),
            subtitle: done ? nil : String(localized: "Your other devices will be signed out.")
        ) {
            if done {
                Text(String(localized: "Your password was changed. Your other devices were signed out and need to log in again with the new password."))
                    .font(Face.ui(13))
                    .foregroundStyle(Ink.text)
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                form
            }
        } footer: {
            if done {
                FormPrimaryButton(title: String(localized: "Done")) { dismiss() }
            } else {
                if busy { ProgressView().controlSize(.small) }
                FormCancelButton { dismiss() }
                FormPrimaryButton(title: String(localized: "Change Password")) { submit() }
                    .disabled(!canSubmit)
            }
        }
    }

    private var form: some View {
        FormGroup {
            FormRow(String(localized: "Current password")) {
                FormTextField(label: String(localized: "Current password"), text: $current, secure: true)
            }
            FormRow(String(localized: "New password")) {
                FormTextField(label: String(localized: "New password"), text: $newPassword, secure: true)
            }
            if let newProblem { FormNote(newProblem) }
            FormRow(String(localized: "Confirm new password")) {
                FormTextField(label: String(localized: "Confirm new password"), text: $confirmation, secure: true)
            }
            if let confirmationProblem { FormNote(confirmationProblem) }
            if let failure {
                FormNote(failure, tone: .negative, detail: failureDetail)
            }
        }
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
