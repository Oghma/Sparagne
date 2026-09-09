# Inventory: `server`, `api_types`, `migration` (+ `app`, `admin_cli`, config, ops)

Sources read in full: all files under `crates/server/src`, `crates/api_types/src/lib.rs`, `crates/migration/src/*`, `crates/app/src/*`, `crates/admin_cli/src/main.rs`, `config/config.toml`, `Dockerfile`, `.dockerignore`, `.github/**`, `rust-toolchain.toml`, `rustfmt.toml`, `.typos.toml`, root `Cargo.toml`, `README.md`, `docs/DEVELOPMENT.md`. Engine files were only spot-checked where a handler's semantics depend on them (noted inline). Workspace version at time of reading: `0.93.0`, branch `master`, HEAD `be41ad0`.

---

## 1. HTTP API

Router: `crates/server/src/server.rs:108-190` (`fn router`). Axum 0.8 (`{id}` path syntax). **Every route** sits behind `.route_layer(middleware::from_fn_with_state(state, auth))`, so all require HTTP Basic auth. There is **no health, version, or admin endpoint**. Nearly all operations are `POST` with a JSON body, including reads (commit `e621f8a` 2025-12-17 "make all the requests to POST"). `vault_id` is scoped via the JSON body on most routes and via the path on membership/share routes.

Authorization is enforced entirely in the engine. Where a test in `server.rs` pins the behavior it is cited; otherwise it is the engine's rule as observed.

### Auth / users
| Method | Path | Handler | Request DTO | Response | Semantics |
|---|---|---|---|---|---|
| POST | `/user/pair` | `user::pair` | `PairUser {code, telegram_id}` | `201` empty | Finds user whose `pair_code == code`, sets `telegram_id`, clears `pair_code`. Not found → `400 {"code":"bad_request","message":"user not found"}`. Caller is whatever Basic-auth user (the bot's service user in practice). DB errors → `400` with raw `DbErr` text. |
| DELETE | `/user/pair` | `user::unpair` | none | `202` empty | Finds user by `telegram_id` of the *resolved* user (see §2 telegram header), sets `telegram_id = NULL`. |

### Vaults
| Method | Path | Handler | Request DTO | Response | Semantics |
|---|---|---|---|---|---|
| POST | `/vault/new` | `vault::vault_new` | `VaultNew {name, currency?}` | `200 Vault{id,name,currency,owner}` | Creates vault owned by caller; currency defaults `EUR`. Returns 200 not 201. |
| POST | `/vault/list` | `vault::list` | `VaultList {}` (body `{}` required) | `VaultListResponse {vaults:[VaultView]}` | All vaults the caller can access (owned + shared). `shared = owner != caller`. |
| POST | `/vault/get` | `vault::get` | `Vault {id?, name?}` | `Vault` header | Needs `id` or `name` else `400 "id or name required"`. Engine `vault_header`; **flow-only members can read it** (test `flow_member_can_get_vault_header_but_not_snapshot`). |
| POST | `/vault/snapshot` | `vault::snapshot` | `Vault {id?, name?}` | `VaultSnapshot` | Full read model: wallets (sorted by lowercase name), flows (Unallocated first, then lowercase name), `unallocated_flow_id` (missing → `400 "missing Unallocated flow"`). Vault members only; flow-only member → `404`. `owner` = `vault.user_id`. |
| DELETE | `/vault/{id}` | `vault::delete` | none | `204` | Owner only; an editor gets `404` (test `vault_delete_is_owner_only`). |
| POST | `/stats/get` | `statistics::get_stats` | `Vault {id?, name?}` | `Statistic` | Resolves vault via `vault_snapshot`, then `vault_statistics(vault.id, user, include_voided=false)`. Owner only (`require_vault_owner`); viewer gets `404`. See §5. |

### Wallets
| Method | Path | Handler | Request DTO | Response | Semantics |
|---|---|---|---|---|---|
| POST | `/wallets` | `wallets::wallet_new` | `WalletNew {vault_id, name, opening_balance_minor, occurred_at}` | `201 WalletCreated {id}` | Trims name. Creates wallet with balance 0 via `new_wallet(vault_id, name, 0, user)`. If `opening_balance_minor != 0`: loads a full `vault_snapshot` to find the Unallocated flow, then posts an `income` (positive) or `expense` (negative, using `abs()`) on wallet+Unallocated with `category: Some("opening")` (free text, `category_id: None`), note `opening balance for wallet '<name>'`, no idempotency key, `occurred_at` converted to UTC. Two non-atomic engine calls. Viewer → `404`, editor → `201` (test `viewer_cannot_write_editor_can_write`). |
| PATCH | `/wallets/{id}` | `wallets::wallet_update` | `WalletUpdate {vault_id, name?, archived?}` | `200` empty | `400 "provide at least one of name or archived"` if both absent. `rename_wallet` then `set_wallet_archived` as two separate engine calls. Path id is `Uuid`. |

### Flows (cash flows / envelopes)
| Method | Path | Handler | Request DTO | Response | Semantics |
|---|---|---|---|---|---|
| POST | `/flows` | `flows::flow_new` | `FlowNew {vault_id, name, mode, opening_balance_minor, occurred_at, allow_negative=false}` | `201 FlowCreated {id}` | `opening_balance_minor < 0` → `400 "opening_balance_minor must be >= 0"`. Name trimmed. `mode` mapped by `map_mode`: `unlimited`→`(max_balance=None, income_bounded=None)`; `net_capped{cap_minor}`→`(Some(cap), None)`; `income_capped{cap_minor}`→`(Some(cap), Some(true))`. Engine `new_cash_flow(NewCashFlowParams{vault_id,name,balance:0,max_balance,income_bounded,allow_negative,user_id})`. If opening > 0: loads snapshot, `transfer_flow` Unallocated→new flow, note `opening allocation for flow '<name>'`. Non-atomic. |
| PATCH | `/flows/{id}` | `flows::flow_update` | `FlowUpdate {vault_id, name?, archived?, mode?, allow_negative?}` | `200` empty | `400 "provide at least one of name, archived, mode, or allow_negative"` if all absent. Up to 4 sequential engine calls: `rename_cash_flow`, `set_cash_flow_archived`, `set_cash_flow_mode(vault, id, max_balance, income_bounded.is_some_and(|v| v), user)`, `set_cash_flow_allow_negative`. |
| POST | `/flows/shared` | `flows::shared_list` | `FlowSharedList {vault_id, include_archived?}` (default false) | `FlowSharedListResponse {flows:[FlowView]}` | Engine `list_accessible_flows`: owner sees all flows incl. Unallocated; a flow-member sees only flows they are member of (test `shared_flow_list_scopes_to_accessible_flows`). Same sort as snapshot. |
| POST | `/cashFlow/get` | `cash_flow::get` | `CashFlowGet {vault_id, id?, name?}` | **`engine::CashFlow`** serialized directly | Lookup by `id` (`engine.cash_flow`), else by `name` (`engine.cash_flow_by_name`); neither → `400 "cash flow id or name required"`. Leaks the engine struct on the wire (fields: `id, name, balance, max_balance, income_balance, currency, archived, allow_negative, is_shared, is_reference, owner_user_id`; `system_kind` is `#[serde(skip)]`; `owner_user_id` skipped when None). Still used by the TUI client. |
| POST | `/vault/{vault_id}/flows/{flow_id}/share` | `flows::flow_share` | `FlowShareRequest {target_user_id, target_vault_name?, role: String}` | `200 FlowShareResponse {success:true}` | Cross-vault share: engine `share_flow_with_user(vault_id, flow_id, target_user_id, target_vault_name, role, user)` creates a `flow_memberships` row and a `flow_references` row in the target user's vault (their primary vault unless `target_vault_name`). Not called by the TUI or bot clients (grep found no caller). |
| DELETE | `/vault/{vault_id}/flow-references/{flow_id}` | `flows::flow_unshare` | none | `204` | Engine `remove_flow_reference`: removes the `flow_references` row only; flow and memberships untouched. |

### Categories
| Method | Path | Handler | Request DTO | Response | Semantics |
|---|---|---|---|---|---|
| POST | `/categories/list` | `categories::list` | `CategoryList {vault_id, include_archived?}` (default false) | `CategoryListResponse {categories:[CategoryView]}` | Engine `list_categories`. |
| POST | `/categories` | `categories::create` | `CategoryCreate {vault_id, name}` | `201 CategoryCreated {id, name}` | Engine `create_category`. |
| PATCH | `/categories/{id}` | `categories::update` | `CategoryUpdate {vault_id, name?, archived?}` | `200 CategoryView` | `400 "provide at least one of name or archived"` if both absent. Engine `update_category(vault, id, name, archived, user)` in one call. |
| POST | `/categories/{id}/aliases/list` | `categories::list_aliases` | `CategoryAliasList {vault_id}` | `CategoryAliasListResponse {aliases:[CategoryAliasView]}` | |
| POST | `/categories/{id}/aliases` | `categories::create_alias` | `CategoryAliasCreate {vault_id, alias}` | `201 CategoryAliasCreated {id, alias}` | |
| DELETE | `/categories/{category_id}/aliases/{alias_id}` | `categories::delete_alias` | `CategoryAliasDelete {vault_id}` (**JSON body on DELETE**) | `204` | |
| POST | `/categories/{id}/merge/preview` | `categories::preview_merge` | `CategoryMergePreview {vault_id, into_category_id}` | `CategoryMergePreviewResponse {ok, conflicts:[{kind, value}]}` | `kind` is the engine's `conflict.kind.as_str()`. Archived target produces a conflict (test `merge_preview_reports_conflicts`). |
| POST | `/categories/{id}/merge` | `categories::merge` | `CategoryMerge {vault_id, into_category_id}` | `200 CategoryView` (the surviving target) | Source `{id}` merged into `into_category_id`. |

### Transactions
| Method | Path | Handler | Request DTO | Response | Semantics |
|---|---|---|---|---|---|
| POST | `/transactions` | `transactions::list` | `TransactionList {vault_id, flow_id?, wallet_id?, limit?, cursor?, from?, to?, kinds?, include_voided?, include_transfers?}` | `TransactionListResponse {transactions:[TransactionView], next_cursor?}` | Exactly one of `flow_id`/`wallet_id`, or neither (vault-wide). Both → `400 "provide only one of flow_id or wallet_id"`. Defaults: `limit=50`, `include_voided=false`, `include_transfers=false`. `from`/`to` converted to UTC; `kinds` mapped to engine kinds; filter struct `engine::TransactionListFilter {from,to,kinds,include_voided,include_transfers}`. Flow-scoped → `list_transactions_for_flow_page`, wallet-scoped → `list_transactions_for_wallet_page` (engine returns `(Transaction, signed_amount)`), vault-wide → `list_transactions_for_vault_page` and sign computed in the handler: income `+`, expense `-`, refund `+`, **transfers `+`**. `occurred_at` returned with `+00:00` offset. `wallet_id`/`flow_id` on each item = first wallet leg / first flow leg found (`extract_wallet_flow_from_legs`). Flow viewer may list a shared flow (test). `from` inclusive, `to` exclusive (per DTO docs). |
| POST | `/transactions/get` | `transactions::get_detail` | `TransactionGet {vault_id, id}` | `TransactionDetailResponse {transaction: TransactionHeaderView, legs:[TransactionLegView]}` | Engine `transaction_with_legs`. Vault members only; flow-only member → `403`; wrong vault → `404` (tests). Header `amount_minor` is positive absolute. |
| POST | `/income` | `transactions::income_new` | `IncomeNew` | `201 TransactionCreated {id}` | Engine `income(IncomeCmd{vault_id, amount_minor, flow_id, wallet_id, meta: TxMeta{category_id, category, note, idempotency_key, occurred_at(UTC)}, user_id})`. |
| POST | `/expense` | `transactions::expense_new` | `ExpenseNew` | `201 TransactionCreated {id}` | Engine `expense(ExpenseCmd{…})`, same shape. |
| POST | `/refund` | `transactions::refund_new` | `Refund` | `201 TransactionCreated {id}` | Engine `refund(RefundCmd{…})`, same shape. |
| POST | `/transferWallet` | `transactions::transfer_wallet_new` | `TransferWalletNew {vault_id, amount_minor, from_wallet_id, to_wallet_id, note?, idempotency_key?, occurred_at}` | `201 TransactionCreated {id}` | Engine `transfer_wallet(TransferWalletCmd{…})`. |
| POST | `/transferFlow` | `transactions::transfer_flow_new` | `TransferFlowNew {vault_id, amount_minor, from_flow_id, to_flow_id, note?, idempotency_key?, occurred_at}` | `201 TransactionCreated {id}` | Engine `transfer_flow(TransferFlowCmd{…})`. |
| PATCH | `/transactions/{id}` | `transactions::update` | `TransactionUpdate` (all optional) | `200` empty | Engine `update_transaction(UpdateTransactionCmd{vault_id, transaction_id, user_id, amount_minor, wallet_id, flow_id, from_wallet_id, to_wallet_id, from_flow_id, to_flow_id, category_id, category, note, occurred_at})`; which fields apply depends on kind (per DTO docs). |
| POST | `/transactions/{id}/void` | `transactions::void_tx` | `TransactionVoid {vault_id, voided_at?}` | `200` empty | `voided_at` defaults to `Utc::now()`. Engine `void_transaction(vault, id, user, voided_at)`. Soft-delete. |

### Recurring templates
| Method | Path | Handler | Request DTO | Response | Semantics |
|---|---|---|---|---|---|
| POST | `/recurring` | `recurring::create` | `RecurringTemplateNew` | `201 RecurringTemplateCreated {id}` | `start_date`/`end_date` are `"YYYY-MM-DD"` strings parsed with `NaiveDate::parse_from_str(.., "%Y-%m-%d")`; parse failure → `400 "invalid start_date: …"` / `"invalid end_date: …"`. Engine `CreateRecurringCmd::new(vault_id, user, kind, amount_minor, frequency, day_of_period, start_date)` then fields `wallet_id, flow_id, category_id, category, note, end_date` set. |
| POST | `/recurring/list` | `recurring::list` | `RecurringTemplateList {vault_id, include_archived=false}` | `RecurringTemplateListResponse {templates:[RecurringTemplateView]}` | Engine `list_recurring`. Dates formatted `%Y-%m-%d`. |
| POST | `/recurring/pending` | `recurring::pending` | `PendingRecurringList {vault_id}` | `PendingRecurringListResponse {pending:[{template, period_date}]}` | "Today" = `Utc::now().date_naive()` (server UTC date, not user timezone). Engine `list_pending_recurring(vault, user, today)`. |
| GET | `/recurring/{id}` | `recurring::get` | `RecurringTemplateList {vault_id,…}` (**JSON body on GET**) | `RecurringTemplateView` | Engine `get_recurring`. |
| PATCH | `/recurring/{id}` | `recurring::update` | `RecurringTemplateUpdate` | `200` empty | Builder-style `UpdateRecurringCmd::new(vault_id, id, user)` with `.amount_minor/.wallet_id/.flow_id/.category_id/.category/.note/.frequency/.day_of_period/.end_date/.enabled` applied only when `Some`. `end_date: Option<Option<String>>` intended as tri-state (see §11). |
| POST | `/recurring/{id}/archive` | `recurring::archive` | `RecurringTemplateArchive {vault_id}` | `204` | Engine `archive_recurring`. |
| POST | `/recurring/{id}/execute` | `recurring::execute` | `RecurringExecute {vault_id}` | `201 RecurringExecuteResponse {transaction_id}` | Engine `execute_recurring(vault, id, user, today_utc)`: materializes the due occurrence as a real transaction. |

### Memberships / sharing
| Method | Path | Handler | Request DTO | Response | Semantics |
|---|---|---|---|---|---|
| GET | `/vault/{vault_id}/members` | `memberships::list_vault_members` | none | `MembersResponse {members:[{username, role}]}` | Engine returns `(username, role_string)`; `"owner"`→Owner, `"editor"`→Editor, anything else→Viewer. |
| POST | `/vault/{vault_id}/members` | `memberships::upsert_vault_member` | `MemberUpsert {username, role}` | `204` | Owner-only (module doc "owner-only"). Engine `upsert_vault_member(vault, username, role.as_str(), caller)`; refuses changing/removing the vault owner (`Forbidden` with specific messages, see §4). |
| DELETE | `/vault/{vault_id}/members/{username}` | `memberships::remove_vault_member` | none | `204` | |
| GET | `/vault/{vault_id}/flows/{flow_id}/members` | `memberships::list_flow_members` | none | `MembersResponse` | `flow_id` is `Uuid` in path. |
| POST | `/vault/{vault_id}/flows/{flow_id}/members` | `memberships::upsert_flow_member` | `MemberUpsert` | `204` | |
| DELETE | `/vault/{vault_id}/flows/{flow_id}/members/{username}` | `memberships::remove_flow_member` | none | `204` | Engine refuses removing the last flow owner. |

### Health / admin
None. Server entry points (`server.rs:192-235`): `run(engine, db)` hard-binds `127.0.0.1:3000` and logs errors; `run_with_listener(engine, db, listener) -> io::Result<()>` (used by the `app` launcher with the configured bind/port); `spawn_with_listener` runs it on a tokio task and returns the local `SocketAddr` (used by tests/TUI harnesses). `ServerState { engine: Arc<Engine>, db: DatabaseConnection }` is `Clone`.

### Client usage of routes (from grepping `crates/tui/src/client` and `crates/telegram_bot/src`)
- TUI client (`crates/tui/src/client/{categories,flows,members,recurring,stats,transactions,vaults,wallets}.rs`) calls every route above **except** `/user/pair` and `/vault/{..}/flows/{..}/share`. It calls `cashFlow/get`, `refund`, `transferWallet`, `transferFlow`, all recurring, all membership, `flow-references` delete, `stats/get`.
- Bot calls: `/vault/snapshot`, `/vault/list`, `/vault/get`, `/user/pair`, `/transactions`, `/transactions/get`, `/transactions/{id}`, `/transactions/{id}/void`, `/stats/get`, `/income`, `/expense`, `/categories`, `/categories/list`.
- The `/vault/{..}/flows/{..}/share` endpoint has no caller in the repo.

---

## 2. Auth model

**Mechanism:** HTTP Basic auth on every route, implemented in `server.rs:65-106` (`async fn auth`), extracted via `axum_extra::TypedHeader<Authorization<Basic>>`. Missing/malformed header → axum-extra typed-header rejection (`400`, plain text). Empty username or password → `401` (bare status, no body, **no `WWW-Authenticate` header**).

**User lookup (verbatim):**
```rust
user::Entity::find()
    .filter(user::Column::Username.contains(auth_header.username()))
    .filter(user::Column::Password.contains(auth_header.password()))
    .one(&state.db)
    .await
    .map_err(|_| StatusCode::UNAUTHORIZED)?;
```
SeaORM's `ColumnTrait::contains` generates `LIKE '%value%'` (documented SeaORM semantics; the local cargo registry has no extracted source to quote). So the check is a **case-insensitive substring match on both username and plaintext password**, returning the first matching row. Any DB error → `401`.

**Password storage:** plaintext. `admin_cli` writes the prompted password verbatim (`password: Set(password)`), the server compares it via SQL `LIKE`. No hashing, salting, or password-reset path anywhere in these crates.

**Telegram impersonation header:** optional header `telegram-user-id` (custom `TelegramHeader(String)` implementing `axum_extra::headers::Header`; static `HeaderName::from_static("telegram-user-id")`). If present, after Basic auth succeeds for *any* user, the middleware **replaces** the resolved user with the row whose `telegram_id` equals the header value (`401` if none). There is no check that the Basic-auth caller is the bot's service user; a "service user" is just an ordinary `users` row whose credentials are put in `[telegram]` config. The bot sends this header in three places in `crates/telegram_bot/src/api.rs` (lines 222, 254, 397).

**Per-request user:** the `user::Model {username, password, telegram_id, pair_code}` (including the plaintext password) is inserted into request extensions (`request.extensions_mut().insert(user)`); handlers take `Extension(user): Extension<user::Model>` and pass `user.username` to the engine as `user_id`. All authorization (owner / editor / viewer, vault vs. flow membership, cross-vault references) lives in the engine; the server has no role logic.

**Pairing flow:** admin creates a user with `--pair-code`; the bot (authenticated as the service user) posts `/user/pair {code, telegram_id}`; the server sets `telegram_id` and clears `pair_code`. Codes are single-use but never expire. Unpair is `DELETE /user/pair` with the telegram header.

**Absent:** sessions, tokens, refresh, logout, rate limiting, lockout, TLS (plain HTTP; nothing in the repo terminates TLS), CORS, per-user API keys, audit log, request timeouts (axum defaults; default 2 MB JSON body limit). No HTTP admin endpoints; administration is the `sparagne_admin` CLI on the DB file.

**Entity duplication:** the `users` SeaORM entity is defined three times with identical fields: `crates/server/src/user.rs`, `crates/engine/src/users.rs`, `crates/admin_cli/src/main.rs` (`mod users`).

---

## 3. `api_types` DTOs (wire contract, literal)

Crate deps: `chrono` (serde), `serde`, `uuid` (serde). All structs derive `Debug, Serialize, Deserialize` unless noted. Field names are the JSON keys (no `rename_all` on structs). `Uuid` serializes as hyphenated string; `DateTime<FixedOffset>` as RFC3339 with offset.

**Top level**
- `enum Currency { Eur }` — `#[serde(rename_all = "UPPERCASE")]` → `"EUR"`; `#[default] Eur`; derives `Clone, Copy, Debug, Default, PartialEq, Eq`.

**`cash_flow`**
- `CashFlowGet { vault_id: String, id: Option<Uuid>, name: Option<String> }` (doc: name = "legacy convenience").

**`wallet`**
- `WalletNew { vault_id: String, name: String, opening_balance_minor: i64 /* can be negative */, occurred_at: DateTime<FixedOffset> }`
- `WalletCreated { id: Uuid }`
- `WalletUpdate { vault_id: String, name: Option<String>, archived: Option<bool> }`

**`flow`**
- `enum FlowMode` — `#[serde(tag = "mode", rename_all = "snake_case")]`, `Clone, Copy, PartialEq, Eq`: `Unlimited`, `NetCapped { cap_minor: i64 }`, `IncomeCapped { cap_minor: i64 }`. JSON: `{"mode":"unlimited"}`, `{"mode":"net_capped","cap_minor":10000}`, `{"mode":"income_capped","cap_minor":…}`.
- `FlowNew { vault_id: String, name: String, mode: FlowMode, opening_balance_minor: i64 /* >= 0 */, occurred_at: DateTime<FixedOffset>, #[serde(default)] allow_negative: bool }`
- `FlowCreated { id: Uuid }`
- `FlowUpdate { vault_id: String, name: Option<String>, archived: Option<bool>, mode: Option<FlowMode>, allow_negative: Option<bool> }`
- `FlowSharedList { vault_id: String, include_archived: Option<bool> }`
- `FlowSharedListResponse { flows: Vec<vault::FlowView> }`
- `FlowShareRequest { target_user_id: String, target_vault_name: Option<String>, role: String }` (role is a raw string: "owner"/"editor"/"viewer")
- `FlowShareResponse { success: bool }`

**`vault`**
- `VaultNew { name: String, currency: Option<Currency> }`
- `Vault { id: Option<String>, name: Option<String>, currency: Option<Currency>, owner: Option<String> }` — all-optional bag used both as a *request* (get/snapshot/stats) and as the *response* of `vault_new`/`vault_get`.
- `VaultList {}` (derives `Default`) — empty struct; clients must send `{}`.
- `VaultView { id: String, name: String, currency: Currency, owner: String, shared: bool }` (`Clone`)
- `VaultListResponse { vaults: Vec<VaultView> }`
- `VaultSnapshot { id: String, name: String, currency: Currency, owner: Option<String>, wallets: Vec<WalletView>, flows: Vec<FlowView>, unallocated_flow_id: Uuid }`
- `WalletView { id: Uuid, name: String, balance_minor: i64, archived: bool }`
- `FlowView { id: Uuid, name: String, balance_minor: i64, archived: bool, is_unallocated: bool, #[serde(default)] allow_negative: bool, #[serde(default)] max_balance: Option<i64>, #[serde(default)] is_shared: bool, #[serde(default)] is_reference: bool, #[serde(skip_serializing_if = "Option::is_none")] owner_user_id: Option<String> }` — `#[serde(default)]` on the newer fields gives forward compatibility with older servers. Note: no `income_balance` or explicit mode on the view; only `max_balance`.

**`category`**
- `CategoryList { vault_id: String, include_archived: Option<bool> }`
- `CategoryView { id: Uuid, name: String, archived: bool, is_system: bool }`
- `CategoryListResponse { categories: Vec<CategoryView> }`
- `CategoryCreate { vault_id: String, name: String }`
- `CategoryCreated { id: Uuid, name: String }`
- `CategoryUpdate { vault_id: String, name: Option<String>, archived: Option<bool> }`
- `CategoryAliasList { vault_id: String }`
- `CategoryAliasView { id: Uuid, alias: String, category_id: Uuid }`
- `CategoryAliasListResponse { aliases: Vec<CategoryAliasView> }`
- `CategoryAliasCreate { vault_id: String, alias: String }`
- `CategoryAliasCreated { id: Uuid, alias: String }`
- `CategoryAliasDelete { vault_id: String }`
- `CategoryMerge { vault_id: String, into_category_id: Uuid }`
- `CategoryMergePreview { vault_id: String, into_category_id: Uuid }`
- `CategoryMergeConflict { kind: String, value: String }`
- `CategoryMergePreviewResponse { ok: bool, conflicts: Vec<CategoryMergeConflict> }`

**`error`**
- `enum ErrorCode` — `#[serde(rename_all = "snake_case")]`, `Clone, Copy, PartialEq, Eq`: `BadRequest, Conflict, CurrencyMismatch, DatabaseError, Forbidden, InsufficientFunds, InvalidAmount, InvalidCursor, InvalidFlow, InvalidId, InvalidName, InvalidRecurring, InvalidRole, MembershipLastOwner, MembershipOwnerImmutable, MembershipOwnerRemoveForbidden, MaxBalanceReached, NotFound, Unknown`. `Unknown` is never emitted by the server (client-side fallback).
- `type ErrorDetails = BTreeMap<String, String>`
- `ErrorEnvelope { error: ErrorPayload }`
- `ErrorPayload { code: ErrorCode, message: String, #[serde(skip_serializing_if = "Option::is_none")] details: Option<ErrorDetails> }`

**`user`**
- `PairUser { code: String, telegram_id: String }`

**`membership`**
- `enum MembershipRole { Owner, Editor, Viewer }` — `#[serde(rename_all = "snake_case")]`, `Clone, Copy, PartialEq, Eq`; `fn as_str(self) -> &'static str` → `"owner" | "editor" | "viewer"`. Docs: owner = full access + manage members; editor = write, no member mgmt; viewer = read-only.
- `MemberUpsert { username: String, role: MembershipRole }`
- `MembersResponse { members: Vec<MemberView> }`
- `MemberView { username: String, role: MembershipRole }`

**`stats`**
- `Statistic { currency: Currency, balance_minor: i64, total_income_minor: i64, total_expenses_minor: i64 }`

**`recurring`**
- `enum RecurrenceFrequency { Daily, Weekly, Monthly, Yearly }` — snake_case, `Clone, Copy, PartialEq, Eq`.
- `enum RecurringKind { Income, Expense }` — snake_case, `Clone, Copy, PartialEq, Eq`.
- `RecurringTemplateNew { vault_id: String, kind: RecurringKind, amount_minor: i64, wallet_id: Option<Uuid>, flow_id: Option<Uuid>, category_id: Option<Uuid>, category: Option<String>, note: Option<String>, frequency: RecurrenceFrequency, day_of_period: i32, start_date: String /* YYYY-MM-DD */, end_date: Option<String> }`
- `RecurringTemplateCreated { id: Uuid }`
- `RecurringTemplateView { id: Uuid, kind: RecurringKind, amount_minor: i64, wallet_id: Option<Uuid>, flow_id: Option<Uuid>, category_id: Uuid, note: Option<String>, frequency: RecurrenceFrequency, day_of_period: i32, start_date: String, end_date: Option<String>, enabled: bool, last_executed_date: Option<String> }` (`Clone`)
- `RecurringTemplateUpdate { vault_id: String, amount_minor: Option<i64>, wallet_id: Option<Uuid>, flow_id: Option<Uuid>, category_id: Option<Uuid>, category: Option<String>, note: Option<String>, frequency: Option<RecurrenceFrequency>, day_of_period: Option<i32>, end_date: Option<Option<String>> /* doc: null clears, absent = no change */, enabled: Option<bool> }`
- `RecurringTemplateList { vault_id: String, #[serde(default)] include_archived: bool }`
- `RecurringTemplateListResponse { templates: Vec<RecurringTemplateView> }`
- `RecurringTemplateArchive { vault_id: String }`
- `PendingRecurringList { vault_id: String }`
- `PendingRecurringView { template: RecurringTemplateView, period_date: String /* YYYY-MM-DD */ }` (`Clone`)
- `PendingRecurringListResponse { pending: Vec<PendingRecurringView> }`
- `RecurringExecute { vault_id: String }`
- `RecurringExecuteResponse { transaction_id: Uuid }`

**`transaction`**
- `enum TransactionKind { Income, Expense, TransferWallet, TransferFlow, Refund }` — snake_case (`"transfer_wallet"`, `"transfer_flow"`), `Clone, Copy, PartialEq, Eq`.
- `TransactionList { vault_id: String, flow_id: Option<Uuid>, wallet_id: Option<Uuid>, limit: Option<u64>, cursor: Option<String> /* opaque base64, newest→older */, from: Option<DateTime<FixedOffset>> /* inclusive */, to: Option<DateTime<FixedOffset>> /* exclusive */, kinds: Option<Vec<TransactionKind>>, include_voided: Option<bool>, include_transfers: Option<bool> }`
- `TransactionView { id: Uuid, kind: TransactionKind, occurred_at: DateTime<FixedOffset>, amount_minor: i64 /* signed for the selected target */, category_id: Uuid, category: Option<String>, note: Option<String>, voided: bool, #[serde(default, skip_serializing_if = "Option::is_none")] wallet_id: Option<Uuid>, #[serde(default, skip_serializing_if = "Option::is_none")] flow_id: Option<Uuid> }`
- `TransactionListResponse { transactions: Vec<TransactionView>, next_cursor: Option<String> }`
- `TransactionGet { vault_id: String, id: Uuid }`
- `enum LegTarget` — `#[serde(tag = "target", rename_all = "snake_case")]`, `Clone, Copy, PartialEq, Eq`: `Wallet { wallet_id: Uuid }`, `Flow { flow_id: Uuid }`.
- `TransactionLegView { #[serde(flatten)] target: LegTarget, amount_minor: i64, attributed_user_id: Option<String>, currency: Currency }` → JSON `{"target":"wallet","wallet_id":"…","amount_minor":…,"attributed_user_id":null,"currency":"EUR"}`.
- `TransactionHeaderView { id: Uuid, kind: TransactionKind, occurred_at: DateTime<FixedOffset>, amount_minor: i64 /* positive absolute */, currency: Currency, category_id: Uuid, category: Option<String>, note: Option<String>, voided: bool }`
- `TransactionDetailResponse { transaction: TransactionHeaderView, legs: Vec<TransactionLegView> }`
- `TransactionCreated { id: Uuid }`
- `IncomeNew`, `ExpenseNew`, `Refund` — identical shape: `{ vault_id: String, amount_minor: i64, flow_id: Option<Uuid>, wallet_id: Option<Uuid>, category_id: Option<Uuid>, category: Option<String>, note: Option<String>, idempotency_key: Option<String>, occurred_at: DateTime<FixedOffset> }` (`Refund` doc: "Must be > 0. The kind defines the sign of the legs.")
- `TransferWalletNew { vault_id: String, amount_minor: i64, from_wallet_id: Uuid, to_wallet_id: Uuid, note: Option<String>, idempotency_key: Option<String>, occurred_at: DateTime<FixedOffset> }`
- `TransferFlowNew { vault_id: String, amount_minor: i64, from_flow_id: Uuid, to_flow_id: Uuid, note: Option<String>, idempotency_key: Option<String>, occurred_at: DateTime<FixedOffset> }`
- `TransactionUpdate { vault_id: String, amount_minor: Option<i64> /* > 0 */, wallet_id: Option<Uuid> /* Income/Expense/Refund */, flow_id: Option<Uuid>, from_wallet_id: Option<Uuid> /* TransferWallet */, to_wallet_id: Option<Uuid>, from_flow_id: Option<Uuid> /* TransferFlow */, to_flow_id: Option<Uuid>, category_id: Option<Uuid>, category: Option<String>, note: Option<String>, occurred_at: Option<DateTime<FixedOffset>> }`
- `TransactionVoid { vault_id: String, voided_at: Option<DateTime<FixedOffset>> /* absent → server now() */ }`

**Notable contract features**
- Money is always `i64` minor units (`*_minor`), never decimals or strings; `Currency` is a separate enum with only `EUR`.
- Time: inputs are `DateTime<FixedOffset>` (RFC3339 with offset); the server normalizes to UTC (`with_timezone(&Utc)`) before the engine. Responses re-attach a fixed `+00:00` offset (`FixedOffset::east_opt(0)`), despite doc comments saying "local user time". Recurring dates are plain `"YYYY-MM-DD"` strings.
- Idempotency: `idempotency_key: Option<String>` in the **body** of all five create endpoints (not a header), backed by a unique index `(vault_id, idempotency_key)`.
- Pagination: opaque base64 `cursor` / `next_cursor`, newest→older, transactions only.
- Errors: `{"error":{"code":"…","message":"…","details":{…}}}` with stable snake_case codes.
- `category_id` (canonical) and `category` (free text, resolved to a category/alias by the engine) coexist on every create/update.
- Kind/role/frequency enums are duplicated between `api_types` and `engine`, with hand-written mapping functions in `transactions.rs` (`map_kind`, `map_currency`, `map_leg_target`), `recurring.rs` (`map_kind_to_engine`, `map_kind_from_engine`, `map_frequency_to_engine`, `map_frequency_from_engine`), `memberships.rs` (inline string match), `vault.rs` and `statistics.rs` (inline `Currency` match).
- `server::types::*` (`lib.rs:20-63`) re-exports a partial subset of DTOs plus `engine::CashFlow` (legacy shim; no membership/recurring/error types).

---

## 4. Error mapping (`crates/server/src/lib.rs:65-193`)

`pub enum ServerError { Engine(EngineError), Generic(String) }`, `impl From<EngineError>`, `impl IntoResponse` → `(status, Json(ErrorEnvelope { error: ErrorPayload {code, message, details} }))`.

| `EngineError` variant | HTTP | `ErrorCode` | `details` |
|---|---|---|---|
| `Forbidden("cannot remove last flow owner")` | 403 | `membership_last_owner` | `{scope: flow_membership, reason: last_owner}` |
| `Forbidden("cannot change vault owner role")` | 403 | `membership_owner_immutable` | `{scope: vault_membership, reason: owner_immutable}` |
| `Forbidden("cannot remove vault owner")` | 403 | `membership_owner_remove_forbidden` | `{scope: vault_membership, reason: owner_remove_forbidden}` |
| `Forbidden(other)` | 403 | `forbidden` | none |
| `KeyNotFound` | 404 | `not_found` | none |
| `ExistingKey` | 409 | `conflict` | none |
| `Database(DbErr)` | 500 | `database_error` | none; message replaced by `"internal server error"`, real error logged via `tracing::error!("database error: …")` |
| `MaxBalanceReached` | 422 | `max_balance_reached` | none |
| `InsufficientFunds` | 422 | `insufficient_funds` | none |
| `InvalidAmount` | 422 | `invalid_amount` | `{field: amount_minor}` |
| `InvalidName` | 422 | `invalid_name` | `{field: name}` |
| `InvalidId` | 422 | `invalid_id` | `{field: id}` |
| `InvalidCursor` | 422 | `invalid_cursor` | `{field: cursor}` |
| `InvalidFlow` | 422 | `invalid_flow` | none |
| `InvalidRecurring` | 422 | `invalid_recurring` | none |
| `InvalidRole` | 422 | `invalid_role` | `{field: role}` |
| `CurrencyMismatch` | 422 | `currency_mismatch` | `{field: currency}` |
| `ServerError::Generic(msg)` | 400 | `bad_request` | none; `msg` verbatim |

Engine `EngineError` (from `crates/engine/src/error.rs`) has exactly these 14 variants: `MaxBalanceReached, InsufficientFunds, KeyNotFound, ExistingKey, InvalidAmount, InvalidName, InvalidId, InvalidCursor, InvalidFlow, InvalidRole, CurrencyMismatch, InvalidRecurring, Forbidden, Database(DbErr)`; `Display` strings like `"\"{0}\" key not found!"`, `"\"{0}\" already present!"`, `"Invalid amount: {0}"`, `"Forbidden: {0}"`, `"Max balance reached!"`.

Membership sub-codes are matched on the **exact English error string** from the engine (fragile coupling). `message` for non-DB errors is `EngineError`'s `Display`.

Not in the envelope: auth failures (bare `401`/`400`), axum `Json`/`Path` extractor rejections (plain-text `400`/`415`/`422`), and `405` for wrong methods. Authorization failures surface inconsistently: most non-member writes/reads return **404** (engine `KeyNotFound`), while transaction detail for a flow-only member returns **403**.

Unit tests in `lib.rs` pin 403/404/409/422/400 and the `membership_last_owner` code + `scope` detail.

---

## 5. Stats endpoints

Only one: `POST /stats/get` → `Statistic { currency, balance_minor, total_income_minor, total_expenses_minor }`.

Engine `vault_statistics(vault_id, user_id, include_voided: bool)` (`crates/engine/src/ops/vaults.rs:387-465`), owner-only via `require_vault_owner`, whole vault lifetime, no parameters beyond the vault (server always passes `include_voided = false`). Runs inside one DB transaction with raw SQL:
- `balance_minor` = `SELECT COALESCE(SUM(balance),0) FROM wallets WHERE vault_id = ? AND archived = 0` (materialized wallet balances, archived wallets excluded).
- `total_income_minor` = `SUM(amount_minor) FROM transactions WHERE vault_id = ? AND kind = 'income' AND voided_at IS NULL`.
- `total_expenses_minor` = `SUM(kind='expense') − SUM(kind='refund')` (same void condition).
- Voided transactions excluded; transfers excluded by construction (only three kinds summed). No date range, no monthly/range/by-category/by-flow/by-wallet aggregation on the server.

Everything richer (daily/monthly rollups, per-category breakdowns, sparklines) is computed **client-side in the TUI** from paged `/transactions` results (`crates/tui/src/app/actions/stats.rs`: "Accumulated daily and monthly totals from a set of transactions", `monthly_category_breakdowns: HashMap<(i32,u32), Vec<(String,i64)>>`, `compute_sparkline`, `build_monthly_rollup`).

---

## 6. DB schema (SQLite only; `sea-orm-migration` 1.0.1)

Conventions: UUIDs are **16-byte BLOBs** (`Uuid::as_bytes()`; migration backfill and engine both bind `id.as_bytes().to_vec()`), exposed on the wire as hyphenated strings (`vault_id` is passed as a `String` and parsed with `parse_vault_uuid`). Money is `BIGINT` minor units. `currency` is a `TEXT` code denormalized onto vaults, wallets, cash_flows, transactions, legs (default `'EUR'` on the first three; required on tx/legs). `kind`, `role`, `target_kind`, `frequency`, `system_kind` are free `TEXT` (no CHECK constraints). `occurred_at`/`voided_at` use sea-query `.timestamp()`; the engine maps them to `DateTimeUtc`, which sqlx stores in SQLite as TEXT (exact format not verified here). `recurring_templates` dates and `created_at`, and `flow_references.created_at`, are ISO `TEXT` strings handled by the engine as `String`/`NaiveDate` (`flow_references.created_at` is `DateTime<Utc>` in the engine entity). Balances on `wallets`/`cash_flows` are **materialized** by the engine, not derived from legs.

| Table | Columns | Keys / constraints / indexes |
|---|---|---|
| `users` | `username TEXT PK`, `password TEXT NN`, `telegram_id TEXT`, `pair_code TEXT` | no index on telegram_id/pair_code |
| `vaults` | `id BLOB PK`, `name TEXT NN`, `user_id TEXT NN`, `currency TEXT NN DEFAULT 'EUR'` | FK `fk-vaults-user_id: user_id → users.username` (no cascade). No unique on (user_id, name). |
| `wallets` | `id BLOB PK`, `name TEXT NN`, `balance BIGINT NN`, `currency TEXT NN DEFAULT 'EUR'`, `archived BOOL NN`, `vault_id BLOB NN` | FK `fk-wallets-vault_id → vaults.id` (no cascade); UNIQUE `idx-wallets-vault_id-name-unique (vault_id, name)` |
| `cash_flows` | `id BLOB PK`, `name TEXT NN`, `system_kind TEXT`, `balance BIGINT NN`, `max_balance BIGINT`, `income_balance BIGINT`, `currency TEXT NN DEFAULT 'EUR'`, `archived BOOL NN`, `vault_id BLOB NN`, `allow_negative BOOL NN DEFAULT false` (added m3) | FK `fk-cash_flows-vault_id → vaults.id` (no cascade); UNIQUE `idx-cash_flows-vault_id-name-unique (vault_id, name)` |
| `transactions` | `id BLOB PK`, `vault_id BLOB NN`, `kind TEXT NN`, `occurred_at TIMESTAMP NN`, `amount_minor BIGINT NN`, `currency TEXT NN`, `category TEXT`, `note TEXT`, `created_by TEXT NN`, `voided_at TIMESTAMP`, `voided_by TEXT`, `refunded_transaction_id BLOB`, `idempotency_key TEXT`, `category_id BLOB` (added m2) | FK `fk-transactions-vault_id → vaults.id ON DELETE CASCADE`; idx `idx-transactions-vault_id-occurred_at (vault_id, occurred_at)`; UNIQUE `idx-transactions-idempotency_key (vault_id, idempotency_key)`; idx `idx-transactions-created_by (created_by)`. **No FK** on `category_id`, `created_by`, `voided_by`, `refunded_transaction_id`. |
| `legs` | `id BLOB PK`, `transaction_id BLOB NN`, `target_kind TEXT NN`, `target_id BLOB NN`, `amount_minor BIGINT NN`, `currency TEXT NN`, `attributed_user_id TEXT` | FK `fk-legs-transaction_id → transactions.id ON DELETE CASCADE`; idx `idx-legs-transaction_id`; idx `idx-legs-target (target_kind, target_id)`; idx `idx-legs-target_id`. Polymorphic target (wallet or flow), no FK. |
| `vault_memberships` | `vault_id BLOB NN`, `user_id TEXT NN`, `role TEXT NN` | PK `(vault_id, user_id)`; FK `fk-vault_memberships-vault_id → vaults.id CASCADE`; FK `fk-vault_memberships-user_id → users.username CASCADE`; idx `idx-vault_memberships-user_id` |
| `flow_memberships` | `flow_id BLOB NN`, `user_id TEXT NN`, `role TEXT NN` | PK `(flow_id, user_id)`; FK `fk-flow_memberships-flow_id → cash_flows.id CASCADE`; FK `fk-flow_memberships-user_id → users.username CASCADE`; idx `idx-flow_memberships-user_id` |
| `categories` (m2) | `id BLOB PK`, `vault_id BLOB NN`, `name TEXT NN`, `name_norm TEXT NN`, `archived BOOL NN DEFAULT false`, `is_system BOOL NN DEFAULT false` | FK `fk-categories-vault_id → vaults.id` (no cascade); UNIQUE `idx-categories-vault_id-name_norm-unique (vault_id, name_norm)` |
| `category_aliases` (m2) | `id BLOB PK`, `vault_id BLOB NN`, `category_id BLOB NN`, `alias TEXT NN`, `alias_norm TEXT NN` | FK `fk-category_aliases-vault_id → vaults.id`; FK `fk-category_aliases-category_id → categories.id` (no cascade); UNIQUE `idx-category_aliases-vault_id-alias_norm-unique (vault_id, alias_norm)` |
| `recurring_templates` (m4) | `id BLOB PK`, `vault_id BLOB NN`, `kind TEXT NN`, `amount_minor BIGINT NN`, `wallet_id BLOB`, `flow_id BLOB`, `category_id BLOB NN`, `note TEXT`, `created_by TEXT NN`, `frequency TEXT NN`, `day_of_period INT NN`, `start_date TEXT NN`, `end_date TEXT`, `enabled BOOL NN DEFAULT true`, `last_executed_date TEXT`, `created_at TEXT NN`, `archived_at TEXT` | **No FKs, no indexes at all.** |
| `flow_references` (m5) | `id BLOB PK`, `vault_id BLOB NN`, `target_flow_id BLOB NN`, `display_name TEXT`, `created_at TEXT NN` | FK `fk_flow_references_vault_id → vaults.id CASCADE`; FK `fk_flow_references_target_flow_id → cash_flows.id CASCADE`; UNIQUE `idx_flow_references_vault_target_unique (vault_id, target_flow_id)`; idx `idx_flow_references_vault_id`; idx `idx_flow_references_target_flow_id` (note underscore naming vs. hyphen naming elsewhere) |

No `created_at`/`updated_at`/version columns on users, vaults, wallets, cash_flows, transactions, legs, memberships, categories. SQLite FK enforcement depends on `PRAGMA foreign_keys`; sqlx enables it by default (not verified in this repo's connect options).

**Migration history (`Migrator::migrations()` order in `crates/migration/src/lib.rs`):**
1. `m20251230_000000_init` — consolidated schema (`users, vaults, wallets, cash_flows, transactions, legs, vault_memberships, flow_memberships`). Replaced 11 earlier files deleted in commit `1f61aea` (2025-12-29, "refactor(migration): merge to single file"): `m20230309_180650_cash_flows`, `m20230309_214510_entries`, `m20230528_204409_wallets`, `m20230531_190127_vaults`, `m20230828_064600_users`, `m20251212_120000_currency`, `m20251214_090000_stable_ids`, `m20251215_090000_system_flows`, `m20251215_120000_transactions`, `m20251217_090000_idempotency_key`, `m20251217_120000_memberships`. UUIDs switched from text to BLOB in `ec65b4f` (2025-12-30, "refactor!(engine): use blobs for uuid"). `down` drops all 8 tables in reverse order.
2. `m20260115_000001_categories` (first commit `240f386` 2025-12-31 "add first version of categories") — creates `categories`, `category_aliases`, adds `transactions.category_id`, then a **data backfill** (`backfill_categories`): per vault inserts system category `Uncategorized` (`name_norm = "uncategorized"`, `is_system = true`); reads every transaction's free-text `category`, normalizes (`normalize_display`: trim + collapse whitespace; `normalize_key`: NFKD, strip combining marks, lowercase alphanumerics, single spaces), groups variants per vault, sorts by count desc / shorter display / earliest occurrence / lexical, and clusters via **Levenshtein** (`similarity_threshold`: 1 for ≤6 chars, else 2): the first variant becomes a canonical `categories` row, later near-duplicates become `category_aliases` (dedup on alias_norm); every transaction gets `category_id` and its `category` text rewritten to the canonical display (or `Uncategorized` + NULL text). Uses `unicode-normalization`. `down` drops both tables and the column.
3. *(no `000002` — numbering gap; no deleted file with that number exists in git history)*
4. `m20260209_000003_allow_negative_flows` (commit `3c90284` 2026-02-09) — `ALTER TABLE cash_flows ADD allow_negative BOOL NOT NULL DEFAULT false`.
5. `m20260210_000004_recurring_templates` (commit `3203b8b` 2026-02-09) — `recurring_templates`.
6. `m20260212_000005_flow_references` (commit `d197642` 2026-02-12) — `flow_references`; source comment says existing `flow_memberships` "will be migrated to flow_references via a separate data migration script or manual process", which does not exist in the repo.

Migrations run automatically at startup of `sparagne` (`Migrator::up(&db, None)` in `parse_database`) and `sparagne_admin` (`connect_db`), and in server tests against `sqlite::memory:`. Standalone CLI (`crates/migration/src/main.rs`): `cargo run -p migration -- up|down|fresh|status`, reading `DATABASE_URL` (default `sqlite:./sparagne.db?mode=rwc`, which differs from the app's default `data/sparagne.sqlite3`). The README notes the custom CLI exists to stay stable under Rust 1.92 instead of the upstream `sea-orm-cli`.

---

## 7. Config (`crates/app/src/settings.rs`, `config/config.toml`)

Parsed with the `config` crate (0.15), TOML only (`FileFormat::Toml`). Struct:
```
[app]      level: String                      # required; tracing level
[server]   (optional) database: Database      # required within section
                      bind: Option<String>    # default "127.0.0.1"
                      port: u16               # required
[telegram] (optional) token, server, username, password: String   # all required
[tui]      present in config.toml but NOT parsed by the app (consumed by the TUI crate):
           base_url, username, vault, timezone, low_balance_minor, undo_toast_secs
```
`enum Database { Memory, Sqlite(String) }` (derives `Deserialize, PartialEq`) is serde externally tagged → TOML `database = "Memory"` or `database = { Sqlite = "data/sparagne.sqlite3" }`. `Memory` becomes URL `sqlite::memory` (note: no trailing colon, unlike the `sqlite::memory:` used in tests); `Sqlite(p)` becomes `sqlite:{p}?mode=rwc`.

Source order as added to the builder (later overrides earlier), all `required(false)`:
1. `$XDG_CONFIG_HOME/sparagne/config.toml` or `$HOME/.config/sparagne/config.toml`
2. `config/config.toml` (relative to CWD)
3. `$SPARAGNE_CONFIG`

So effective precedence is `SPARAGNE_CONFIG` > `./config/config.toml` > XDG file. README/DEVELOPMENT.md list "1) SPARAGNE_CONFIG 2) XDG 3) config/config.toml", which reverses the middle two if read as priority.

Env overrides: `SPARAGNE_SERVER=host:port` (parsed with `rsplit_once(':')`; error if no `[server]` section, empty host, or bad port); `SPARAGNE_LOG` then `RUST_LOG` (EnvFilter syntax) override `app.level`; default filter `sparagne=L,telegram_bot=L,server=L,engine=L` (level lowercased, empty → `info`, invalid → `info` with stderr warning); `SPARAGNE_LOG_FILE=<path>` adds a non-blocking `tracing_appender` file layer (append mode; stderr layer always on, ANSI only when stderr is a TTY; open failure falls back to stderr). Settings are logged at `debug` with token/password omitted (`redacted_log_lines`: `app.level`, `server.bind`, `server.port`, `server.database`, `telegram.server`, `telegram.username`, or `*.disabled=true`). Shipped `config/config.toml`: `level="info"`, `port=3000`, `bind="0.0.0.0"`, SQLite at `data/sparagne.sqlite3`, `[tui]` with `base_url="http://127.0.0.1:3000"`, `username=""`, `vault="Main"`, `timezone="Europe/Rome"`, commented `low_balance_minor=2500`, `undo_toast_secs=5`; `[telegram]` commented out. README documents `tui.vault = "Main (owner)"` or `"id:<uuid>"` disambiguation for shared vaults.

---

## 8. App launcher & admin CLI

**`sparagne` (`crates/app`, package name `sparagne`)**: `Settings::new()` (exit with error if it fails), `init_tracing(&settings.app.level)` keeping the appender guard alive, then a `tokio::task::JoinSet`: if `[server]` present → `parse_database` (connect + `Migrator::up`), `engine::Engine::builder().database(db.clone()).build()`, bind `{bind}:{port}` (`bind` default `127.0.0.1`), `server::run_with_listener(engine, db, listener)`; if `[telegram]` present → `telegram_bot::Bot::builder().token(&token).server(&server, &username, &password).build()` then `bot.run()`. When the first task exits, `tasks.shutdown()` stops the rest. No signal handling for graceful shutdown seen. Errors inside tasks are logged, not propagated (exit code 0).

**`sparagne_admin` (`crates/admin_cli`, package name `sparagne_admin`)**, clap derive:
- Global: `--database-url` (env `DATABASE_URL`, default `sqlite:./sparagne.db?mode=rwc`); connects and runs migrations first.
- `user create --username <u> [--telegram-id <id>] [--pair-code <code>]` — prompts password twice via crossterm raw mode on stderr (masked with `*`, Backspace supported, Ctrl-C aborts, up to 3 attempts, non-empty required), refuses existing username (exit 1), inserts **plaintext** password.
- `vault create --owner <u> --name <n> [--currency EUR]` — owner must exist (exit 1); only `EUR`/`eur` accepted (exit 2 otherwise); calls `engine.new_vault(name, owner, Some(currency))` and prints `created vault: <name> (<id>)`.
- `vault delete --owner <u> --vault-id <uuid>` — owner must exist; calls `engine.delete_vault(vault_id, owner)`.
- Nothing else: no password reset/change, no user list/delete, no membership or pairing management, no wallet/flow bootstrap.

**`migration` binary**: `up` (default) | `down` | `fresh` | `status` on `DATABASE_URL`; unknown command prints usage and exits 2.

Note: `docs/DEVELOPMENT.md` says `cargo run -p server` and `cargo run -p telegram_bot`; both crates are `[lib]`-only (no `main.rs`), so those commands do not work.

---

## 9. Ops / tooling

- **Workspace** (`Cargo.toml`): members `app, admin_cli, engine, migration, server, telegram_bot, api_types, tui`; version `0.93.0`, edition 2024, `rust-version = "1.92"`, resolver 3, MIT, author Matteo Lisotto. Lints: `[workspace.lints.rust] unsafe_code = "forbid"`; `[workspace.lints.clippy] all/pedantic/nursery = allow (priority -1)`, `unwrap_used/expect_used/todo/dbg_macro = warn`. `[profile.release]`: `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`. Key deps: axum 0.8.7, axum-extra 0.12.2 (`typed-header`), sea-orm 1.0.1 (`sqlx-sqlite`, `runtime-tokio-rustls`), sea-orm-migration 1.0.1, reqwest 0.13.1 (`json`, `rustls`, no default features → no openssl), tokio 1.48, chrono 0.4.43 (`serde`, `clock`), chrono-tz 0.10, uuid 1.19, config 0.15.19, clap 4.5.53 (`derive`, `env`), teloxide 0.17, ratatui 0.30, crossterm 0.29, thiserror 2, tracing 0.1.44, tracing-subscriber 0.3.22, tracing-appender 0.2.4, unicode-normalization 0.1.23, base64 0.22, tower 0.5, http-body-util 0.1 (dev). Server crate deps: axum, axum-extra, chrono, engine, api_types, serde, sea-orm, tokio (`full`), tracing, uuid; dev: base64, http-body-util, migration, serde_json, tower.
- **`rust-toolchain.toml`**: `channel = "stable"`, `profile = "minimal"`, components `rustfmt, clippy, rust-analyzer`.
- **`rustfmt.toml`**: `reorder_imports = true`, `imports_granularity = "Crate"`, `wrap_comments = true`, `use_field_init_shorthand = true`, `format_code_in_doc_comments = true`, `edition = "2024"` — three nightly-only options, hence CI formats with nightly.
- **`.typos.toml`**: `extend-ignore-re = ["Sparagne"]`; `extend-words` allow `sparagne`, `ratatui`, and Italian UI words (`applicato, categorie, comando, contanti, dedicato, determinare, eliminato, impossibile, limite, possibile, preferenze, successivo, trasport, Vai, visibile, Visuale, visuale`).
- **`.gitignore`**: `/target`, `migration/target/`, `/data`, `config/telegram_bot_state.json`.
- **Dockerfile**: stage 1 `rust:1.92-bookworm`, `WORKDIR /sparagne`, `COPY ./ .`, installs `libsqlite3-dev libssl-dev pkg-config ca-certificates`, copies `libsqlite3.so.0*`, `libssl.so.*`, `libcrypto.so.*`, `libz.so.1` into `/sparagne/runtime-libs`, creates `/sparagne/data` and `/sparagne/artifacts`, builds `cargo build -p sparagne --release --locked` with BuildKit cache mounts for `/usr/local/cargo/registry`, `/usr/local/cargo/git`, `/sparagne/target`, copies the binary to `artifacts/`. Stage 2 `gcr.io/distroless/cc-debian12`: copies runtime libs to `/usr/lib/`, `/etc/ssl/certs/ca-certificates.crt`, the `sparagne` binary and `data/` dir (owner `10001:10001`), bakes `config/` into the image (`COPY --chown=10001:10001 config/ /sparagne/config/`), `EXPOSE 3000`, `USER 10001:10001`, `CMD ["/sparagne/sparagne"]`, `VOLUME /sparagne/data`. Only the app binary is shipped (no `sparagne_admin`), so user bootstrap needs a separate build or DB access from outside the container. libssl/libcrypto are copied although Rust deps use rustls (possibly unnecessary; not verified). `.dockerignore` excludes `.git`, `.gitignore`, `target`, `.idea`, `.vscode`, swap/log files, `config/*.sqlite3`, `data`. README run command mounts `./config` and `./data`.
- **CI `ci.yml`** (push to `master` + all PRs; concurrency group `ci-${ref}` cancel-in-progress; `permissions: contents: read`): `dtolnay/rust-toolchain@master` with `1.92.0` + clippy, and `nightly` + rustfmt; `Swatinem/rust-cache@v2`; steps: `cargo +nightly fmt --all -- --check`; `cargo +1.92.0 clippy --workspace --all-targets` (warnings **not** denied; comment: "Keep clippy non-blocking on warnings for now"); `cargo +1.92.0 clippy --workspace --all-targets -- -D clippy::unwrap_used -D clippy::expect_used`; `cargo +1.92.0 test --workspace --all-targets`.
- **`deps.yml`**: `rustsec/audit-check@v2` weekly (`0 8 * * 1`, Mondays 08:00 UTC) + `workflow_dispatch`; `permissions: issues: write`.
- **`docker.yml`**: `docker build --pull --tag sparagne:ci .` on push to master / PRs touching `Dockerfile`, `Cargo.toml`, `Cargo.lock`, `crates/**`, `config/**`.
- **`release.yml`**: on tag `v*`, `permissions: contents: write`, matrix `x86_64-unknown-linux-gnu` and `x86_64-unknown-linux-musl`, `cargo install cross --locked`, `cross build --release -p sparagne -p sparagne_admin -p sparagne_tui --target …`, packages `sparagne sparagne_admin sparagne_tui` into `dist/sparagne-<target>.tar.gz`, uploads via `softprops/action-gh-release@v2` (`fail_on_unmatched_files: true`). No macOS/ARM targets.
- **`typos.yml`**: `crate-ci/typos@v1.30.2` on push to master / PRs.
- **`dependabot.yml`**: cargo ecosystem, directory `/`, target branch `master`, daily, all deps grouped as `cargo-dependencies`, commit prefix `chore` / `chore(dev)` with scope.

---

## 10. Tests

No `crates/server/tests/` directory (and none in `migration` or `api_types`). All tests are inline `#[cfg(test)]`.

`crates/server/src/lib.rs` `mod tests` (6): `engine_forbidden_maps_to_403`, `engine_not_found_maps_to_404`, `engine_conflict_maps_to_409`, `engine_validation_maps_to_422` (uses `InvalidAmount`), `generic_maps_to_400`, `membership_forbidden_includes_code` (deserializes the body, asserts `ErrorCode::MembershipLastOwner` and `details.scope == "flow_membership"`).

`crates/server/src/server.rs` `mod http_tests` (13), harness `setup()`: `Database::connect("sqlite::memory:")` + `Migrator::up`, users `owner`/`pw` and `alice`/`pw` inserted directly via the server's `user::ActiveModel`, `Engine::builder().database(db).build()`, `router(state)`; requests via `tower::ServiceExt::oneshot` with a hand-built `Basic base64(user:pw)` header and `serde_json` bodies:
1. `flow_member_can_list_transactions_for_flow_but_cannot_get_detail` — owner creates vault "Main" + flow "Shared", adds `alice` as flow viewer, posts an income; alice: `POST /transactions` (flow-scoped) → 200; `POST /transactions/get` → 403.
2. `flow_member_can_get_vault_header_but_not_snapshot` — alice (flow viewer): `POST /vault/get` by name → 200; `POST /vault/snapshot` → 404.
3. `vault_delete_is_owner_only` — alice as vault editor: `DELETE /vault/{id}` → 404; owner → 204.
4. `vault_stats_are_owner_only` — alice as vault viewer: `POST /stats/get` → 404; owner → 200.
5. `shared_flow_list_scopes_to_accessible_flows` — flows "Shared A"/"Shared B", alice viewer on A only: alice's `POST /flows/shared` returns exactly A; owner's returns A, B and the Unallocated flow.
6. `viewer_cannot_write_editor_can_write` — alice as vault viewer: `POST /wallets` → 404, `POST /income` → 404; after upgrade to editor both → 201.
7. `vault_owner_can_get_transaction_detail_and_wrong_vault_is_404` — detail 200 with matching id; same id with `vault_id: "other"` → 404.
8. `vault_owner_can_list_transactions_vault_wide` — vault-wide list contains the created income.
9. `vault_owner_can_manage_categories_and_aliases` — create "Spese" 201, list contains it, alias "spesa" 201, alias list contains it, alias delete 204.
10. `vault_owner_can_merge_categories` — merge "Food" into "Spese" → 200, response id == Spese.
11. `merge_preview_reports_conflicts` — target archived → `ok == false`, conflicts non-empty.
12. `vault_owner_can_create_and_update_wallet` — "Bank" with opening 1234 → 201; `PATCH` name "Bank X" + archived → 200; verified via engine snapshot.
13. `vault_owner_can_create_and_update_flow` — "Vacanze" `net_capped` 10000 + opening 500 → 201; `PATCH` rename + `income_capped` 20000 → 200; snapshot shows `max_balance == Some(20000)` and `income_balance.is_some()`.

Not covered over HTTP: the auth middleware (bad credentials, substring matching, telegram header), pair/unpair, refund/transfers/update/void, recurring endpoints, membership endpoints, share/unshare, pagination cursors and filters, error envelopes for extractor rejections, wallet/flow opening balances with negative values, `/cashFlow/get`, `/vault/new`, `/vault/list`.

---

## 11. Warts, tech debt, security issues

**Security (serious)**
1. **Plaintext passwords** in `users.password`; no hashing anywhere (server, admin_cli).
2. **Auth uses `LIKE '%…%'` substring matching** on both username and password (`Column::contains`). Any substring of a real username plus any substring of its password authenticates; `%`/`_` act as wildcards; SQLite `LIKE` is ASCII case-insensitive; `.one()` picks an arbitrary first match. This is an authentication bypass.
3. **`telegram-user-id` header lets any authenticated user impersonate any paired user**; no service-user gating.
4. Plain HTTP only; no rate limiting, lockout, or brute-force protection; Basic creds on every request; the plaintext password rides in request extensions.
5. DB errors during auth collapse to `401`; `user::pair`/`unpair` return raw `DbErr` text in a `400` body.
6. `unpair` for a resolved user with `telegram_id = NULL` filters `telegram_id = NULL` (SQL semantics: matches nothing, so `400 user not found`) — dead path rather than exploit, but wrong.
7. Pair codes never expire; anyone holding the service credentials can pair any code.

**API design**
8. RPC-over-POST: reads are `POST` with JSON bodies; `GET /recurring/{id}` and `DELETE /categories/{..}/aliases/{..}` **require JSON bodies**, which many HTTP clients/proxies drop.
9. Inconsistent naming and scoping: `/cashFlow/get`, `/transferWallet`, `/transferFlow` (camelCase), `/flow-references` (kebab), `/vault/…` singular vs `/wallets`, `/flows`, `/categories` plural; `vault_id` in body for some routes and in path for others; `vault_id` is a `String` while every other id is `Uuid`.
10. `/cashFlow/get` returns the engine's `CashFlow` struct directly; `server::types` shim is stale.
11. `/vault/new` returns 200 (others 201); `/vault/list` requires an empty `{}` body; `Vault` DTO doubles as query and response.
12. 404 vs 403 for authorization failures is inconsistent (engine `KeyNotFound` hides unauthorized resources, except transaction detail).
13. `FlowShareRequest.role` is a raw string while `MemberUpsert.role` is an enum; `Refund` is not named `RefundNew`; kind/frequency/role enums duplicated engine↔api_types with mapping helpers in five files.
14. `RecurringTemplateUpdate.end_date: Option<Option<String>>` relies on plain serde; JSON `null` deserializes to the outer `None` (serde's Option-of-Option quirk), so "null clears end_date" very likely does not work without a custom deserializer. Not tested; verify.
15. Responses always carry `+00:00` offsets although DTO docs promise local time; recurring "today" and pending computation use the server's UTC date; recurring dates are strings, not typed.
16. Vault-wide transaction list gives transfers a positive sign; `wallet_id`/`flow_id` on list items are "first leg found", ambiguous for transfers.
17. Stats are lifetime totals only, owner-only; every richer view requires the client to page all transactions (the TUI does this).
18. No list pagination except transactions; no concurrency control (no `updated_at`/version/ETag) — a multi-client spreadsheet UI would suffer lost updates; no OpenAPI or schema document, `api_types` is the only contract.
19. `ErrorCode::Unknown` unused; membership error codes are derived by matching exact English strings from the engine.

**Handlers**
20. Non-atomic multi-step handlers: `wallet_new` and `flow_new` (create + opening transaction), `wallet_update`, `flow_update` (up to 4 engine calls) — partial failure leaves inconsistent state and no rollback at the HTTP layer.
21. `wallet_new`/`flow_new` load a full `vault_snapshot` only to find the Unallocated flow id; opening balances use the free-text category `"opening"` rather than a system category.
22. `run()` hard-codes `127.0.0.1:3000` (unused by the app).

**Schema**
23. `recurring_templates` has no foreign keys and no indexes; `transactions.category_id` has no FK; `legs.target_id` is polymorphic without FK; `categories`/`category_aliases`/`wallets`/`cash_flows`/`vaults` FKs lack `ON DELETE CASCADE` while `transactions`/memberships/`flow_references` have it (vault deletion order therefore matters and is engine-managed).
24. `kind`/`role`/`target_kind`/`frequency`/`system_kind` are unconstrained strings; currency denormalized on five tables with a single supported value.
25. Materialized `wallets.balance`, `cash_flows.balance`, `cash_flows.income_balance` can drift from legs; no audit/`created_at`/`updated_at` on core tables; `users` has no indexes on `telegram_id`/`pair_code`; no unique on `(vaults.user_id, name)` although the API looks vaults up by name.
26. Migration numbering gap (`000002` missing); `flow_references` migration promises a data migration that never landed; migration CLI default DB path differs from the app default; `init.down` drops the whole database; index naming mixes hyphens (`idx-…`) and underscores (`idx_…`).
27. `Database::Memory` produces `sqlite::memory` (no trailing colon) — unsure whether sqlx treats it as in-memory or as a file named `:memory`.

**Ops / docs**
28. Config precedence documented in the opposite order of the code for the two file sources; `[tui]` lives in the server config file.
29. Docker image bakes `config/` in and ships no admin CLI; release builds are Linux x86_64 only.
30. CI: clippy warnings non-blocking; formatting requires nightly; no coverage; no tests for the auth layer; `docs/DEVELOPMENT.md` references non-existent binaries (`cargo run -p server`, `cargo run -p telegram_bot`).
31. Three copies of the `users` entity (server, engine, admin_cli).

**Worth carrying over** (positive findings, for the rewrite): integer minor units + explicit currency; body-level idempotency keys backed by a unique index; opaque cursor pagination with from/to/kinds filters; structured error envelope with stable codes and `details`; header + legs transaction model with polymorphic wallet/flow targets and soft-void (`voided_at`/`voided_by`) plus `refunded_transaction_id` linkage; system Unallocated flow and Uncategorized category; flow modes (`unlimited` / `net_capped` / `income_capped`) plus `allow_negative`; category normalization (NFKD key, aliases, merge with preview, Levenshtein backfill); two-level memberships (vault, flow) with cross-vault `flow_references`; recurring templates with an explicit pending → execute confirmation step; opening balances modeled as ordinary transactions; `#[serde(default)]` forward-compatible views; the in-memory-SQLite + `tower::oneshot` HTTP test harness; redacted settings logging and `SPARAGNE_LOG`/`SPARAGNE_LOG_FILE` env conventions; distroless non-root Docker image with cache mounts.
