import Foundation
import SparagneCore

/// A request the server refused, or a network failure.
///
/// `code` is the server's stable snake_case code (`ErrorBody`, `docs/v2/SYNC
/// .md` §3), so `ErrorMessages` can localize it exactly like a domain error.
/// A failure below HTTP — no route, timeout, a non-HTTP answer — is
/// `status == 0` with the code `offline`.
nonisolated struct ServerError: Error, Equatable, Sendable {
    let status: Int
    let code: String
    let message: String
    /// Seconds the server asked to wait, from the `Retry-After` of a `429`
    /// (login and register limits, `docs/v2/SYNC.md` §3).
    let retryAfter: Int?

    init(status: Int, code: String, message: String, retryAfter: Int? = nil) {
        self.status = status
        self.code = code
        self.message = message
        self.retryAfter = retryAfter
    }

    static let offline = ServerError(status: 0, code: "offline", message: "")

    static func offline(detail: String) -> ServerError {
        ServerError(status: 0, code: "offline", message: detail)
    }

    static let notConfigured = ServerError(status: 0, code: "invalid_server_url", message: "")
    static let notLoggedIn = ServerError(status: 401, code: "unauthorized", message: "")

    var isOffline: Bool { code == "offline" }
    var isUnauthorized: Bool { status == 401 }
    /// A `403 forbidden`: the role does not allow it. `author_mismatch` is a
    /// 403 too, but it means the outbox was not relabelled, not a role.
    var isForbidden: Bool { status == 403 && code == "forbidden" }
    var isNotFound: Bool { status == 404 }
    var isRateLimited: Bool { status == 429 || code == "too_many_requests" }

    /// Nothing more in the round can succeed: the network is gone, or the
    /// server stopped accepting the token.
    var endsRound: Bool { isOffline || isUnauthorized }

    /// The headline for the alert; `message` stays as the detail line.
    var summary: String {
        isRateLimited ? ErrorMessages.tooManyAttempts(retryAfter: retryAfter) : ErrorMessages.summary(for: code)
    }

    /// The server's own English words, when they add something to the
    /// headline: a rate limit's "retry in N seconds" does not.
    var detail: String? {
        guard !message.isEmpty, !isRateLimited, message != code else { return nil }
        return message
    }
}

/// Who may do what with a vault (`vault_memberships.role`).
nonisolated enum MemberRole: String, Codable, Sendable, CaseIterable, Identifiable {
    case owner
    case editor
    case viewer

    var id: String { rawValue }

    /// Owner and editor may push; a viewer only reads.
    var canWrite: Bool { self != .viewer }

    var label: String {
        switch self {
        case .owner: String(localized: "Owner")
        case .editor: String(localized: "Editor")
        case .viewer: String(localized: "Viewer")
        }
    }
}

/// One row of `GET /vaults`: a vault as the server sees it, with my role.
nonisolated struct VaultSummary: Codable, Sendable, Equatable, Identifiable {
    let id: Uuid
    let name: String
    /// The currency code as the wire spells it (`"EUR"`); the app reads the
    /// local vault for anything it has to format.
    let currency: String
    let owner: String
    let role: MemberRole
    let lastSeq: Int64
}

nonisolated struct TokenResponse: Codable, Sendable, Equatable {
    let token: String
    /// Unix seconds.
    let expiresAt: Int64
    let username: String
}

nonisolated struct MemberEntry: Codable, Sendable, Equatable, Identifiable {
    let username: String
    let role: MemberRole

    var id: String { username }
}

private struct SetMemberRequest: Codable, Sendable {
    let username: String
    let role: MemberRole
}

private struct Credentials: Codable, Sendable {
    let username: String
    let password: String
}

private struct PasswordChange: Codable, Sendable {
    let currentPassword: String
    let newPassword: String
}

private struct MeResponse: Codable, Sendable {
    let username: String
}

private nonisolated struct ErrorBody: Codable, Sendable {
    struct Detail: Codable, Sendable {
        let code: String
        let message: String
    }

    let error: Detail
}

/// The HTTP API of `docs/v2/SYNC.md` §3, typed.
///
/// Only the small flat wire types are decoded here. Commands, envelopes and
/// push or pull bodies stay opaque strings that travel between the core and
/// the server untouched.
nonisolated struct ServerAPI: Sendable {
    let transport: SyncTransport

    init(transport: SyncTransport) {
        self.transport = transport
    }

    // MARK: - Auth

    func register(username: String, password: String) async throws -> TokenResponse {
        try await decode(
            call(
                SyncRequest(
                    method: "POST",
                    path: "/auth/register",
                    body: try encode(Credentials(username: username, password: password))
                )
            )
        )
    }

    func login(username: String, password: String) async throws -> TokenResponse {
        try await decode(
            call(
                SyncRequest(
                    method: "POST",
                    path: "/auth/login",
                    body: try encode(Credentials(username: username, password: password))
                )
            )
        )
    }

    func logout(token: String) async throws {
        _ = try await call(SyncRequest(method: "POST", path: "/auth/logout", bearer: token))
    }

    /// `POST /auth/password`. The token that asks stays valid; every other
    /// token of the account is revoked. A wrong current password is a `401`
    /// that counts as a failed login.
    func changePassword(token: String, currentPassword: String, newPassword: String) async throws {
        _ = try await call(
            SyncRequest(
                method: "POST",
                path: "/auth/password",
                bearer: token,
                body: try encode(PasswordChange(currentPassword: currentPassword, newPassword: newPassword))
            )
        )
    }

    func me(token: String) async throws -> String {
        let response: MeResponse = try await decode(call(SyncRequest(path: "/me", bearer: token)))
        return response.username
    }

    // MARK: - Vaults

    func vaults(token: String) async throws -> [VaultSummary] {
        try await decode(call(SyncRequest(path: "/vaults", bearer: token)))
    }

    /// `body` comes from `pushRequestJson`; the answer goes back to
    /// `applyPushResponseJson`. A vault the server has never seen is created
    /// by this same call, when the body starts with its `CreateVault`
    /// (`docs/v2/SYNC.md` §3).
    func push(token: String, vaultId: Uuid, body: String) async throws -> String {
        let data = try await call(
            SyncRequest(
                method: "POST",
                path: "/vaults/\(escape(vaultId))/push",
                bearer: token,
                body: Data(body.utf8)
            )
        )
        return String(decoding: data, as: UTF8.self)
    }

    /// The answer goes straight to `integratePullJson`.
    func pull(token: String, vaultId: Uuid, since: Int64, limit: Int) async throws -> String {
        let data = try await call(
            SyncRequest(
                path: "/vaults/\(escape(vaultId))/pull?since=\(since)&limit=\(limit)",
                bearer: token
            )
        )
        return String(decoding: data, as: UTF8.self)
    }

    // MARK: - Members

    func members(token: String, vaultId: Uuid) async throws -> [MemberEntry] {
        try await decode(
            call(SyncRequest(path: "/vaults/\(escape(vaultId))/members", bearer: token))
        )
    }

    func setMember(token: String, vaultId: Uuid, username: String, role: MemberRole) async throws {
        _ = try await call(
            SyncRequest(
                method: "PUT",
                path: "/vaults/\(escape(vaultId))/members",
                bearer: token,
                body: try encode(SetMemberRequest(username: username, role: role))
            )
        )
    }

    /// The owner removing a member, or a member removing themself: leaving
    /// the vault (`docs/v2/SYNC.md` §3).
    func removeMember(token: String, vaultId: Uuid, username: String) async throws {
        _ = try await call(
            SyncRequest(
                method: "DELETE",
                path: "/vaults/\(escape(vaultId))/members/\(escape(username))",
                bearer: token
            )
        )
    }

    // MARK: - Plumbing

    private func call(_ request: SyncRequest) async throws -> Data {
        let response = try await transport.send(request)
        guard (200..<300).contains(response.status) else {
            throw Self.error(from: response)
        }
        return response.body
    }

    private static func error(from response: SyncResponse) -> ServerError {
        let retryAfter = retryAfter(response.header("Retry-After"))
        if let body = try? JSONDecoder().decode(ErrorBody.self, from: response.body) {
            return ServerError(
                status: response.status,
                code: body.error.code,
                message: body.error.message,
                retryAfter: retryAfter
            )
        }
        return ServerError(
            status: response.status,
            code: fallbackCode(for: response.status),
            message: String(decoding: response.body, as: UTF8.self),
            retryAfter: retryAfter
        )
    }

    /// `Retry-After` in seconds, the only form the server sends; an HTTP
    /// date or anything else reads as "no hint".
    private static func retryAfter(_ value: String?) -> Int? {
        guard let value, let seconds = Int(value.trimmingCharacters(in: .whitespaces)), seconds >= 0
        else { return nil }
        return seconds
    }

    /// What a body without an `ErrorBody` means, by status
    /// (`docs/v2/SYNC.md` §3).
    private static func fallbackCode(for status: Int) -> String {
        switch status {
        case 400: "invalid_request"
        case 401: "unauthorized"
        case 403: "forbidden"
        case 404: "not_found"
        case 409: "already_exists"
        case 429: "too_many_requests"
        default: "server_error"
        }
    }

    private func decode<T: Decodable>(_ data: Data) throws -> T {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        do {
            return try decoder.decode(T.self, from: data)
        } catch {
            throw ServerError(status: 0, code: "invalid_response", message: "\(error)")
        }
    }

    private func encode(_ value: some Encodable) throws -> Data {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        return try encoder.encode(value)
    }

    private func escape(_ component: String) -> String {
        component.addingPercentEncoding(withAllowedCharacters: .alphanumerics.union(.init(charactersIn: "-._~")))
            ?? component
    }
}
