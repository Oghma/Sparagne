import Testing

@testable import SparagneCore

// The binding on its own, as CI builds it without the app. Everything the
// app's tests drive through the same binding is left to them.

@Test("The package links and a core opens in memory")
func coreOpensInMemory() throws {
    let core = try CoreHandle.openInMemory()
    #expect(try core.vaults().isEmpty)
}

@Test("A malformed id lifts into a domain error instead of panicking")
func malformedIdBecomesADomainError() throws {
    let core = try CoreHandle.openInMemory()
    do {
        _ = try core.snapshot(vaultId: "not-a-uuid")
        Issue.record("expected the lift to fail")
    } catch let error as DomainError {
        #expect(error.code == "invalid_command")
    }
}

@Test("A malformed body is a domain error, not a crash")
func malformedSyncBodiesAreDomainErrors() throws {
    let core = try CoreHandle.openInMemory()
    let vault = try core.execute(envelope: createVaultEnvelope(author: "alice", name: "Casa", currency: .eur))
    let vaultId = try #require(vault.resultId)
    for bad in ["", "{", "{\"nope\": 1}"] {
        #expect(throws: DomainError.self) {
            try core.integratePullJson(vaultId: vaultId, json: bad)
        }
        #expect(throws: DomainError.self) {
            try core.applyPushResponseJson(vaultId: vaultId, json: bad)
        }
    }
}
