//! Renaming and deleting a vault across members, over the real router
//! (`docs/v2/SYNC.md` §3 "Cancellazione del vault").
//!
//! Both are ordinary commands in the vault's log: the server applies them
//! with the core, the members receive them with the next pull. The server
//! keeps the memberships of a deleted vault, so that pull is still allowed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::StatusCode;
use common::{Api, Client, T0, basics, expense_cmd, wallet_cmd};
use serde_json::json;
use sparagne_core::{
    Command, CommandEnvelope, Currency,
    sync::{MemberEntry, PushOutcome, PushResponse, VaultSummary},
};

fn rename(name: &str) -> Command {
    Command::RenameVault {
        name: name.to_string(),
    }
}

async fn listed(api: &Api, token: &str) -> Vec<VaultSummary> {
    api.get("/vaults", token).await.json()
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_rename_reaches_the_listing_and_the_members() {
    let api = Api::new();
    let (mut alice, mut bob, vault, _) = basics(&api).await;

    alice.exec(vault, rename("Casa nuova"));
    alice.sync(&api, vault).await;

    for client in [&alice, &bob] {
        let vaults = listed(&api, &client.token).await;
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].name, "Casa nuova");
    }
    assert_eq!(
        api.state.core().vault(vault).unwrap().unwrap().name,
        "Casa nuova"
    );

    bob.sync(&api, vault).await;
    assert_eq!(bob.core.vault(vault).unwrap().unwrap().name, "Casa nuova");
}

#[tokio::test]
async fn a_rename_to_a_name_the_owner_already_uses_is_refused() {
    let api = Api::new();
    let (mut alice, mut bob, vault, _) = basics(&api).await;
    // A second vault of alice's the server knows and bob has never seen.
    let lavoro = alice.create_vault("Lavoro");
    alice.sync(&api, lavoro).await;
    assert_eq!(listed(&api, &alice.token).await.len(), 2);

    // bob may rename (he is an editor), but the name belongs to alice's
    // namespace: fine on his machine, refused by the server.
    let envelope = CommandEnvelope::new(vault, "bob", rename("lavoro"));
    let clash = envelope.id;
    bob.core.execute(envelope).unwrap();
    assert_eq!(bob.core.vault(vault).unwrap().unwrap().name, "lavoro");

    let reports = bob.sync(&api, vault).await;
    let rejected: Vec<_> = reports.iter().flat_map(|r| r.rejected.clone()).collect();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].command_id, clash);
    assert_eq!(rejected[0].code, "already_exists");
    assert_eq!(bob.core.vault(vault).unwrap().unwrap().name, "Casa");
    assert_eq!(api.state.core().vault(vault).unwrap().unwrap().name, "Casa");
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_owner_deletes_the_vault_for_everyone() {
    let api = Api::new();
    let (mut alice, mut bob, vault, wallet) = basics(&api).await;

    // bob writes before he learns of the deletion.
    let doomed = bob.exec(vault, expense_cmd(1_000, wallet, None, "bar", T0 + 60));

    alice.exec(vault, Command::DeleteVault);
    assert!(alice.core.vaults().unwrap().is_empty());
    let reports = alice.sync(&api, vault).await;
    assert!(reports.iter().all(|r| r.rejected.is_empty()));
    assert_eq!(alice.core.sync_state(vault).unwrap().outbox, 0);

    // Gone on the server and from every listing, for the owner and for the
    // member alike.
    assert_eq!(api.state.core().vault(vault).unwrap(), None);
    assert!(listed(&api, &alice.token).await.is_empty());
    assert!(listed(&api, &bob.token).await.is_empty());
    // The memberships stay, which is what lets bob pull the deletion.
    let members: Vec<MemberEntry> = {
        let res = api
            .get(&format!("/vaults/{vault}/members"), &bob.token)
            .await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        res.json()
    };
    assert_eq!(members.len(), 2);

    let reports = bob.sync(&api, vault).await;
    let rejected: Vec<_> = reports.iter().flat_map(|r| r.rejected.clone()).collect();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].command_id, doomed);
    assert_eq!(rejected[0].code, "not_found");
    assert_eq!(bob.core.vault(vault).unwrap(), None);
    assert!(bob.core.vaults().unwrap().is_empty());
    assert_eq!(bob.core.deleted_vaults().unwrap(), vec![vault]);
    let state = bob.core.sync_state(vault).unwrap();
    assert_eq!(state.outbox, 0);
    assert_eq!(state.rejected, 1);
    assert_eq!(
        state.last_server_seq,
        api.state.core().last_seq(vault).unwrap()
    );

    // Whatever anyone pushes from now on is refused, command by command.
    let res = api
        .post(
            &format!("/vaults/{vault}/push"),
            &alice.token,
            json!({ "commands": [CommandEnvelope::new(vault, "alice", wallet_cmd("Late", 0))] }),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let response: PushResponse = res.json();
    assert!(matches!(
        &response.results[0].outcome,
        PushOutcome::Rejected { code, .. } if code == "not_found"
    ));
}

#[tokio::test]
async fn an_editor_cannot_delete_the_vault() {
    let api = Api::new();
    let (_alice, bob, vault, _) = basics(&api).await;

    // bob's own core would refuse the command, so it is pushed by hand: the
    // server runs the same rule and answers per command, not per request.
    let res = api
        .post(
            &format!("/vaults/{vault}/push"),
            &bob.token,
            json!({ "commands": [CommandEnvelope::new(vault, "bob", Command::DeleteVault)] }),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let response: PushResponse = res.json();
    assert!(matches!(
        &response.results[0].outcome,
        PushOutcome::Rejected { code, .. } if code == "forbidden"
    ));
    assert!(api.state.core().vault(vault).unwrap().is_some());
    assert_eq!(listed(&api, &bob.token).await.len(), 1);
}

#[tokio::test]
async fn a_deleted_vaults_id_cannot_be_claimed_but_its_name_can_be_reused() {
    let api = Api::new();
    let (mut alice, _bob, vault, _) = basics(&api).await;
    alice.exec(vault, Command::DeleteVault);
    alice.sync(&api, vault).await;

    // The id stays taken for ever: a stranger forging its `CreateVault`
    // gets the same blind 404 as for any vault that is not theirs.
    let mallory = Client::register(&api, "mallory").await;
    let mut forged = CommandEnvelope::create_vault("mallory", "Mine", Currency::Eur);
    forged.id = vault;
    forged.vault_id = vault;
    let res = api
        .post(
            &format!("/vaults/{vault}/push"),
            &mallory.token,
            json!({ "commands": [forged] }),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert!(listed(&api, &mallory.token).await.is_empty());
    assert_eq!(mallory.core.vaults().unwrap().len(), 0);

    // The name is free again for its owner.
    let reborn = alice.create_vault("Casa");
    assert_ne!(reborn, vault);
    alice.sync(&api, reborn).await;
    let vaults = listed(&api, &alice.token).await;
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].id, reborn);
    assert_eq!(vaults[0].name, "Casa");
}
