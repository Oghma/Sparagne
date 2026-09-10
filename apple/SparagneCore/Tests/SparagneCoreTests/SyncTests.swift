import Foundation
import Testing

@testable import SparagneCore

/// The sync surface as the app will use it: the core writes the push body, the
/// app does the HTTP, the core reads the answers back. Swift never decodes a
/// command, so these tests move the wire JSON around without ever turning it
/// into a Swift type.

/// The `commands` array of a push body, as raw JSON objects.
private func commandObjects(in body: String) throws -> [[String: Any]] {
    let root = try #require(
        try JSONSerialization.jsonObject(with: Data(body.utf8)) as? [String: Any])
    return try #require(root["commands"] as? [[String: Any]])
}

private func encode(_ object: [String: Any]) throws -> String {
    String(decoding: try JSONSerialization.data(withJSONObject: object), as: UTF8.self)
}

/// A push response that accepted every command, numbering them from `firstSeq`.
private func acceptAll(_ body: String, firstSeq: Int) throws -> String {
    let results = try commandObjects(in: body).enumerated().map { index, command in
        ["command_id": command["id"] ?? "", "status": "applied", "seq": firstSeq + index]
            as [String: Any]
    }
    return try encode(["results": results, "last_seq": firstSeq + results.count - 1])
}

/// A pull response carrying the pushed commands back, numbered from `firstSeq`.
private func echoPull(_ body: String, firstSeq: Int) throws -> String {
    let commands = try commandObjects(in: body).enumerated().map { index, command in
        ["envelope": command, "seq": firstSeq + index, "result_id": NSNull(), "created_at": 0]
            as [String: Any]
    }
    return try encode(["commands": commands, "last_seq": firstSeq + commands.count - 1])
}

/// One client with a vault and a `Cash` wallet holding 100.00, both unpushed.
private struct Client {
    let core: CoreHandle
    let vaultId: Uuid
    let walletId: Uuid

    static let author = "alice"
    static let now: OffsetDateTime = "2026-03-01T12:00:00+01:00"

    init() throws {
        core = try CoreHandle.openInMemory()
        let vault = try core.execute(
            envelope: createVaultEnvelope(author: Self.author, name: "Casa", currency: .eur)
        )
        vaultId = try #require(vault.resultId)
        let wallet = try core.execute(
            envelope: newEnvelope(
                vaultId: vaultId,
                author: Self.author,
                command: .createWallet(name: "Cash", openingBalance: 10_000, occurredAt: Self.now)
            )
        )
        walletId = try #require(wallet.resultId)
    }

    func execute(_ command: Command) throws -> Receipt {
        try core.execute(
            envelope: newEnvelope(vaultId: vaultId, author: Self.author, command: command))
    }

    func transactions() throws -> [TransactionView] {
        try core.listTransactions(
            vaultId: vaultId,
            filter: TransactionFilter(includeVoided: true, includeTransfers: true),
            limit: 50,
            cursor: nil
        ).items
    }
}

@Test("A push body comes out of the core and its response goes back in")
func pushRoundTripsThroughJson() throws {
    let client = try Client()

    var state = try client.core.syncState(vaultId: client.vaultId)
    #expect(state.outbox == 2)
    #expect(state.lastServerSeq == 0)
    #expect(state.rejected == 0)

    let body = try client.core.pushRequestJson(vaultId: client.vaultId)
    #expect(try commandObjects(in: body).count == 2)

    let report = try client.core.applyPushResponseJson(
        vaultId: client.vaultId, json: try acceptAll(body, firstSeq: 1))
    #expect(report.confirmed == 2)
    #expect(report.received == 0)
    #expect(report.rebased == false)
    #expect(report.rejected.isEmpty)

    state = try client.core.syncState(vaultId: client.vaultId)
    #expect(state.outbox == 0)
    #expect(state.lastServerSeq == 2)
    #expect(try client.core.lastSeq(vaultId: client.vaultId) == 2)
}

@Test("A pull of one's own commands only stamps them, it does not rebase")
func pullOfOwnCommandsIsAFastPath() throws {
    let client = try Client()
    let body = try client.core.pushRequestJson(vaultId: client.vaultId)
    let before = try client.transactions()

    // The push went through but its response was lost, so the pull is what
    // confirms the commands.
    let report = try client.core.integratePullJson(
        vaultId: client.vaultId, json: try echoPull(body, firstSeq: 1))
    #expect(report.confirmed == 2)
    #expect(report.received == 0)
    #expect(report.rebased == false)
    #expect(try client.core.syncState(vaultId: client.vaultId).lastServerSeq == 2)
    #expect(try client.transactions().map(\.id) == before.map(\.id))

    // A second pull of the same records changes nothing.
    let again = try client.core.integratePullJson(
        vaultId: client.vaultId, json: try echoPull(body, firstSeq: 1))
    #expect(again.confirmed == 0)
    #expect(again.rebased == false)
}

@Test("A rejected push drops the command and lists it until dismissed")
func aRejectionIsListedAndDismissed() throws {
    let client = try Client()
    let opening = try client.core.pushRequestJson(vaultId: client.vaultId)
    _ = try client.core.applyPushResponseJson(
        vaultId: client.vaultId, json: try acceptAll(opening, firstSeq: 1))

    let flow = try client.execute(
        .createFlow(
            name: "Vacanze", mode: .unlimited, allowNegative: false, openingAllocation: 5000,
            occurredAt: Client.now))
    let flowId = try #require(flow.resultId)
    let spend = try client.execute(
        .expense(
            Entry(
                amount: 5000, walletId: client.walletId, flowId: flowId, category: "hotel",
                note: nil, occurredAt: Client.now)))
    let spendId = try #require(spend.resultId)
    #expect(try client.transactions().contains { $0.id == spendId })

    // The server takes the envelope but another member has emptied it in the
    // meantime, so the spend comes back refused.
    let body = try client.core.pushRequestJson(vaultId: client.vaultId)
    let ids = try commandObjects(in: body).map { $0["id"] ?? "" }
    #expect(ids.count == 2)
    let response = try encode([
        "results": [
            ["command_id": ids[0], "status": "applied", "seq": 3] as [String: Any],
            [
                "command_id": ids[1], "status": "rejected", "code": "insufficient_funds",
                "message": "insufficient funds in flow 'Vacanze'",
            ] as [String: Any],
        ],
        "last_seq": 3,
    ])

    let report = try client.core.applyPushResponseJson(vaultId: client.vaultId, json: response)
    #expect(report.confirmed == 1)
    #expect(report.rebased == true)
    #expect(report.rejected.count == 1)
    let rejection = try #require(report.rejected.first)
    #expect(rejection.commandId == spendId)
    #expect(rejection.kind == "expense")
    #expect(rejection.code == "insufficient_funds")
    #expect(rejection.message.contains("Vacanze"))

    #expect(!(try client.transactions().contains { $0.id == spendId }))
    #expect(try client.core.rejectedCommands(vaultId: client.vaultId) == report.rejected)
    #expect(try client.core.syncState(vaultId: client.vaultId).outbox == 0)

    try client.core.dismissRejected(vaultId: client.vaultId, commandId: rejection.commandId)
    #expect(try client.core.rejectedCommands(vaultId: client.vaultId).isEmpty)
    #expect(try client.core.syncState(vaultId: client.vaultId).rejected == 0)
}

@Test("Logging in relabels the outbox and the projection follows")
func relabelOutboxFollowsTheAccount() throws {
    let core = try CoreHandle.openInMemory()
    let vault = try core.execute(
        envelope: createVaultEnvelope(author: "local", name: "Casa", currency: .eur))
    let vaultId = try #require(vault.resultId)
    _ = try core.execute(
        envelope: newEnvelope(
            vaultId: vaultId,
            author: "local",
            command: .createWallet(name: "Cash", openingBalance: 10_000, occurredAt: Client.now)
        )
    )

    try core.relabelOutbox(vaultId: vaultId, author: "alice")

    let page = try core.listTransactions(
        vaultId: vaultId,
        filter: TransactionFilter(includeVoided: true, includeTransfers: true),
        limit: 50,
        cursor: nil
    )
    #expect(page.items.count == 1)
    #expect(page.items.allSatisfy { $0.createdBy == "alice" })
    #expect(try core.vaults().first?.owner == "alice")
}

@Test("A malformed body is a domain error, not a crash")
func malformedSyncBodiesAreDomainErrors() throws {
    let client = try Client()
    for bad in ["", "{", "{\"nope\": 1}"] {
        #expect(throws: DomainError.self) {
            try client.core.integratePullJson(vaultId: client.vaultId, json: bad)
        }
        #expect(throws: DomainError.self) {
            try client.core.applyPushResponseJson(vaultId: client.vaultId, json: bad)
        }
    }
}
