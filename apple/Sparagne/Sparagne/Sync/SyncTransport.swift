import Foundation

/// One HTTP call, reduced to what the sync protocol needs.
///
/// `path` is everything after the server's base URL, query string included
/// (`/vaults/<id>/pull?since=0&limit=500`). The body is opaque `Data`: the
/// engine hands over JSON the core wrote and never inspects a command
/// (`docs/v2/SYNC.md` §1).
struct SyncRequest: Sendable, Equatable {
    var method: String
    var path: String
    var bearer: String?
    var body: Data?

    init(method: String = "GET", path: String, bearer: String? = nil, body: Data? = nil) {
        self.method = method
        self.path = path
        self.bearer = bearer
        self.body = body
    }
}

/// The status line, the headers and the bytes; the API layer decides what
/// they mean.
struct SyncResponse: Sendable, Equatable {
    var status: Int
    var body: Data
    /// Header names are case-insensitive in HTTP, so they are kept
    /// lowercased and looked up through `header(_:)`. The protocol reads one:
    /// `Retry-After` on a `429` (`docs/v2/SYNC.md` §3).
    private(set) var headers: [String: String]

    init(status: Int, body: Data = Data(), headers: [String: String] = [:]) {
        self.status = status
        self.body = body
        self.headers = Dictionary(
            headers.map { ($0.key.lowercased(), $0.value) },
            uniquingKeysWith: { _, last in last }
        )
    }

    func header(_ name: String) -> String? {
        headers[name.lowercased()]
    }
}

/// What `ServerAPI` talks to. The tests replace it with a fake server backed
/// by a second core, so the whole engine runs without a network.
protocol SyncTransport: Sendable {
    func send(_ request: SyncRequest) async throws -> SyncResponse
}

/// The real transport: `URLSession` against the configured server.
///
/// Any transport-level failure (no route to host, timeout, a response that is
/// not HTTP) becomes `ServerError.offline`, which the engine shows as the
/// offline status rather than an error.
struct URLSessionTransport: SyncTransport {
    let baseURL: URL
    var timeout: TimeInterval = 20

    init(baseURL: URL, timeout: TimeInterval = 20) {
        self.baseURL = baseURL
        self.timeout = timeout
    }

    func send(_ request: SyncRequest) async throws -> SyncResponse {
        var base = baseURL.absoluteString
        while base.hasSuffix("/") { base.removeLast() }
        guard let url = URL(string: base + request.path) else {
            throw ServerError(status: 0, code: "invalid_server_url", message: base + request.path)
        }
        var urlRequest = URLRequest(url: url, timeoutInterval: timeout)
        urlRequest.httpMethod = request.method
        urlRequest.setValue("application/json", forHTTPHeaderField: "Accept")
        if let bearer = request.bearer {
            urlRequest.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        }
        if let body = request.body {
            urlRequest.httpBody = body
            urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        do {
            let (data, response) = try await URLSession.shared.data(for: urlRequest)
            guard let http = response as? HTTPURLResponse else {
                throw ServerError.offline(detail: url.absoluteString)
            }
            var headers: [String: String] = [:]
            for case let (name as String, value) in http.allHeaderFields {
                headers[name] = "\(value)"
            }
            return SyncResponse(status: http.statusCode, body: data, headers: headers)
        } catch let error as ServerError {
            throw error
        } catch {
            throw ServerError.offline(detail: error.localizedDescription)
        }
    }
}
