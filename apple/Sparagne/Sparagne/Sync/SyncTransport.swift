import Foundation

/// One HTTP call: `path` is everything after the base URL; the body is the core's JSON, never inspected.
nonisolated struct SyncRequest: Sendable, Equatable {
    var method = "GET"
    var path: String
    var bearer: String?
    var body: Data?
}

/// The status, the headers and the bytes; `ServerAPI` decides what they mean.
nonisolated struct SyncResponse: Sendable, Equatable {
    var status: Int
    var body: Data
    /// Header names are case-insensitive in HTTP, so they are kept
    /// lowercased and looked up through `header(_:)`. The protocol reads one:
    /// `Retry-After` on a `429`.
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

/// What `ServerAPI` talks to; the tests put a fake server on a second core behind it.
nonisolated protocol SyncTransport: Sendable {
    func send(_ request: SyncRequest) async throws -> SyncResponse
}

/// `URLSession` against the server; any failure below HTTP is `ServerError.offline`.
nonisolated struct URLSessionTransport: SyncTransport {
    let baseURL: URL
    var timeout: TimeInterval = 20

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
