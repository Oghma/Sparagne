import SwiftUI
import SparagneCore

/// A member leaving a shared vault: the membership goes on the server, the
/// local copy goes with `forgetVault` (`SyncEngine.leaveVault`).
///
/// It says what happens before it happens: pending changes go up first, the
/// vault leaves this Mac, the others keep it, and only the owner can bring
/// it back. When the server refuses the removal nothing is forgotten, and
/// the sheet stays open with the reason.
struct LeaveVaultSheet: View {
    let engine: SyncEngine
    let vault: VaultView
    @Environment(\.dismiss) private var dismiss

    @State private var busy = false
    @State private var failure: String?
    @State private var failureDetail: String?

    var body: some View {
        FormSheet(String(localized: "Leave Vault")) {
            ConfirmationText(
                question: String(localized: "Leave \u{201C}\(vault.name)\u{201D}?"),
                consequences: String(localized: "Your changes still waiting to sync are sent first. Then the vault is removed from this Mac and you stop receiving its changes. Its owner and the other members keep it; only the owner can share it with you again.")
            )
            if let failure {
                FormNote(failure, tone: .negative, detail: failureDetail, indented: false)
            }
        } footer: {
            if busy { ProgressView().controlSize(.small) }
            FormCancelButton { dismiss() }
            // Destructive, so ↩ does not trigger it.
            FormDestructiveButton(title: String(localized: "Leave")) { leave() }
                .disabled(busy)
        }
    }

    private func leave() {
        busy = true
        failure = nil
        failureDetail = nil
        Task {
            do {
                try await engine.leaveVault(vault.id)
                dismiss()
            } catch let error as ServerError {
                failure = error.summary
                failureDetail = error.detail
            } catch let error as DomainError {
                failure = ErrorMessages.summary(for: error.code)
                failureDetail = error.message
            } catch {
                failure = error.localizedDescription
            }
            busy = false
        }
    }
}
