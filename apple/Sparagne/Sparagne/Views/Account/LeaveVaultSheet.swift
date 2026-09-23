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
        VStack(alignment: .leading, spacing: 12) {
            Text(String(localized: "Leave Vault")).font(.headline)
            Text(String(localized: "Leave \u{201C}\(vault.name)\u{201D}?"))
            Text(String(localized: "Your changes still waiting to sync are sent first. Then the vault is removed from this Mac and you stop receiving its changes. Its owner and the other members keep it; only the owner can share it with you again."))
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

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
                // Destructive, so ↩ does not trigger it.
                Button(String(localized: "Leave"), role: .destructive) { leave() }
                    .disabled(busy)
            }
        }
        .padding(20)
        .frame(width: 420)
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
