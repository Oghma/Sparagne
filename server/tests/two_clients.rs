//! Two-client end-to-end acceptance test of Sparagne v2 sync, per
//! `docs/v2/SYNC.md` §6.
//!
//! Two [`Core`]s in memory play alice and bob (and, for the permission test,
//! carol), driven over HTTP against `sparagne_server::router` in-process with
//! `tower::ServiceExt::oneshot` (see `common::Api`). The server's own log and
//! projection is reachable through `Api::state` for assertions. `common::Client`
//! implements the client-side algorithm of `docs/v2/SYNC.md` §4-§5.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::StatusCode;
use common::{Api, Client, T0, all_txns, basics, expense_cmd, flow_cmd, projection};
use serde_json::json;
use sparagne_core::{Core, FlowMode, SyncReport, sync::PushResponse};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// 1. Convergence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn convergence() {
    let api = Api::new();
    let (mut alice, mut bob, vault, wallet) = basics(&api).await;

    // Both write before either syncs, so their orders interleave on the
    // server: bob pushes first, so alice's expense lands after his.
    bob.exec(vault, expense_cmd(2_000, wallet, None, "spesa", T0 + 60));
    alice.exec(vault, expense_cmd(1_000, wallet, None, "bar", T0 + 120));

    let mut rebased = false;
    for _ in 0..2 {
        for report in bob.sync(&api, vault).await {
            rebased |= report.rebased;
        }
        for report in alice.sync(&api, vault).await {
            rebased |= report.rebased;
        }
    }
    assert!(rebased, "at least one sync must have rebased");

    let expected = {
        let core = api.state.core();
        projection(&core, vault)
    };
    assert_eq!(projection(&alice.core, vault), expected);
    assert_eq!(projection(&bob.core, vault), expected);

    let server_last_seq = { api.state.core().last_seq(vault).unwrap() };
    for client in [&alice, &bob] {
        let state = client.core.sync_state(vault).unwrap();
        assert_eq!(state.outbox, 0, "no outbox left");
        assert_eq!(state.rejected, 0, "no rejections in this scenario");
        assert_eq!(state.last_server_seq, server_last_seq);
    }

    // Every entity id (wallets, flows, categories, transactions) is the same
    // on every core: they derive from the command id, so the rebase produced
    // identical ids everywhere.
    let ids = |core: &Core| {
        let snapshot = core.snapshot(vault).unwrap();
        let mut out: Vec<Uuid> = snapshot.wallets.iter().map(|w| w.id).collect();
        out.extend(snapshot.flows.iter().map(|f| f.id));
        out.extend(core.categories(vault, true).unwrap().iter().map(|c| c.id));
        out.extend(
            core.list_transactions(vault, &all_txns(), 100, None)
                .unwrap()
                .items
                .iter()
                .map(|t| t.id),
        );
        out
    };
    let server_ids = {
        let core = api.state.core();
        ids(&core)
    };
    assert!(!server_ids.is_empty());
    assert_eq!(ids(&alice.core), server_ids);
    assert_eq!(ids(&bob.core), server_ids);
}

// ---------------------------------------------------------------------------
// 2. Rejection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rejection() {
    let api = Api::new();
    let (mut alice, mut bob, vault, wallet) = basics(&api).await;

    // A net-capped envelope with a 30.00 opening allocation; the cap of
    // 100.00 is never approached, the rejection below comes from overspending
    // the balance, not the cap.
    let vacanze = alice.exec(
        vault,
        flow_cmd("Vacanze", FlowMode::NetCapped { cap: 10_000 }, false, 3_000),
    );
    alice.sync(&api, vault).await;
    bob.sync(&api, vault).await;

    // Both spend from Vacanze without syncing first.
    alice.exec(
        vault,
        expense_cmd(2_500, wallet, Some(vacanze), "vacanza", T0 + 60),
    );
    let doomed = bob.exec(
        vault,
        expense_cmd(1_000, wallet, Some(vacanze), "vacanza", T0 + 60),
    );

    alice.sync(&api, vault).await;
    let reports = bob.sync(&api, vault).await;

    let rejected: Vec<_> = reports.iter().flat_map(|r| r.rejected.clone()).collect();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].command_id, doomed);
    assert_eq!(rejected[0].code, "insufficient_funds");

    assert!(
        !bob.core
            .list_transactions(vault, &all_txns(), 100, None)
            .unwrap()
            .items
            .iter()
            .any(|t| t.id == doomed),
        "the rejected expense must not appear in bob's projection"
    );

    let bob_rejected = bob.core.rejected_commands(vault).unwrap();
    assert_eq!(bob_rejected.len(), 1);
    assert_eq!(bob_rejected[0].command_id, doomed);
    assert_eq!(bob_rejected[0].code, "insufficient_funds");

    let expected = {
        let core = api.state.core();
        projection(&core, vault)
    };
    assert_eq!(projection(&alice.core, vault), expected);
    assert_eq!(projection(&bob.core, vault), expected);

    bob.core.dismiss_rejected(vault, doomed).unwrap();
    assert!(bob.core.rejected_commands(vault).unwrap().is_empty());
    assert_eq!(bob.core.sync_state(vault).unwrap().rejected, 0);
    // Dismissing a rejection never touches the projection.
    assert_eq!(projection(&bob.core, vault), expected);
}

// ---------------------------------------------------------------------------
// 3. Author and permissions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn author_and_permissions() {
    let api = Api::new();
    let (alice, mut bob, vault, wallet) = basics(&api).await;

    // Bob's outbox gets relabelled to someone else's username (e.g. an
    // account mix-up before login) and the server refuses it outright.
    bob.exec(vault, expense_cmd(500, wallet, None, "spesa", T0 + 60));
    bob.core.relabel_outbox(vault, "mallory").unwrap();

    let before_seq = api.state.core().last_seq(vault).unwrap();
    let request = bob.core.push_request(vault, common::BATCH).unwrap();
    let res = api
        .post(&format!("/vaults/{vault}/push"), &bob.token, json!(request))
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.code(), "author_mismatch");
    assert_eq!(
        api.state.core().last_seq(vault).unwrap(),
        before_seq,
        "a refused push applies nothing"
    );

    // Relabelling back lets the sync go through normally.
    bob.core.relabel_outbox(vault, "bob").unwrap();
    bob.sync(&api, vault).await;
    assert_eq!(bob.core.sync_state(vault).unwrap().outbox, 0);

    // Carol is not a member yet: a blind 404 on pull.
    let mut carol = Client::register(&api, "carol").await;
    let pull = api
        .get(&format!("/vaults/{vault}/pull?since=0"), &carol.token)
        .await;
    assert_eq!(pull.status, StatusCode::NOT_FOUND);
    assert_eq!(pull.code(), "not_found");

    // Made a viewer, she can join by pulling the whole log...
    let put = api
        .put(
            &format!("/vaults/{vault}/members"),
            &alice.token,
            json!({ "username": "carol", "role": "viewer" }),
        )
        .await;
    assert_eq!(put.status, StatusCode::NO_CONTENT, "{:?}", put.body);

    let report = carol.join(&api, vault).await;
    assert!(report.rebased);
    assert_eq!(projection(&carol.core, vault), projection(&bob.core, vault));

    // ...but her push is forbidden, and her local outbox command stays
    // unconfirmed.
    let doomed = carol.exec(vault, expense_cmd(100, wallet, None, "spesa", T0 + 120));
    let request = carol.core.push_request(vault, common::BATCH).unwrap();
    let res = api
        .post(
            &format!("/vaults/{vault}/push"),
            &carol.token,
            json!(request),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.code(), "forbidden");

    assert_eq!(carol.core.sync_state(vault).unwrap().outbox, 1);
    assert!(
        carol
            .core
            .outbox(vault)
            .unwrap()
            .iter()
            .any(|r| r.envelope.id == doomed)
    );
}

// ---------------------------------------------------------------------------
// 4. Idempotency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn idempotent_push() {
    let api = Api::new();
    let (mut alice, _bob, vault, wallet) = basics(&api).await;

    alice.exec(vault, expense_cmd(200, wallet, None, "spesa", T0 + 60));
    let request = alice.core.push_request(vault, common::BATCH).unwrap();

    let first_res = api
        .post(
            &format!("/vaults/{vault}/push"),
            &alice.token,
            json!(request.clone()),
        )
        .await;
    assert_eq!(first_res.status, StatusCode::OK);
    let first: PushResponse = first_res.json();
    alice.core.apply_push_response(vault, &first).unwrap();

    let before_seq = api.state.core().last_seq(vault).unwrap();

    // Re-sending the exact same body (a retried request after a lost reply,
    // say) must answer with the same seqs and not grow the log.
    let again_res = api
        .post(
            &format!("/vaults/{vault}/push"),
            &alice.token,
            json!(request),
        )
        .await;
    assert_eq!(again_res.status, StatusCode::OK);
    let again: PushResponse = again_res.json();
    assert_eq!(again, first, "a replayed push must answer the same seqs");

    let after_seq = api.state.core().last_seq(vault).unwrap();
    assert_eq!(
        after_seq, before_seq,
        "the server log must not grow on a replay"
    );

    // And applying the replayed response locally is a no-op, apart from the
    // server position every report carries.
    let noop = alice.core.apply_push_response(vault, &again).unwrap();
    assert_eq!(
        noop,
        SyncReport {
            server_last_seq: again.last_seq,
            ..SyncReport::default()
        }
    );
}
