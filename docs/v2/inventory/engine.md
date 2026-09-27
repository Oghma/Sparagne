# Engine crate feature inventory

Scope: `/Users/oghma/Documents/Projects/Sparagne/crates/engine/src/**` and `tests/`. Everything below is what the code does today, not what SPEC.md says. Divergences from SPEC are called out inline and collected in section 13.

Cross-cutting facts:

- `Engine` is `Clone + Debug` and holds only a SeaORM `DatabaseConnection`. There is no in-memory state. Every public op opens its own DB transaction through `Engine::with_tx`, which is itself `pub` and leaks SeaORM types into the public API.
- Ids: wallets, flows, categories, transactions, templates are `Uuid`. Vault ids cross the API boundary as `String` (`Vault.id`, `VaultHeader.id`, `Transaction.vault_id`, every `cmd.vault_id`). Invalid vault uuid strings become `KeyNotFound("vault not found")`, which differs from the usual `"vault not exists"`.
- Amounts everywhere are raw `i64` minor units. The `Money` type is a parse/format helper for clients only; the engine never uses it internally.
- Authorization denials are "blind 404" (`KeyNotFound`) except `transaction_with_legs`, which returns `Forbidden("forbidden")`.

## 1. Engine public API surface

**Construction**

- `Engine::builder() -> EngineBuilder`; `EngineBuilder::database(DatabaseConnection)`; `EngineBuilder::build().await -> Engine`.
- `Engine::with_tx(f)`: runs `f(engine_clone, &DatabaseTransaction)` inside one DB transaction. Public.

**Vaults** (`ops/vaults.rs`)

- `new_vault(name, user_id, currency: Option<Currency>) -> String`. Trims name; unique per owner case-insensitively (`ExistingKey`). Currency defaults to `Eur`. Atomically also creates: the system flow named `unallocated` with `system_kind = Unallocated`, a default wallet named `Cash` with balance 0, the system category `Uncategorized` (`is_system = true`), and a `vault_memberships` row `(vault, user, "owner")`. The default wallet and the owner membership row are not in SPEC.
- `delete_vault(vault_id, user_id)`: owner-level only. Raw SQL deletes in order: legs, transactions, category_aliases, categories, cash_flows, wallets, vaults. Does not touch `recurring_templates` (no FK, rows are orphaned). Memberships and flow_references rely on FK cascade.
- `vault_header(vault_id: Option<&str>, vault_name: Option<String>, user_id) -> VaultHeader { id, name, currency, owner }`. Both `None` gives `KeyNotFound("missing vault id or name")`. Access: owner, any vault member, or any flow member of any flow living in the vault.
- `vault_list(user_id) -> Vec<VaultHeader>`: union of owned vaults, vault-membership vaults, and vaults containing a flow the user is a flow member of. Sorted by `(is_shared, name.lowercase, owner.lowercase)` so owned vaults come first.
- `vault_snapshot(vault_id: Option<&str>, vault_name: Option<String>, user_id) -> Vault { id: String, name, cash_flow: HashMap<Uuid, CashFlow>, wallet: HashMap<Uuid, Wallet>, user_id, currency }`. Access: owner or vault member only, flow members are rejected. Excludes archived flows (direct and referenced) but includes archived wallets. Sets `is_shared` on direct flows that have at least one `flow_memberships` row. Referenced flows get `name` replaced by `flow_references.display_name` when set, `is_shared = true`, `is_reference = true`, `owner_user_id` from the owning vault (one extra query per referenced flow).
- `vault_statistics(vault_id, user_id, include_voided) -> (Currency, balance_minor, total_income_minor, total_expenses_minor)`. Owner-level only. `balance` = `SUM(wallets.balance) WHERE archived = 0`. `income` = `SUM(transactions.amount_minor) WHERE kind = 'income'`. `expenses` = `SUM(expense) - SUM(refund)`. Transfers excluded. All-time, no date range parameter. SPEC 5.2 describes monthly statistics; the code has no period support.

**Wallets** (`ops/wallets.rs`)

- `wallet(wallet_id, vault_id, user_id) -> Wallet { id, name, balance, currency, archived }`. Vault read access.
- `new_wallet(vault_id, name, balance_minor, user_id) -> Uuid`. Vault write. Name trimmed, unique case-insensitively per vault. Wallet is inserted with balance 0. If `balance_minor != 0` an opening transaction is created at `Utc::now()`: kind `Income` if positive, `Expense` if negative, `amount = abs`, note `opening balance for wallet '{name}'`, legs on the wallet and on `Unallocated`, category resolved from the literal string `"opening"` (this creates a real, user-visible category named `opening` on first use). Not in SPEC.
- `rename_wallet(vault_id, wallet_id, new_name, user_id)`: vault write; unique check excludes self.
- `set_wallet_archived(vault_id, wallet_id, archived, user_id)`: vault write. No balance check. Archived wallets still accept legs everywhere (no `ArchivedTarget` exists). Only effect of archiving: excluded from `resolve_wallet_id` default selection and from `vault_statistics` balance.
- There is no wallet delete and no `WalletKind` (SPEC 3.2 is not implemented).

**Flows** (`ops/flows.rs`)

- `cash_flow(cash_flow_id, vault_id, user_id) -> CashFlow`. Access via `require_flow_read` (see section 8), so flow members and reference holders can read.
- `cash_flow_by_name(name, vault_id, user_id) -> CashFlow`. Case-insensitive `LOWER(name)` match on direct flows only, archived included, references not searched. Access: vault read or flow membership.
- `list_accessible_flows(vault_id, user_id, include_archived) -> Vec<CashFlow>`. If the user is owner or vault member: all direct flows. Otherwise: only direct flows the user has a `flow_memberships` row for. In both cases it then appends every flow referenced into this vault, with no membership check on those. It does not set `is_shared`, `is_reference`, `owner_user_id`, and does not apply `display_name`. Inconsistent with `vault_snapshot`.
- `new_cash_flow(NewCashFlowParams { vault_id, name, balance, max_balance, income_bounded: Option<bool>, allow_negative, user_id }) -> Uuid`. Vault write. Name trimmed; `unallocated` reserved case-insensitively (`InvalidFlow("flow name is reserved")`); unique case-insensitively. `balance < 0 && !allow_negative` gives `InvalidAmount`. Flow row inserted with balance 0. If `balance > 0` an opening `TransferFlow` from `Unallocated` to the new flow is created at `Utc::now()` with note `opening allocation for flow '{name}'` and category Uncategorized. That transfer must pass the cap, and on an income-capped flow it counts toward `income_balance`. A negative opening balance with `allow_negative = true` passes validation and is then silently ignored.
- `delete_cash_flow(vault_id, cash_flow_id, archive: bool, user_id)`. Vault write. Refuses Unallocated. `archive = true` sets `archived`; otherwise hard `DELETE` of the row with no balance check and no leg cleanup, leaving orphan legs.
- `rename_cash_flow(vault_id, flow_id, new_name, user_id)`. Flow write. Reserved-name and system-flow guards. Uniqueness is checked against the caller's vault, which is the wrong vault when the flow is a reference.
- `set_cash_flow_archived(vault_id, flow_id, archived, user_id)`. Flow write; not on system flow. No balance check.
- `set_cash_flow_allow_negative(vault_id, flow_id, allow_negative, user_id)`. Flow write; not on system flow; refuses to turn off while balance is negative.
- `set_cash_flow_mode(vault_id, flow_id, max_balance: Option<i64>, income_capped: bool, user_id)`. Flow write; not on system flow. `None` gives Unlimited. `Some(cap)` with `income_capped = false` gives NetCapped and fails with `MaxBalanceReached` if current balance exceeds the cap. `Some(cap)` with `income_capped = true` recomputes `income_balance` as the SQL sum of positive, non-voided flow legs where `transactions.vault_id` equals the caller's vault, then fails if that exceeds the cap. Cap must be `> 0`.
- `share_flow_with_user(vault_id, flow_id, target_user_id, target_vault_name: Option<&str>, role, user_id)`. Source vault owner-level only. Target user must exist. Flow must be a direct, non-Unallocated flow. Target vault: the named vault resolved on behalf of the target user, else the target user's first owned vault ordered by name ascending. Upserts a `flow_memberships` row with the validated role. If a `flow_references` row already exists it returns. Otherwise it checks name conflicts in the target vault (direct flows by `LOWER(name)`, and existing references by `display_name` or the referenced flow's name) and, on conflict, sets `display_name = "{flow_name} ({source_owner_username})"`. Inserts the reference with `created_at = Utc::now()`.
- `remove_flow_reference(vault_id, flow_id, user_id)`. Write access on the vault holding the reference. Deletes the reference only; membership stays. `KeyNotFound("flow reference not found in this vault")` if nothing deleted.

**Categories** (`ops/categories.rs`), see section 4 for rules

- `list_categories(vault_id, user_id, include_archived) -> Vec<Category { id, name, archived, is_system }>`, ordered by `name` ascending in DB collation. Vault read.
- `create_category(vault_id, name, user_id) -> Category`. Vault write.
- `update_category(vault_id, category_id, name: Option<&str>, archived: Option<bool>, user_id) -> Category`. Vault write.
- `list_category_aliases(vault_id, category_id, user_id) -> Vec<CategoryAlias { id, alias, category_id }>`, ordered by alias. Vault read.
- `create_category_alias(vault_id, category_id, alias, user_id) -> CategoryAlias`. Vault write.
- `delete_category_alias(vault_id, category_id, alias_id, user_id)`. Vault write. `KeyNotFound("alias not exists")` if no row.
- `preview_category_merge(vault_id, from, into, user_id) -> CategoryMergePreview { ok, conflicts }`. Requires vault write even though it is read-only.
- `merge_category(vault_id, from, into, user_id) -> Category` (returns the target). Vault write.

**Transactions, read** (`ops/transactions/list.rs`, `write/detail.rs`)

- `list_transactions_for_flow(vault_id, flow_id, user_id, limit: u64, &TransactionListFilter) -> Vec<(Transaction, i64)>`. The `i64` is the flow leg's signed amount. Access via `require_flow_read`.
- `list_transactions_for_flow_page(vault_id, flow_id, user_id, limit, cursor: Option<&str>, &filter) -> (Vec<(Transaction, i64)>, Option<String>)`.
- `list_transactions_for_wallet(vault_id, wallet_id, user_id, limit, &filter) -> Vec<(Transaction, i64)>` and `list_transactions_for_wallet_page(...)`. Vault read. The wallet is not checked to belong to the vault; scoping comes from `transactions.vault_id`.
- `list_transactions_for_vault_page(vault_id, user_id, limit, cursor, &filter) -> (Vec<Transaction>, Option<String>)`. Vault read. No unpaged variant.
- `transaction_with_legs(vault_id, transaction_id, user_id) -> Transaction` with `legs` populated, ordered by leg id (random UUID order). Access: owner or vault membership row, else `Forbidden("forbidden")`. Transaction must belong to the vault, else `KeyNotFound("transaction not exists")`.
- All list functions return `Transaction` with `legs` empty.

**Transactions, write** (`ops/transactions/write/*`)

- `income(IncomeCmd) -> Uuid`, `expense(ExpenseCmd) -> Uuid`, `refund(RefundCmd) -> Uuid`. Generated by one macro over a shared `FlowWalletCmd`. Vault write on `cmd.vault_id`. Category resolved (section 4), flow resolved (`None` gives Unallocated; `Some` must be direct or referenced in the vault), wallet resolved (`None` gives the single non-archived wallet), note trimmed, `amount_minor > 0` enforced, idempotency dedupe. Legs: wallet and flow both get `+amount` for Income and Refund, `-amount` for Expense. `refunded_transaction_id` is always `None`.
- `transfer_wallet(TransferWalletCmd) -> Uuid`. `from != to` else `InvalidAmount`. Vault write. Both wallets must be direct in the vault. Category Uncategorized. Legs: from `-amount`, to `+amount`. Flows untouched. Wallets may go negative.
- `transfer_flow(TransferFlowCmd) -> Uuid`. `from != to`. If the user has vault write access, both flows resolve via `resolve_flow_id` (direct or referenced). Otherwise both flows need `require_flow_write` (flow membership owner/editor). Legs: from `-amount`, to `+amount`. Non-negativity and caps enforced; an incoming transfer counts as income for income-capped flows (test `income_capped_counts_transfers_in`).
- `update_transaction(UpdateTransactionCmd)`: section 6.
- `void_transaction(vault_id, transaction_id, user_id, voided_at: DateTime<Utc>)`: section 6.

**Recurring** (`ops/recurring.rs`), section 5 for semantics. Every one of these, including reads, requires vault write access.

- `create_recurring(CreateRecurringCmd) -> Uuid`
- `update_recurring(UpdateRecurringCmd)`
- `archive_recurring(vault_id, template_id, user_id)`: sets `archived_at` to RFC3339 now; refuses already-archived.
- `list_recurring(vault_id, user_id, include_archived) -> Vec<RecurringTemplate>`: no ordering.
- `get_recurring(vault_id, template_id, user_id) -> RecurringTemplate`: archived included.
- `list_pending_recurring(vault_id, user_id, as_of_date: NaiveDate) -> Vec<PendingRecurring>`
- `execute_recurring(vault_id, template_id, user_id, as_of_date: NaiveDate) -> Uuid` (created transaction id).
- There is no skip operation.

**Memberships and sharing** (`ops/memberships.rs`). All owner-level on the vault.

- `upsert_vault_member(vault_id, member_username, role, user_id)`: user must exist; role parsed (`owner|editor|viewer`, else `InvalidRole`); refuses to give `vault.user_id` a non-owner role (`Forbidden("cannot change vault owner role")`); insert or update.
- `remove_vault_member(vault_id, member_username, user_id)`: refuses `vault.user_id` (`Forbidden("cannot remove vault owner")`); silent if the member does not exist.
- `list_vault_members(vault_id, user_id) -> Vec<(username, role)>`, includes the owner row created by `new_vault`.
- `upsert_flow_member(vault_id, flow_id, member_username, role, user_id)`: flow must be direct in the vault and not Unallocated (`InvalidFlow("cannot share Unallocated")`); demoting a flow owner when they are the only owner gives `Forbidden("cannot remove last flow owner")`.
- `remove_flow_member(vault_id, flow_id, member_username, user_id)`: same last-owner protection; uses `require_flow_read` so referenced flows pass the existence check.
- `list_flow_members(vault_id, flow_id, user_id) -> Vec<(username, role)>`.
- `shared_flow_ids(vault_id, user_id) -> HashSet<Uuid>`: vault read; ids of direct flows with at least one membership row.

**Balances and maintenance** (`ops/balances.rs`)

- `recompute_balances(vault_id, user_id)`: vault write. Section 9.

## 2. Command structs

All in `commands.rs`. All have `new(...)` constructors plus chainable builder setters with the same names as the fields.

- `TxMeta { category_id: Option<Uuid>, category: Option<String>, note: Option<String>, idempotency_key: Option<String>, occurred_at: DateTime<Utc> }`. `category_id` wins over `category` when both set. `note` is trimmed and empty becomes `None`. `idempotency_key` that is blank after trim is rejected with `InvalidAmount("idempotency_key must not be empty")`. `occurred_at` is caller-supplied and can be any date, including future.
- `IncomeCmd`, `ExpenseCmd`, `RefundCmd`, identical shape: `{ vault_id: String, amount_minor: i64, flow_id: Option<Uuid>, wallet_id: Option<Uuid>, meta: TxMeta, user_id: String }`. `amount_minor` must be `> 0`; sign is derived from kind. Extra convenience setters `.category()`, `.category_id()`, `.note()`, `.idempotency_key()` write into `meta`. `flow_id = None` resolves to the vault's Unallocated flow. `wallet_id = None` resolves to the only non-archived wallet; zero wallets gives `KeyNotFound("missing wallet")`, more than one gives `InvalidAmount("wallet_id is required when more than one wallet exists")`. `flow_id = Some` may be a flow referenced into this vault (cross-vault).
- `TransferWalletCmd { vault_id, amount_minor, from_wallet_id: Uuid, to_wallet_id: Uuid, note: Option<String>, idempotency_key: Option<String>, occurred_at, user_id }`. No category, no `TxMeta`.
- `TransferFlowCmd { vault_id, amount_minor, from_flow_id: Uuid, to_flow_id: Uuid, note, idempotency_key, occurred_at, user_id }`.
- `UpdateTransactionCmd { vault_id, transaction_id, user_id, amount_minor: Option<i64>, wallet_id: Option<Uuid>, flow_id: Option<Uuid>, from_wallet_id: Option<Uuid>, to_wallet_id: Option<Uuid>, from_flow_id: Option<Uuid>, to_flow_id: Option<Uuid>, category_id: Option<Uuid>, category: Option<String>, note: Option<String>, occurred_at: Option<DateTime<Utc>> }`. `None` means keep. `note = Some("   ")` clears the note to `None` (test `update_transfer_wallet_can_change_endpoints_and_amount`). `category`/`category_id` `Some` re-resolves through the category chain; `Some("")` or `Some("   ")` resolves to Uncategorized. Target fields must match the kind or the call fails with `InvalidAmount("invalid update: unexpected ... fields")`.
- `CreateRecurringCmd { vault_id, user_id, kind: TransactionKind, amount_minor, wallet_id: Option<Uuid>, flow_id: Option<Uuid>, category_id: Option<Uuid>, category: Option<String>, note: Option<String>, frequency: RecurrenceFrequency, day_of_period: i32, start_date: NaiveDate, end_date: Option<NaiveDate> }`. Only `Income` and `Expense` kinds accepted. `wallet_id`/`flow_id` are stored unvalidated and only resolved at execute time with the same `None` defaults as above.
- `UpdateRecurringCmd { vault_id, template_id, user_id, amount_minor: Option<i64>, wallet_id: Option<Uuid>, flow_id: Option<Uuid>, category_id: Option<Uuid>, category: Option<String>, note: Option<String>, frequency: Option<RecurrenceFrequency>, day_of_period: Option<i32>, end_date: Option<Option<NaiveDate>>, enabled: Option<bool> }`. `end_date = Some(None)` clears the end date. `wallet_id`, `flow_id`, `note` can be set but never cleared. `note` is stored untrimmed.

## 3. Money handling

`Money(i64)` is `Copy, Ord, Hash, Default`, `#[repr(transparent)]`. Constructors `Money::new(minor)`, `minor()`.

`Money::parse_major(input, currency)` accepted grammar (EUR, `minor_units = 2`):

```
input   := WS* [ '-' | '+' ] WS* number WS*
number  := major [ SEP [ frac ] ]
major   := ASCII_DIGIT+
frac    := ASCII_DIGIT{1..minor_units}
SEP     := '.' | ','
```

- Exactly one separator allowed. `"1.000,50"` and `"1,000.50"` are rejected as `invalid amount` because both separators are folded to `.` before splitting.
- No thousands separators, no internal spaces, no currency symbols or codes, no leading separator (`".5"` rejected because `major` is empty). `"10."` is accepted as 1000. Leading zeros accepted.
- Fraction is right-padded, so `"10.5"` gives 1050. More fractional digits than `minor_units` gives `InvalidAmount("too many decimals")`. No rounding ever happens.
- Whitespace between sign and digits is allowed (`"- 5"` parses). `"+-5"` is rejected.
- Overflow on `major * 100 + frac` or negation gives `InvalidAmount("amount too large")`. Empty or whitespace-only gives `InvalidAmount("empty amount")`.
- `Money::format(currency)` produces `"{sign}{major}.{minor zero-padded to minor_units} {CODE}"`, for example `"-12.34 EUR"`, `"0.01 EUR"`. Currencies with zero minor units would print `"{abs} {CODE}"`. No grouping, no locale.

`Currency` has one variant `Eur` (`code() = "EUR"`, `minor_units() = 2`), serde `"EUR"`, `TryFrom<&str>` is trim plus ASCII-uppercase, unknown gives `CurrencyMismatch("unsupported currency: X")`. Every vault, wallet, flow, transaction and leg stores a currency and every load path checks it equals the vault currency (`CurrencyMismatch`).

## 4. Categories

Normalization (`util.rs`):

- `normalize_category_display(s)`: trim, error `InvalidName("category name must not be empty")` if empty, collapse any whitespace run to one space. This is the stored display name.
- `normalize_category_key(s)`: trim; iterate NFKD decomposition; drop combining marks; any `char::is_alphanumeric` char is pushed as its Unicode lowercase; any other char becomes a single space separator (never leading, never doubled); trim; error if empty. So `"  spesa!!!  "` gives `spesa`, `"Caffè"` gives `caffe`, `"Auto-Moto"` gives `auto moto`, `"!!!"` is an error.
- `validate_category_name(s) -> (display, key)`: both of the above plus `key == "uncategorized"` gives `InvalidName("category name is reserved")`.

Similarity guard (not in SPEC): `find_similar_category` runs Levenshtein over chars between the new key and `name_norm` of every non-system, non-archived category in the vault. Threshold is 1 when the key has at most 6 chars, else 2. Best match is the smallest distance, ties broken by shorter `name_norm`. A hit gives `InvalidName("category '<display>' too similar to existing '<name>'; use '<name>' to confirm")`. There is no confirm or force flag anywhere, so a category within the threshold of an existing one can never be created.

Free-text resolution chain, `resolve_category_input(category_id, input)`:

1. If `category_id` is `Some`: load by id within the vault (`KeyNotFound("category not exists")`), reject archived (`InvalidName("category is archived")`), accept system Uncategorized.
2. Else if `input` is `None` or blank after trim: use the vault's Uncategorized (found by `name_norm = "uncategorized"`; created as a system category if missing).
3. Else normalize; if key is `"uncategorized"`: same as step 2.
4. Exact match on `categories.name_norm` with `archived = false`: use it.
5. Exact match on `category_aliases.alias_norm`: use its category; if that category is archived, `InvalidName("category is archived")`.
6. Exact match on an archived category's `name_norm`: `InvalidName("category is archived")`.
7. Similarity guard as above.
8. Auto-create a new non-system category with the normalized display name and use it.

Selection result carries `name: Option<String>`; it is `None` for system Uncategorized and `Some(canonical stored name)` otherwise. That is what lands in `transactions.category`, so an Uncategorized transaction has `category = None` (test `category_and_note_are_trimmed_and_empty_becomes_none`).

Create: `create_category` runs `validate_category_name`, then rejects if `name_norm` matches any category or any alias in the vault (`ExistingKey(display)`), then runs the similarity guard, then inserts with `archived = false, is_system = false`.

Rename and archive: `update_category` refuses system categories entirely (`InvalidName("system categories cannot be modified")`), so Uncategorized cannot be archived or renamed. Rename validates and checks conflicts excluding self, with no similarity guard. If the display name changed, it runs `UPDATE transactions SET category = <new name> WHERE category_id = <id>` so the denormalized display stays in sync (test `rename_category_updates_transactions`). `archived` is applied as given. Archiving does not touch recurring templates that point at the category; their execution then fails with `category is archived`.

Aliases: `create_category_alias` refuses system and archived categories, validates the alias like a name (so `uncategorized` is reserved), refuses an alias equal to the category's own `name_norm` and any conflict with existing categories or aliases (`ExistingKey`). No similarity guard on aliases.

Merge, `merge_context`, conflicts in `CategoryMergeConflict { kind, value }` with `CategoryMergeConflictKind` and `as_str()`:

- `SameCategory` (`"same_category"`): `from == into`.
- `SourceSystem` (`"source_system"`): source is system.
- `TargetArchived` (`"target_archived"`): target is archived.
- `Alias` (`"alias_conflict"`): a source alias's `alias_norm` equals the target's `name_norm` or one of the target's aliases.
- `Name` (`"name_conflict"`): source `name_norm` differs from target's and equals one of the target's aliases. In practice unreachable through the engine because creation already forbids cross-table duplicates.

`preview_category_merge` returns `ok = conflicts.is_empty()` plus all conflicts. `merge_category` errors on the first conflict (`InvalidName` for the first three, `ExistingKey(value)` for `Alias`/`Name`), then: repoints all transactions (`category_id = into`, `category = into.name`, or `NULL` when the target is system Uncategorized), moves the source's aliases to the target, adds the source's display name as a new alias of the target when the norms differ (this happens even when the target is the system category, which `create_category_alias` would forbid), and archives the source. The source is never deleted. Merging an archived source is allowed; merging into Uncategorized is allowed.

## 5. Recurring templates

Not in SPEC at all.

Model `RecurringTemplate { id, vault_id: Uuid, kind: TransactionKind, amount_minor, wallet_id: Option<Uuid>, flow_id: Option<Uuid>, category_id: Uuid, note: Option<String>, created_by: String, frequency: RecurrenceFrequency, day_of_period: i32, start_date: NaiveDate, end_date: Option<NaiveDate>, enabled: bool, last_executed_date: Option<NaiveDate>, created_at: String, archived_at: Option<String> }`. Dates persist as `%Y-%m-%d` TEXT; `created_at`/`archived_at` are RFC3339 strings.

`RecurrenceFrequency`: `Daily`, `Weekly`, `Monthly`, `Yearly` (DB and serde strings `daily|weekly|monthly|yearly`).

`day_of_period` validation (`validate_day_of_period`): Daily ignores it; Weekly requires `1..=7` as ISO weekday with Monday = 1; Monthly requires `1..=28`; Yearly encodes `MMDD` as an integer and requires month `1..=12` and day `1..=28`. Days 29 to 31 are impossible by design.

Anchor computation, `compute_current_period_date(frequency, day_of_period, as_of)` (pure, `pub`): returns the most recent scheduled date on or before `as_of`. Daily: `as_of`. Weekly: walk back to the given weekday. Monthly: this month's day if `as_of.day >= day`, else previous month's (January wraps to December of the previous year). Yearly: this year's `MM-DD` if `as_of >= it`, else last year's. Day is clamped to 28 again here.

Timezone: everything is `NaiveDate`; the caller supplies `as_of_date`. The created transaction's `occurred_at` is `period_date` at `00:00:00 UTC`. No timezone is stored or considered.

`PendingRecurring { template, period_date }` = a template that is due. `list_pending_recurring(as_of)` selects templates with `enabled = true` and `archived_at IS NULL`, skips those with `start_date > as_of` or `end_date < as_of` (string comparison on ISO dates), computes `period_date`, and is due when `last_executed_date` is `None` or `< period_date`. Consequences worth knowing: only the single most recent period is ever pending, missed periods are never backfilled, and `period_date` can precede `start_date` (start 2026-02-20, monthly day 15, as-of 2026-02-25 yields a pending 2026-02-15).

Materialization, `execute_recurring(template_id, as_of)`: template must exist in the vault, be enabled and unarchived (else `KeyNotFound("recurring template not found")`). Recomputes `period_date` from `as_of`; if `last_executed_date >= period_date` gives `InvalidRecurring("template already executed for this period")`. Builds a `FlowWalletCmd` with the template's kind, amount, wallet_id, flow_id, `category_id`, note, `occurred_at = period_date 00:00 UTC`, and `idempotency_key = "recurring:{template_id}:{YYYYMMDD}"`. Creates the transaction inside the same DB transaction as the `last_executed_date = period_date` update, so both commit or neither. Returns the transaction id. `start_date`/`end_date` are not checked at execute time. `created_by` on the transaction is the executing user, not the template's `created_by`.

Idempotency of materialization: two layers. `last_executed_date` blocks re-execution of the same or earlier period. The idempotency key would dedupe a replay by the same user in the same vault; a different user hitting the same key would trip the DB unique index instead (see section 13).

Skip: no primitive exists. The only ways to move past a period are executing it, disabling the template, or archiving it.

`update_recurring` validates `amount > 0`, validates the effective `(frequency, day_of_period)` pair whenever either changes, re-resolves category if either category field is set, and patches only the `Some` fields. `enabled` toggles. It refuses archived templates.

## 6. Transaction update, void, detail

**Editable** via `update_transaction`: `amount_minor` (must stay `> 0`), `occurred_at`, `category`/`category_id`, `note`, and targets depending on kind: `wallet_id` and/or `flow_id` for Income/Expense/Refund; `from_wallet_id`/`to_wallet_id` for TransferWallet; `from_flow_id`/`to_flow_id` for TransferFlow. Supplying target fields of the wrong family gives `InvalidAmount("invalid update: unexpected ...")`. New from/to must differ.

**Immutable**: `id`, `kind`, `vault_id`, `currency`, `created_by`, `idempotency_key`, `refunded_transaction_id`, leg ids. A voided transaction cannot be updated (`InvalidAmount("cannot update a voided transaction")`).

**Balance re-application**: existing legs are loaded and paired with their domain `Leg`. For each leg the code emits `(target, old_amount, new_amount)` triples: same target gives `(t, old, new)`; retarget gives `(old_t, old, 0)` and `(new_t, 0, new)`. `preview_apply_leg_updates` folds these: wallets accumulate a delta on top of their current DB balance; flows are loaded once into an in-memory `CashFlow` preview and `apply_leg_change(old, new)` is called per triple, which enforces non-negativity (unless Unallocated or `allow_negative`), the NetCapped cap on the resulting balance, and the IncomeCapped rule on `income_total - max(old,0) + max(new,0)`. Only after every check passes does it update the transaction row, update each leg row in place (`target_kind`, `target_id`, `amount_minor`), and persist wallet and flow balances. All inside one DB transaction, so a failing invariant leaves nothing changed (test `update_expense_retarget_flow_fails_if_insufficient_and_is_atomic`).

Target validation on update uses the direct-only `require_wallet_in_vault` / `require_flow_in_vault`. A transaction whose flow leg targets a referenced (cross-vault) flow therefore cannot be updated at all from the recipient vault, even to change the note, because the existing flow id fails the direct lookup.

**Void**, `void_transaction(vault_id, transaction_id, user_id, voided_at)`: vault write; transaction must be in the vault and not already voided (`InvalidAmount("transaction already voided")`). Emits `(target, amount, 0)` for every leg and runs the same preview, so voiding is subject to invariants: voiding an income that funded a flow that has since been spent fails with `InsufficientFunds`, and voiding an expense on a NetCapped flow at its cap fails with `MaxBalanceReached`. Sets `voided_at` to the caller-supplied timestamp and `voided_by` to the user. Legs are kept unchanged. Void works for cross-vault flows because the balance path uses `resolve_flow_vault`.

**Detail payload**, `Transaction`:

```
Transaction {
  id: Uuid, vault_id: String, kind: TransactionKind, occurred_at: DateTime<Utc>,
  amount_minor: i64, idempotency_key: Option<String>, currency: Currency,
  category_id: Uuid, category: Option<String>, note: Option<String>,
  created_by: String, voided_at: Option<DateTime<Utc>>, voided_by: Option<String>,
  refunded_transaction_id: Option<Uuid>, legs: Vec<Leg>
}
Leg { id: Uuid, transaction_id: Uuid, target: LegTarget, amount_minor: i64,
      currency: Currency, attributed_user_id: Option<String> }
LegTarget = Wallet { wallet_id } | Flow { flow_id }   // serde tag "target", snake_case
TransactionKind serde/DB: income | expense | transfer_wallet | transfer_flow | refund
```

`attributed_user_id` and `refunded_transaction_id` are never populated by any code path.

## 7. Transaction listing

`TransactionListFilter { from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>, kinds: Option<Vec<TransactionKind>>, include_voided: bool, include_transfers: bool }`, `Default` is all `None`/`false`.

- Range is `[from, to)`. `from >= to` gives `InvalidAmount("invalid range: from must be < to")`.
- `kinds` is an allow-list; `Some(vec![])` gives `InvalidAmount("kinds must not be empty")`. When `kinds` is `Some`, `include_transfers` is ignored entirely.
- When `kinds` is `None` and `include_transfers` is false, `transfer_wallet` and `transfer_flow` are excluded.
- `include_voided = false` adds `voided_at IS NULL`.
- Sort is fixed: `occurred_at DESC, id DESC`. No sort options. No text search.
- Pagination is keyset. `limit + 1` rows are fetched; `next_cursor` is returned only when more exist. The cursor is base64url without padding of JSON `{ "occurred_at": <DateTime<Utc>>, "transaction_id": "<uuid string>" }` of the last returned row. Next page filter is `occurred_at < c OR (occurred_at = c AND id < c.transaction_id)`. Bad cursors give `InvalidCursor("invalid transactions cursor")`. `limit = 0` returns an empty page with no cursor. No upper bound on `limit`.
- For flow and wallet lists the row source is `legs` joined to `transactions`, filtered by `target_kind` and `target_id`, and additionally by `transactions.vault_id = <vault passed in>`. For a flow shared across vaults, each side therefore sees only the transactions that were created from its own vault, while the flow balance reflects both.

## 8. Access control (`ops/access.rs`)

Roles: `MembershipRole { Owner, Editor, Viewer }`, DB strings `owner|editor|viewer`, anything else `InvalidRole`. `can_write()` is Owner or Editor; `is_owner()` is Owner. Internal `AccessLevel { Read, Write, Owner }`.

Vault checks:

- `check_vault_access(vault, user, level)`: `vault.user_id == user` passes everything. Otherwise a `vault_memberships` row is required; Read accepts any role, Write requires `can_write`, Owner requires role `owner`. So a membership row with role `owner` grants full owner powers (delete vault, manage members, stats) to a non-owner user, while the owner-protection guards in memberships only protect `vault.user_id`. Every denial is `KeyNotFound("vault not exists")`.
- `require_vault_by_id` (Read), `require_vault_by_id_write` (Write), `require_vault_owner` (Owner), plus boolean `has_vault_read_access` / `has_vault_write_access`.
- `require_vault_by_name(name, user)`: trims; parses an optional `"Name (owner)"` suffix via `parse_vault_name_owner`; matches `LOWER(name)` across all vaults; keeps those where the user is owner or vault member. Zero matches: try the `(base, owner)` hint, else `KeyNotFound`. More than one: prefer the user's own, else the hint, else `InvalidAmount("ambiguous vault name")`. One: return it.
- `require_vault_header_by_id` / `require_vault_header_by_name`: same shape but access is owner, vault member, or flow member of any flow in that vault (`has_flow_membership_in_vault`, a join on `flow_memberships` to `cash_flows.vault_id`).
- `require_user_exists(username)`: `KeyNotFound("user not exists")`.

Flow checks:

- `require_flow_read(vault_id, flow_id, user)`: if the flow is direct in the vault, pass on vault read access or on any `flow_memberships` row. If not direct, a `flow_references` row `(vault_id, target_flow_id)` must exist, and then a `flow_memberships` row is required; vault access on the recipient vault is not enough. Denial is `KeyNotFound("cash_flow not exists")`.
- `require_flow_write`: `require_flow_read`, then pass if the user has write access on the vault passed in, else the flow membership role must `can_write`. Because the vault check comes first, a user with any flow membership role (even `viewer`) who owns or edits the recipient vault gets write on a referenced flow: rename, archive, mode changes, and via `transfer_flow`/`expense` moving money out of it. Flow membership roles only bite for direct flows accessed by users without vault access.
- `flow_membership_role(flow_id, user)`, `has_flow_membership_in_vault(vault, user)`.
- `require_flow_in_vault` / `require_wallet_in_vault` (macro-generated): direct-in-vault existence only, no user check.
- `unallocated_flow_id(vault)`: by `system_kind = 'unallocated'`; missing gives `InvalidFlow("missing Unallocated flow")`.
- `resolve_flow_id(vault, Option<Uuid>)`: `None` gives Unallocated; `Some(id)` passes if direct in the vault or referenced into the vault; no user check at all (callers rely on their vault write check).
- `resolve_wallet_id(vault, Option<Uuid>)`: `Some` checks direct existence; `None` picks the single non-archived wallet as in section 2.
- `resolve_flow_vault(user_vault, flow_id)` (in `write/common.rs`): direct gives that vault; referenced gives the flow's real `vault_id`; else `KeyNotFound("flow not accessible in this vault")`. Used only when applying balance changes so the real flow row is updated.

Per-operation matrix as implemented: reads of vault, wallets, categories, aliases, wallet and vault transaction lists need vault Read. Flow reads and flow transaction lists need `require_flow_read`. Creating transactions, updating, voiding, wallet CRUD, flow create/delete, category writes, `preview_category_merge`, all recurring ops, `recompute_balances`, `remove_flow_reference` need vault Write. Flow rename/archive/allow_negative/mode need `require_flow_write`. `transfer_flow` needs vault Write or flow write on both flows. Memberships, `share_flow_with_user`, `delete_vault`, `vault_statistics` need Owner. `transaction_with_legs` needs owner or any membership row and returns `Forbidden` otherwise.

## 9. Balances

- Denormalized columns: `wallets.balance`, `cash_flows.balance`, `cash_flows.income_balance`. Every create, update and void goes through `preview_apply_leg_updates` and then `persist_targets`, so the columns are rewritten from the simulated result inside the same DB transaction as the ledger rows.
- Flow domain rules live in `CashFlow::apply_leg_change(old, new)`: `new_balance = balance - old + new`; reject `new_balance < 0` with `InsufficientFunds(flow_name)` unless the flow is Unallocated (by `system_kind` or by name `unallocated` case-insensitively) or `allow_negative`; then `FlowMode::Unlimited` nothing, `NetCapped { cap_minor }` rejects `new_balance > cap` with `MaxBalanceReached(flow_name)`, `IncomeCapped { cap_minor, income_total_minor }` rejects `income_total - max(old,0) + max(new,0) > cap` and otherwise stores the new income total. `mode()` is derived: `max_balance = None` is Unlimited, `Some` with `income_balance = None` is NetCapped, both `Some` is IncomeCapped. Wallets have no rules and may go negative.
- `allow_negative` per flow is not in SPEC. It bypasses non-negativity but not caps.
- Load-time integrity: `validate_flow_mode_fields` runs on every flow load and on every flow balance change: cap must be `> 0`, `income_balance` requires `max_balance`, `income_balance >= 0`, `income_balance <= cap`. A degenerate row makes `vault_snapshot` fail hard with `InvalidFlow` (test `names_are_trimmed_and_unique_case_insensitive`). Currency equality with the vault is checked on every wallet, flow and leg load.
- `recompute_balances(vault_id, user_id)`: loads all wallets and flows of the vault including archived, zeroes balances (income_balance to `Some(0)` where it was `Some`), replays legs of non-voided transactions where `transactions.vault_id = vault`, ordered by `occurred_at ASC, legs.id ASC`, applying wallet deltas directly and flow changes through `apply_leg_change(0, amount)` with full invariant checking, then writes the results. It fails if a leg targets an unknown wallet or flow (hard-deleted flows, or cross-vault legs), and it can fail on history that violates current rules, for instance backdated transfers into a capped flow or a flow whose `allow_negative` was later turned off. For a shared flow it only replays the owner vault's transactions, so running it on the owner vault silently drops contributions made from recipient vaults, and running it on a recipient vault errors with `KeyNotFound("cash_flow not exists")`.
- The `sum(wallets) == sum(flows including Unallocated)` identity holds by construction, but nothing checks it.

## 10. Error enum

`EngineError` (`error.rs`), `thiserror`, manual `PartialEq` (the `Database` variant compares by `to_string()`):

- `MaxBalanceReached(String)`: a flow cap would be exceeded. Payload is the flow name but the Display string is the fixed `"Max balance reached!"` and drops it.
- `InsufficientFunds(String)`: a flow would go below zero. Payload is the flow name. Display `"Insufficient funds: {0}"`.
- `KeyNotFound(String)`: not found, or not authorized (blind 404). Display `"\"{0}\" key not found!"`.
- `ExistingKey(String)`: duplicate vault/wallet/flow/category/alias name. Display `"\"{0}\" already present!"`.
- `InvalidAmount(String)`: heavily overloaded. Amounts, blank idempotency key, leg shape errors, list filter validation, ambiguous vault name, `wallet_id` required, updating or re-voiding a voided transaction, wrong update fields, transfer from/to equal.
- `InvalidName(String)`: empty names, category reserved/similar/archived/system rules, merge conflicts.
- `InvalidId(String)`: unparsable uuid in vault or transaction conversion.
- `InvalidCursor(String)`: bad pagination cursor.
- `InvalidFlow(String)`: flow mode field violations, reserved or system flow operations, missing Unallocated, cannot share/archive/delete Unallocated, allow_negative toggles.
- `InvalidRole(String)`: membership role string not in the set.
- `CurrencyMismatch(String)`: unsupported currency code or stored currency not equal to vault currency.
- `InvalidRecurring(String)`: recurring validation, date parse failures, already executed.
- `Forbidden(String)`: owner protections in memberships, last flow owner, transaction detail access.
- `Database(DbErr)`: transparent SeaORM error.

SPEC section 8 names `ArchivedTarget`, `FlowCapExceeded`, `InsufficientFlowFunds`, `ForbiddenOperation`, `IdempotencyKeyConflict`, `Db`. None exist under those names; `ArchivedTarget` and `IdempotencyKeyConflict` have no equivalent behavior at all. The module doc comment in `error.rs` lists only two variants and is stale.

## 11. Persistence

SQLite through SeaORM/sqlx. All UUID columns are 16-byte BLOBs; raw SQL paths bind `uuid.as_bytes()`. Migrations live in `crates/migration`, and their FK/cascade declarations are the truth; several entity `Relation` annotations say `NoAction` where the migration says `Cascade`.

Tables and notable details:

- `users(username TEXT PK, password, telegram_id, pair_code)`. The engine only reads it for `require_user_exists`; `user_id` everywhere is the username.
- `vaults(id BLOB PK, name, user_id, currency TEXT)`. FK to users. No unique index on `(user_id, name)`; case-insensitive uniqueness is enforced in code only.
- `wallets(id, name, balance BIGINT, currency, archived BOOL, vault_id)`; unique index `(vault_id, name)` with default case-sensitive collation.
- `cash_flows(id, name, system_kind TEXT nullable, balance, max_balance nullable, income_balance nullable, currency, archived, vault_id, allow_negative BOOL default false)`; unique `(vault_id, name)`.
- `transactions(id, vault_id, kind TEXT, occurred_at TIMESTAMP, amount_minor, currency, category TEXT nullable, note, created_by, voided_at TIMESTAMP nullable, voided_by, refunded_transaction_id BLOB nullable, idempotency_key nullable, category_id BLOB)`. `category_id` was added by the categories migration as nullable at the DB level while the entity declares it non-null. Indexes: `(vault_id, occurred_at)`, unique `(vault_id, idempotency_key)`, `created_by`. FK to vaults cascade.
- `legs(id, transaction_id, target_kind TEXT, target_id BLOB, amount_minor, currency, attributed_user_id nullable)`. Indexes `transaction_id`, `(target_kind, target_id)`, `target_id`. FK to transactions cascade. No FK to wallets or flows.
- `vault_memberships(vault_id, user_id, role)` composite PK, FKs cascade, index `user_id`.
- `flow_memberships(flow_id, user_id, role)` composite PK, FKs cascade, index `user_id`.
- `categories(id, vault_id, name, name_norm, archived, is_system)`; unique `(vault_id, name_norm)`; FK to vaults cascade.
- `category_aliases(id, vault_id, category_id, alias, alias_norm)`; unique `(vault_id, alias_norm)`; FKs cascade.
- `recurring_templates(id, vault_id, kind TEXT, amount_minor, wallet_id, flow_id, category_id, note, created_by, frequency TEXT, day_of_period INT, start_date TEXT, end_date TEXT, enabled BOOL, last_executed_date TEXT, created_at TEXT, archived_at TEXT)`. No indexes, no FKs.
- `flow_references(id, vault_id, target_flow_id, display_name TEXT nullable, created_at TEXT)`; unique `(vault_id, target_flow_id)`, indexes on `vault_id` and `target_flow_id`; FKs to vaults and cash_flows cascade.

The categories migration backfills a system `Uncategorized` per existing vault and canonicalizes historical free-text `transactions.category` values into category rows. Timestamps on transactions are SeaORM `DateTimeUtc`; recurring and flow_references dates are text.

## 12. Behaviors covered by tests worth preserving

`tests/transactions.rs` (in-memory SQLite, users alice/bob/charlie inserted directly):

- `new_vault_creates_unallocated_and_default_wallet`: new vault has an Unallocated flow and a `Cash` wallet.
- `income_expense_void_reverts_balances`: income raises both wallet and flow; expense lowers both; void restores both.
- `refund_increases_balances`: refund adds to wallet and flow.
- `transfer_wallet_does_not_touch_flows`: wallet transfer moves wallet balances and leaves Unallocated unchanged.
- `income_capped_counts_transfers_in`: a flow transfer into an income-capped flow above its cap fails with `MaxBalanceReached(name)`.
- `update_transaction_updates_balances`: raising an expense amount adjusts wallet and flow.
- `update_income_can_retarget_wallet_and_flow_and_keeps_metadata_when_omitted`: retargeting moves balances; category and note untouched when omitted; note stored trimmed.
- `update_expense_retarget_flow_fails_if_insufficient_and_is_atomic`: retarget to an empty flow fails with `InsufficientFunds` and nothing changes.
- `update_transfer_wallet_can_change_endpoints_and_amount`: endpoints and amount change; whitespace note clears to `None`; wallets go negative freely.
- `update_transfer_flow_can_change_endpoints_and_amount`: flow transfer endpoints and amount change with correct net balances.
- `recompute_balances_restores_denormalized_state_and_ignores_voided`: corrupted balances are rebuilt from legs; voided transactions ignored; `income_balance` rebuilt; Unallocated goes negative from allocations.
- `expense_on_flow_without_balance_fails`: spending 1 minor unit from an empty normal flow fails with `InsufficientFunds`.
- `list_transactions_excludes_voided_and_transfers_by_default`: defaults hide voided and transfers; flags reveal them.
- `transactions_pagination_cursor_walks_pages_without_duplicates`: page size 2 over 5 rows yields each id once.
- `restart_engine_reads_same_state`: file DB survives engine restart.
- `idempotency_key_dedupes_create`: same key returns the same id and does not double-apply.
- `names_are_trimmed_and_unique_case_insensitive`: vault, wallet, flow names trimmed; duplicates differing only in case give `ExistingKey`; blank names give `InvalidName`; a degenerate `income_balance` without `max_balance` makes snapshot fail with a specific `InvalidFlow` message.
- `category_and_note_are_trimmed_and_empty_becomes_none`: blank category becomes `None` (Uncategorized); note trimmed.
- `category_normalizes_to_existing`: `"  spesa!!!  "` resolves to existing `Spesa`.
- `category_similar_requires_confirmation`: `spese` vs existing `spesa` fails with a "too similar" `InvalidName`.
- `list_categories_includes_uncategorized_and_new`: system Uncategorized listed; created category listed.
- `alias_resolves_to_category`: alias input resolves to the canonical display name.
- `rename_category_updates_transactions`: rename propagates to `transactions.category`, id unchanged.
- `archived_category_rejected_on_create`: using an archived category name fails with an "archived" `InvalidName`.
- `merge_category_moves_transactions_and_aliases`: merge repoints transactions and later use of the old name resolves to the target.
- `preview_merge_reports_conflicts`: preview flags `TargetArchived`.
- `list_transactions_can_filter_by_date_range_and_kinds`: `[from, to)` semantics; kinds allow-list.
- `list_transactions_rejects_invalid_filters`: `from == to` and empty kinds are rejected with exact messages.
- `vault_statistics_treats_refunds_as_expense_reduction`: expenses minus refunds; balance from wallets.
- `flow_membership_allows_reading_flow_without_vault_access`: a flow viewer can read the flow but not the vault snapshot.
- `flow_member_cannot_access_transaction_detail`: flow viewer gets `Forbidden("forbidden")` on detail.
- `flow_membership_editor_can_transfer_between_shared_flows_without_vault_membership`: flow editor moves allocation between two shared flows.
- `vault_owner_can_manage_vault_members`: upsert, role update, remove; viewer can read snapshot.
- `non_owner_cannot_manage_memberships`: a member gets `KeyNotFound("vault not exists")` on membership ops.
- `editor_cannot_delete_vault`: editor gets `KeyNotFound`.
- `vault_owner_can_manage_flow_members_and_unallocated_is_not_shareable`: flow member CRUD; Unallocated share fails with `InvalidFlow("cannot share Unallocated")`.
- `vault_owner_role_cannot_be_changed_or_removed`: exact `Forbidden` messages.
- `flow_last_owner_cannot_be_demoted`, `flow_last_owner_cannot_be_removed`: last flow owner protection.

`tests/flow_sharing.rs`:

- `test_share_flow_creates_reference`: shared flow appears in the recipient snapshot with `is_shared` and `is_reference` set.
- `test_cross_vault_transaction`: recipient's income from their own wallet into the shared flow raises the flow balance in the owner's vault and the wallet in the recipient's vault.
- `test_remove_flow_reference`: recipient unshares; flow stays in the owner vault.
- `test_shared_flow_archival`: owner archives; recipient snapshot hides it; `list_accessible_flows(include_archived = true)` still returns it with `archived = true`.
- `test_name_conflict_handling`: same-named flow in the recipient vault produces a display name containing the owner username.

## 13. Warts, tech debt, and things that look wrong

Correctness and security:

- **Flow membership role is bypassed for referenced flows.** `require_flow_write` checks recipient-vault write access before the flow role, so a `viewer` flow member who owns their vault can rename, archive, change caps on, and move money out of the shared flow. `income`/`expense` on a referenced flow only check recipient-vault write plus `resolve_flow_id`, which has no role check.
- **`recompute_balances` is wrong for shared flows.** Owner-vault recompute drops recipient contributions; recipient-vault recompute errors. Hard-deleted flows also break it via orphan legs.
- **Cross-vault transactions cannot be updated** because update validates the existing flow leg with the direct-only lookup.
- **Shared flow history is split by `transactions.vault_id`.** Each side only lists its own transactions for the flow.
- **Cursor tiebreak likely never matches.** The `id < cursor.transaction_id` clause compares a BLOB column to a TEXT value; in SQLite TEXT sorts before BLOB so the predicate is always false. Rows sharing an identical `occurred_at` across a page boundary would be skipped. Not covered by tests; unverified at runtime.
- **Idempotency lookup and index disagree.** Code dedupes on `(vault_id, created_by, key)`; the unique index is `(vault_id, key)`. A second user reusing a key in the same vault gets a raw `Database` error instead of either dedupe or a domain error. SPEC says the scope is `(vault_id, key)`.
- **A vault membership with role `owner` grants full owner power** to someone other than `vault.user_id`, and that user can then remove or demote themselves but not the real owner. Probably intended as scaffolding; the semantics are undefined.
- **No `ArchivedTarget`.** Archived wallets and flows accept legs, transfers, and recurring executions. Archiving only affects default wallet selection, snapshot filtering, and statistics.
- **Void can be blocked by invariants** (`InsufficientFunds` or `MaxBalanceReached`), leaving some transactions un-voidable without first moving money.
- **`delete_vault`** orphans `recurring_templates` and depends on SQLite FK enforcement for memberships and references.
- **`delete_cash_flow(archive = false)`** hard-deletes with orphan legs and no balance check. Renaming and mode changes on a referenced flow check uniqueness and compute income sums against the caller's vault instead of the owner vault.
- **`new_cash_flow`** silently ignores a negative opening balance even when `allow_negative` is true.
- **`new_wallet`** creates a real user category literally named `opening`.

Half-done or inconsistent:

- Recurring: no skip; execute ignores `start_date`/`end_date`; pending `period_date` can precede `start_date`; missed periods are never backfilled; `wallet_id`/`flow_id` unvalidated at create/update and cannot be cleared; `note` not trimmed and cannot be cleared; all reads require vault write; an archived category breaks execution.
- `refunded_transaction_id` and `attributed_user_id` exist in the schema and models but are never written. SPEC says `attributed_user_id` defaults to the creator.
- `list_accessible_flows` skips membership checks on referenced flows and does not set `is_reference` or `display_name`, unlike `vault_snapshot`. `vault_snapshot` includes archived wallets but not archived flows, and does one query per referenced flow for the owner.
- `transaction_with_legs` breaks the blind-404 convention with `Forbidden`. Membership checks there are hand-rolled instead of using `require_vault_by_id`.
- `create_category` and free-text resolution offer no way to force-create a name within the similarity threshold; the error message tells the user to "confirm" but no confirm path exists.
- `merge_category` can attach an alias to the system Uncategorized category, which `create_category_alias` forbids.
- `apply_flow_change` has a botched error string literal `"cash_EngineError::FLOW_NOT_FOUND"` from a refactor.
- `MaxBalanceReached` Display discards the flow name. `InvalidAmount` is used for ambiguous vault name, filter validation, and update field validation.
- `Vault::new` builds empty `HashMap`s that are never used; `Vault.id` is a `String` while every other id is `Uuid`. `VaultHeader` and `Vault` are not `Serialize`; `CashFlow` is, `Wallet` is not.
- `Engine::with_tx` is public and exposes SeaORM's `DatabaseTransaction`.
- `list_categories` sorts by DB byte order (case-sensitive); `list_recurring` has no ordering; legs are ordered by random UUID in detail and in recompute replay.
- DB unique indexes on names are case-sensitive while the engine enforces case-insensitive uniqueness; DB `category_id` is nullable while the model is not.
- `is_unallocated()` and several guards fall back to matching the name `unallocated` case-insensitively in addition to `system_kind`.
- `share_flow_with_user` picks the "primary" target vault as the first owned vault by name, which is arbitrary.
- `cash_flow_by_name` searches archived flows but not referenced ones, and ignores `display_name`.
- `error.rs` module docs and the "refreshes the in-memory vault state" comment in `recompute_balances` are stale.

SPEC divergences not listed above: kind names are `Expense`/`TransferWallet`/`TransferFlow` rather than SPEC's `Spend`/`InternalWalletTransfer`/`InternalFlowTransfer`; `WalletKind` is absent; `vault_statistics` is all-time rather than monthly and owner-only; opening balances are modeled as real transactions; a default `Cash` wallet and an owner membership row are created per vault; `allow_negative`, category aliases with merge, the Levenshtein guard, and recurring templates are all features beyond the SPEC.
