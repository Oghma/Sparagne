# Telegram bot inventory and git history (v1, 0.93.0)

Raw inventory produced during the v2 distillation (2026-09-05). Part A covers `crates/telegram_bot`; part B the git history of the whole repo.

# Report A: Telegram bot inventory (`crates/telegram_bot`, 34 files, ~6,400 lines incl. tests)

## A.0 Shape
- Thin client over the HTTP server. `lib.rs` doc: "The bot is a thin client: it talks only to the HTTP server API and never accesses the database directly." teloxide + reqwest.
- Two dispatcher branches: `Update::filter_message()` -> `handlers::handle_message`; `Update::filter_callback_query()` -> `handlers::handle_callback`.
- "Hub message" pattern: each screen is rendered into one Telegram message edited in place (`shared::edit_or_send`: edit `session.hub_message_id` if present, else send and remember it). Confirmations and errors go out as separate messages.
- State:
  - `PrefsStore`: JSON file (default `config/telegram_bot_state.json`), `users: HashMap<String telegram_user_id, UserPrefs>`, atomic write via tmp + rename. `UserPrefs { active_vault_name: "Main", default_wallet_id, default_flow_id, last_flow_id, include_voided, category_hints, templates }`.
  - `SessionStore`: in-memory `HashMap<ChatId, Session>` (lost on restart): `hub_message_id, display_name, pending: Option<PendingAction>, list: Option<ListSession>, last_detail_tx, wizard, current_screen`.
- Locale: `resolve_locale` from Telegram `language_code`: prefix `it` -> It, `en` -> En, anything else -> It (default). All strings live in `i18n/mod.rs` (IT + EN, ~150 keys).
- Currency: hard-coded `Currency::Eur` for parsing (`parse_quick_add(text, EngineCurrency::Eur)`); display currency from the vault snapshot (only `Eur` exists).
- Timezone: hard-coded `Europe/Rome` (`now_rome`, `rome_start_of_day`, `rome_end_of_day`).

## A.1 Everything the bot understands

### A.1.1 Slash commands (`routing::parse_command`)
```rust
let mut parts = trimmed.splitn(2, ' ');
let cmd = parts.next().unwrap_or("");
let arg = parts.next().map(|s| s.to_string());
match cmd {
    "/start" => Some(Command::Start { code: arg }),
    "/home" => Some(Command::Home),
    "/help" => Some(Command::Help),
    "/categories" => Some(Command::Categories),
    "/export" => Some(Command::Export),
    "/template" | "/templates" => Some(Command::Template),
    "/vault" => Some(Command::Vault { value: arg }),
    _ => None,
}
```
| Command | Behaviour |
|---|---|
| `/start <code>` | pairs the Telegram id with the app user via `POST /user/pair`; on success "✅ Pairing completato!", then first-time onboarding text if `default_wallet_id` is unset, then home |
| `/start` | welcome text with quick-add examples, then home |
| `/home` | clears wizard, renders home |
| `/help` | static help + "Per fare pairing: /start <codice>" |
| `/categories` | lists category names of the active vault |
| `/export` | CSV export sent as a document |
| `/template`, `/templates` | template list screen |
| `/vault [x]` | argument ignored (`let _ = value;`), always shows the vault picker |
| any other `/...` | silently ignored |

`/start@botname` forms are not handled (exact match only). The command menu is registered with Telegram in IT (default) and EN (`register_commands`).

### A.1.2 Quick-add free text (the core grammar)
Trigger (`routing::looks_like_quick_add`):
```rust
let trimmed = text.trim_start();
trimmed.starts_with('+')
    || trimmed.starts_with('-')
    || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
```
Any non-command message that does not match is ignored silently (unless a pending input is active).

Parser (`parsing::parse_quick_add`), documented rules:
```rust
/// Rules:
/// - `12.50 ...` and `-12.50 ...` => Expense
/// - `+12.50 ...` => Income
/// - optional `#tag` (max 1) => category (case-insensitive)
```
Code:
```rust
let trimmed = collapse_whitespace(input.trim());
if trimmed.is_empty() { return Err(ParseError::Empty); }
let kind = if trimmed.starts_with('+') { QuickKind::Income } else { QuickKind::Expense };
let mut parts = trimmed.splitn(2, ' ');
let amount_str = parts.next().ok_or(ParseError::InvalidAmount)?;
let tail = parts.next().unwrap_or("").trim();
let amount = Money::parse_major(amount_str, currency).map_err(|_| ParseError::InvalidAmount)?;
let amount_minor = i64::try_from(amount.minor().unsigned_abs()).map_err(|_| ParseError::InvalidAmount)?;
if amount_minor <= 0 { return Err(ParseError::InvalidAmount); }
let mut tag: Option<String> = None;
let mut note_tokens: Vec<&str> = Vec::new();
for token in tail.split_whitespace() {
    if let Some(raw) = token.strip_prefix('#') {
        if raw.is_empty() { note_tokens.push(token); continue; }
        if tag.is_some() { return Err(ParseError::TooManyTags); }
        tag = Some(raw.to_ascii_lowercase());
    } else {
        note_tokens.push(token);
    }
}
let note = collapse_whitespace(&note_tokens.join(" "));
let note = (!note.is_empty()).then_some(note);
```
Extraction summary:
- Sign/kind: leading `+` -> Income; anything else (including `-`) -> Expense. Absolute value always taken; the sign never reaches the server, only the kind does.
- Amount: first whitespace token, parsed by `engine::Money::parse_major(str, Eur)`. Zero rejected.
- Category: exactly one `#tag` token anywhere after the amount, `#` stripped, `to_ascii_lowercase()`. Two tags -> "Troppi tag: massimo 1.". A bare `#` stays in the note. Sent as `category: Some(name)`, `category_id: None` (name resolution is server-side).
- Note: all remaining tokens joined with single spaces; `None` if empty.
- Wallet: not in the grammar. `prefs.default_wallet_id`; if unset (or no longer in the snapshot) the draft is parked as `PendingAction::WalletForQuickAdd(draft)` and a wallet picker is shown; the chosen wallet is saved as default and the draft finalized.
- Flow (budget): not in the grammar. `prefs.last_flow_id` if still present in the snapshot, else the vault's `unallocated_flow_id` (and `last_flow_id`/`default_flow_id` are set to it).
- Date: not in the grammar; always `occurred_at = now_rome()`.
- Idempotency: `format!("tg:{}:{}", msg.chat.id.0, msg.id.0)`; HTTP 409 is rendered as "✅ Già salvato.".
- Smart category suggestion (only when no `#tag`): `suggest_category(note, prefs.category_hints)` does `note.to_lowercase().contains(keyword.to_lowercase())` over a `HashMap`, first hit wins (iteration order unspecified, so overlapping keywords are nondeterministic). Default hints:
```
caffè/caffe/coffee/cappuccino -> bar
pranzo/cena/lunch/dinner -> cibo
spesa/grocery -> supermercato
benzina/gas/fuel -> auto
treno/train/bus/metro/taxi/uber -> trasporti
farmacia/pharmacy/medico/doctor -> salute
```
  No UI edits hints; they live only in the JSON prefs file.
- Success feedback: "✅ Salvato: {amount}" + " • {category}" + " • {note}" with inline keyboard `[↩ Annulla] [✏️ Modifica]` -> `tx:void:{id}` (immediate void, no confirmation) and `tx:edit:{id}`.
- Errors: `Empty` -> ignored; `InvalidAmount` -> "Importo non valido (es: 10 o 10.50)."; `TooManyTags` -> "Troppi tag: massimo 1.".

### A.1.3 Wizard (guided entry)
Home buttons `➖ Spesa` / `➕ Entrata` (`home:expense`/`home:income`) open a wizard screen showing wallet + budget and "Inserisci: importo [#categoria] [nota]\nEs: 12.50 #cibo caffè". Buttons: `✏️ Inserisci` (`wiz:input`, sets `PendingAction::WizardDraft{kind}` and prompts), `👛 Wallet` (`wiz:wallet`), `🎯 Budget` (`wiz:flow`), `🏠 Home` (`wiz:cancel`).
The next text message is normalized (`wizard::normalize_wizard_input`) then goes through the same `parse_quick_add`:
```rust
QuickKind::Expense => {
    // Remove + prefix if present (treat as expense anyway)
    let cleaned = trimmed.strip_prefix('+').unwrap_or(trimmed);
    Ok(cleaned.to_string())
}
QuickKind::Income => {
    // Ensure + prefix for income
    if trimmed.starts_with('+') { Ok(trimmed.to_string()) } else { Ok(format!("+{trimmed}")) }
}
```
In the wizard the sign is forced by the chosen kind. After saving, the wizard is re-shown (repeat-entry loop). Differences vs plain quick-add: if no default wallet the draft is dropped (picker shown, text must be re-sent); same idempotency key scheme.

### A.1.4 Other text inputs (`PendingAction`, handled before commands)
| Pending | Input grammar |
|---|---|
| `PairCode` | any non-empty trimmed text is sent as the code |
| `EditAmount{tx_id}` | `Money::parse_major(text, Eur)`, `checked_abs()`, must be `> 0`; `PATCH /transactions/{id}` with only `amount_minor` |
| `EditNote{tx_id}` | trimmed text; empty -> `note: None` (removes the note) |
| `TemplateCreate` | `name | amount [#category] [note]`: `text.split_once('|')`, both halves trimmed and non-empty, right half through `parse_quick_add`; max 10 templates (`MAX_TEMPLATES`) |
| `WalletForQuickAdd(draft)` | not a text input; resolved by the `wallet:set:` callback |

### A.1.5 Callback data grammar (`routing::parse_callback_action`)
```
nav:home
home:expense | home:income | home:history | nav:list | home:list | home:stats | home:help | home:wallet | home:flow
wallet:set:<uuid> | flow:set:<uuid> | vault:set:<uuid>
list:next | list:prev | list:toggle_voided | prefs:toggle_voided
list:filters | list:filter:kind:all | list:filter:kind:expense | list:filter:kind:income | list:filter:clear
tx:detail:<1-based index in current page> | tx:detail_id:<uuid>
tx:void_confirm:<uuid> | tx:void:<uuid> | tx:repeat:<uuid> | tx:edit:<uuid> | tx:edit_amount:<uuid> | tx:edit_note:<uuid>
wiz:input | wiz:cancel | wiz:wallet | wiz:flow
tpl:list | tpl:create | tpl:use:<index> | tpl:delete:<index>
noop
```
Unknown data is ignored. Every callback is `answer_callback_query`'d first.

### A.1.6 Screens
- Home: "👋 Ciao {display_name}!\n\n🏦 {vault}\n👛 Wallet: {wallet}\n🎯 Budget: {flow}\n💰 Saldo: {balance}" (balance = default wallet balance, 0 if none). Rows: `[➖ Spesa][➕ Entrata]`, `[📜 Cronologia][📊 Stats]`, `[👛 <wallet>][🎯 <flow>]` (pickers), `[❓ Aiuto]`. If the snapshot call returns 401/403 the bot prompts "Inserisci il codice di pairing:" and sets `PendingAction::PairCode`.
- History (list): `POST /transactions` with `limit: 5`, `wallet_id: default wallet`, `include_transfers: false`, `include_voided: prefs.include_voided`, `kinds` from filter. Cursor pagination with a cursor stack (`cursors`, `current`, `next`) -> "Pagina N", `⬅️ Prec` / `Succ ➡️`. Text grouped by day ("📅 {day} {Month}") and lines "N. {amount} {category} {note}[ • annullata]". Numbered buttons `[1]..[5]` open detail. `🔍 Filtri` (kind only: Tutti / Solo spese / Solo entrate; `from`/`to` exist in `ListFilters` but no UI sets them), `Mostra annullate: Sì/No` (persisted), `🏠 Home`. Filter change resets pagination.
- Detail: "📋 Dettaglio\n\n📌 Tipo\n📅 Data (dd/mm/yyyy)\n💶 Importo\n🏷 Categoria\n📝 Nota". Buttons `[↩️ Annulla][🔄 Ripeti][✏️ Modifica]`, `[⬅️ Indietro]`.
- Void confirm: "⚠️ Conferma annullamento … {amount} • {note}" -> `[✅ Sì, annulla][❌ No, torna indietro]`. Void = `POST /transactions/{id}/void` (soft delete); afterwards back to list.
- Edit menu: only `💶 Importo` and `📝 Nota` (no category/date/wallet edit).
- Repeat: fetches detail, copies amount/category/note and wallet/flow from the legs, creates a new Expense/Income with `occurred_at = now`, no idempotency key. Refund/transfers -> "Operazione non permessa.".
- Stats: `POST /stats/get` (all-time `balance_minor`, `total_income_minor`, `total_expenses_minor`, but labelled with the current month name, so header and totals disagree) plus a client-side current-month expense-by-category breakdown built by paging `/transactions` (200/page, `kinds: [Expense]`, voided and transfers excluded), sorted alphabetically, "Senza categoria" bucket. Only button: Home.
- Categories: plain list of names.
- Templates: numbered list "N. name ±amount #cat note", per-template `[N] Usa` / `🗑️ Elimina name`, `➕ Nuovo template`, Home. Using a template creates the transaction like quick-add (default wallet + last flow, `idempotency_key: None`) with the same Undo/Edit keyboard.
- Export: "⏳ Generazione export in corso...", pages `/transactions` 100 at a time (`include_voided: true`, `include_transfers: true`, scoped to the default wallet if set, otherwise whole vault), CSV `data,tipo,importo,categoria,nota,annullata` (kind words localized: spesa/entrata/rimborso/trasf_wallet/trasf_budget; note escaping `"`->`""` and `,`->`;`; voided sì/no), filename `sparagne_export_%Y%m%d_%H%M%S.csv` (uses `chrono::Local`, not Rome), sent as a document.
- Vault picker: `/vault` only; list sorted by lowercase name; selecting stores `active_vault_name = "id:{uuid}"` and clears wallet/flow defaults. `vault_ref_from_value` accepts `"id:<uuid>"` or a name (empty -> "Main").
- Help: `/help` static; `❓ Aiuto` is contextual by `Session.current_screen` (Home/Wizard/List/Stats) + command footer.
- Onboarding after pairing when no default wallet: "🎉 Benvenuto su Sparagne…" + "💡 Concetti base: 👛 Wallet - Dove tieni i soldi … 🎯 Budget - Come organizzi le spese … 🏷 Categoria - Tag per classificare" + "🚀 Per iniziare: • Scrivi: 12.50 caffè • Oppure: +1000 stipendio • Usa i pulsanti qui sotto".
- Error mapping (`user_message_for_api_error`): network -> "Problemi di connessione. Riprova più tardi!"; 401 -> "Non autorizzato. Usa /start per fare il pairing."; 403 -> "Operazione non permessa."; 404 -> "Risorsa non trovata. Prova a reimpostare i default."; 409 -> "Richiesta duplicata (già salvata)."; 400 with body "user not found" -> "Codice di pairing non valido."; 422 -> server message verbatim; other -> "Errore server."; non-auth errors get "\n\n💡 Prova: /home per tornare alla home".

### A.1.7 The previous bot (deleted 2025-12-18, "chore: delete old telegram bot")
Italian slash commands via teloxide `BotCommands`: `/help`, `/entrata amount category note…`, `/uscita amount category note…`, `/sommario` (last entries), `/elimina` (dialogue-based delete), `/pair <code>`, `/unpair`, `/stats`, `/export`, `/start`. Grammar:
```rust
pub fn split_entry(input: String) -> Result<(String, String, String), ParseError> {
    let args: Vec<&str> = input.split(' ').collect();
    if args.len() < 3 {
        Err(ParseError::Custom("Failed to parse the entry".into()))
    } else {
        Ok((args[0].to_string(), args[1].to_string(), args[2..].join(" ")))
    }
}
```
Free text was interpreted as an expense with the same positional `amount category note…` grammar (category mandatory, no `#`). The file carried `// TODO: Avoid to hardcode italian strings and commands. Generalize`. The rewrite moved to: sign prefix for kind, optional `#tag`, single hub message UI, i18n, pairing prompt on 401.

## A.2 UX ideas worth carrying to a native app
1. One-line entry grammar `[+|-]amount [#category] [note…]`: expense by default, `+` for income, category optional and position-free. The wizard proves the same parser works with the sign forced by context.
2. Sticky defaults instead of questions: wallet = saved default, budget = last used, date = now. Missing default triggers a picker and the parked draft resumes after the pick.
3. Undo on the confirmation itself: "✅ Salvato: -12,50 € • bar • caffè [↩ Annulla][✏️ Modifica]". Void is a soft delete so undo is safe; on the fresh toast no confirmation, in the detail screen a confirmation.
4. Idempotency key derived from the input event (`tg:{chat}:{msg}`) with a friendly "Già salvato." on 409.
5. Keyword -> category suggestion when no tag is given (static IT/EN map; a natural hook for a learned mapping).
6. Templates (`name | 1.50 #bar caffè`, max 10) as one-tap repeats, plus Repeat on any past transaction (copies wallet/flow/category/note, new date).
7. "Show voided" toggle persisted per user; voided rows shown with a "• annullata" suffix rather than hidden.
8. History grouped by day with numbered rows matching numbered buttons; 5 per page with a cursor stack for back navigation.
9. Monthly stats with per-category breakdown and an "uncategorized" bucket.
10. CSV export with localized kind labels and a voided column.
11. Contextual help keyed on the current screen, and a first-run concepts explainer (wallet vs budget vs category).
12. Pairing on demand: any 401/403 flips the session into "type your pairing code" mode instead of a dead end.
13. Not in the bot (only TUI/engine): recurring materialization prompts, reminders, transfers, refunds, date override, category edit. Bot edit covers only amount and note.

## A.3 Authentication and account linking
- Bot -> server: every request carries HTTP Basic auth with the service user from `config/config.toml` `[telegram] username/password` (README: "Telegram bot requires a dedicated service user"). Set as a default `Authorization: Basic base64(username:password)` header on the reqwest client in `Bot::new`.
- Acting user: every call except `/user/pair` adds `telegram-user-id: <numeric Telegram user id>` (`ApiClient::post_json`). Server middleware `server.rs::auth`:
```rust
let user: Option<user::Model> = user::Entity::find()
    .filter(user::Column::Username.contains(auth_header.username()))
    .filter(user::Column::Password.contains(auth_header.password()))
    .one(&state.db) ...
if let Some(header) = telegram_header {
    let user_entry = user::Entity::find()
        .filter(user::Column::TelegramId.eq(header.0))
        .one(&state.db) ...
    user = if let Some(user) = user_entry { user } else { return Err(StatusCode::UNAUTHORIZED); };
}
request.extensions_mut().insert(user);
```
  The service user authenticates, then the request is re-attributed to the app user whose `users.telegram_id` equals the header. Weaknesses to avoid in the rewrite: passwords stored and compared in plaintext; the Basic check uses SQL `contains` (substring match); any user row works as a service user; no check that the service user is actually the bot.
- Linking (pairing): an admin runs `admin_cli user create --username X --pair-code CODE` (password prompted; `pair_code` stored on `users`). The user sends `/start CODE` (or types the code when prompted after a 401/403). The bot POSTs `/user/pair {code, telegram_id}` (Basic auth only). Server `user.rs::pair`: finds the row by `pair_code`, sets `telegram_id`, clears `pair_code`, returns 201; unknown code -> 400 "user not found" -> "Codice di pairing non valido.". `DELETE /user/pair` (unpair) exists server-side but the current bot has no unpair command (the old bot had `/unpair`).
- Allow-list: optional `allowed_users: Vec<UserId>` from config; if non-empty, updates from other Telegram ids are dropped silently (`is_allowed`).
- Per-user preferences keyed by Telegram user id in the JSON file; sessions keyed by chat id in memory.

## A.4 Tests
No `tests/` directory; 35 `#[test]`/`#[tokio::test]` functions inline in 7 files:
| File | Tests |
|---|---|
| `parsing.rs` | 6: default expense, `-` expense, `+` income, tag sets category and is removed from note, tag anywhere, rejects >1 tag |
| `routing.rs` | 12: `/start abc`, `/home`, `/help`, `/categories`, unknown -> None, `/vault Main`, `wallet:set:`, `vault:set:`, `tx:detail:3`, `home:history`, `looks_like_quick_add` positive/negative |
| `text.rs` | 5: display name from username / first name / fallback "Sparagne", help contains `/home`, help contains `12.50` and `+1000` |
| `i18n/mod.rs` | 4: locale resolution (missing, unknown fr/de, en/en-US/en-GB, it/it-IT) |
| `handlers/callbacks.rs` | 2: `apply_list_next` pushes cursor, `apply_list_prev` pops |
| `use_cases/shared.rs` | 2 (`MockApi`): `resolve_vault_id` ok / missing id |
| `use_cases/flow_tests.rs` | 4 (`MockApi` + `MockBot`): home sends summary then edits the hub message; English locale shows "Budget:"; wizard renders "Nuova Spesa" with Wallet/Budget; list renders "Ultime transazioni:" with category and note |
Test doubles: `api/mock.rs` `MockApi` (one `Mutex<Option<Result<_, ApiError>>>` slot per endpoint, consumed by `take()`, unconfigured -> 500) and `bot_client/mock.rs` `MockBot` (records sent/edited messages and a has_kb flag). The `BotClient` trait exists so use-cases are testable; handlers still take a concrete `teloxide::Bot`. Not covered: quick-add end to end (wallet/flow fallback, 409 path), `suggest_category`, templates, CSV formatting, stats breakdown, filters, void/repeat/edit callbacks, pairing.

# Report B: Git history and evolution

## B.5 Counts
- Commits on `master`: 703. Authors: Matteo Lisotto 683, dependabot[bot] 24 (`git shortlog -sne --all`; sum exceeds 703 because `--all` includes two remote branches).
- Tags: `v0.90.0` (2026-01-13), `v0.91.0` (2026-01-20), `v0.92.0` (2026-02-10), `v0.93.0` (2026-02-13). No CHANGELOG file; `docs/` holds only `DEVELOPMENT.md`; the engine has `crates/engine/SPEC.md` and `ARCH.md`.
- Commits per month: 2022-11 6, 2022-12 32, 2023-01 11, 2023-02 9, 2023-03 16, 2023-04 2, 2023-05 29, 2023-06 12, 2023-07 1, 2023-08 9, 2023-09 11, 2023-10 2, 2023-11 6, 2024-01 66, 2024-02 38, 2024-03 3, 2024-05 14, 2024-06 through 2025-11 zero, 2025-12 198, 2026-01 114, 2026-02 124. About 62% of all commits are from the last three months.
- Message style: `[scope] Message` until 2023-11; conventional commits from 2024-01-10 (`feat:` 209, `chore:` 120, `refactor:` 110, `fix:` 39, `test:` 13). 171 commits have bodies; detailed multi-paragraph bodies start 2026-02-04.

## B.1 Timeline
Author dates; the root "First commit" is dated 2022-12-09 but earlier-dated commits follow it (rebased history).

| Period | Milestone |
|---|---|
| 2022-11-01 to 2022-12-10 | Engine prototype: `Entry`, `CashFlow` trait with `Unbounded`/`Bounded`/`HardBounded`, `Engine`, `archived` flag, rusqlite; a tui dependency already added 2022-11-21. |
| 2023-01-22 | Three cash-flow types merged into one struct. |
| 2023-03-06 to 03-20 | rusqlite replaced by sea-orm + migrations. |
| 2023-05-01 | First axum server; repo split into `engine` and `server` packages; README. |
| 2023-05-15 to 05-28 | Telegram workspace; "first minimal and buggy bot version"; `Engine` renamed `Vault`; `Wallet` struct; vaults schema. 2023-06-01 bot made opt-in. |
| 2023-06 to 2023-08 | Rework churn (2023-06-16 body: "Too many days have passed from when I started the refactor and I changed my mind too many times… Mayne the change of id in `Uuid` is bad and will be reverted"). `Engine`/`EngineBuilder` re-added 2023-06-25. |
| 2023-09-04 to 09-13 | `Users` table, `auth` middleware, `telegram-user-id` TypedHeader, `Vaults -> Users` FK, app settings. |
| 2023-11-01 | `ServerError`, vault API. |
| 2024-01-10 to 01-20 | Pairing (`pair_code`, `/user/pair`, `/pair`, `/unpair`), Basic auth in the bot, `/entrata` `/uscita`, `/sommario`; conventional commits, rustfmt, dependabot (01-17); "big bump" of year-old deps (01-21); ids switched from `Uuid` to `String` (01-17). |
| 2024-02-01 to 02-25 | Free-text expense parsing (02-01); config and DB moved to `config/`, Dockerfile (02-07/09); `/elimina` + delete endpoint (02-22); rename to Sparagne (02-24, previously hodlTracker); statistics endpoint + `/stats` (02-24/25). |
| 2024-03-26 | `/export` CSV. |
| 2024-05-03 to 05-27 | `/start`, timestamp on `Entry`, composite PKs on `Wallet`/`CashFlow`. Then dormant for about 18.5 months. |
| 2025-12-12 | Revival: `crates/` layout, `api_types` crate (shared DTOs), `MoneyCents` + `Currency`, Rust 2024 edition, stable toolchain. |
| 2025-12-14 to 12-15 | UUID ids again; `DateTime` replaces `Duration`; FlowMode rules (caps + non-negativity); Unallocated system flow; `Leg` + `Transaction` introduced. |
| 2025-12-16 | `Entry` replaced by `Transaction` everywhere (engine, server, bot); engine made stateless, DB source of truth ("This is a big change. I'm not going to split in different commits"); atomic writes; `recompute_balances`; GitHub Actions. |
| 2025-12-17 | Refund; idempotency_key; pagination; membership roles + users endpoints (sharing groundwork); PATCH `/transactions/:id`; list filters from/to/kinds; `Forbidden`; all endpoints switched to POST. |
| 2025-12-18 | Bootstrap CLI; old bot deleted, new bot written (api/ui/parsing/state/handlers/wizard). |
| 2025-12-19 | Flows/wallets endpoints; first TUI (ratatui): shell, transaction view with void/edit/repeat, quick-action, stats/vault/wallet/flows views, command palette, filtering. |
| 2025-12-28 to 12-30 | TUI forms/login; engine split into `ops` modules, `with_tx`, transaction builder; migrations merged into one init file; sea-orm 1.0; native UUID blobs (breaking); TUI ctrl+f search and edit. |
| 2025-12-31 | Categories (engine, server, bot, TUI) incl. aliases, conflicts, merge endpoint; TUI charts and recents; "sharing update". |
| 2026-01-01 to 01-05 | Version 0.90.0; sharing for vaults and flows in TUI and bot; delete vault; same-name vault fix. |
| 2026-01-12 to 01-13 | Sharing PR #63 merged; release/docker CI; tag v0.90.0; `Cross.toml` added and reverted same day; "remove `server` from the release. There's no server". |
| 2026-01-14 to 01-20 | Bot refactor: routing/use_cases/text modules, i18n (IT then EN), mocks, admin commands deleted (PR #67); export CSV, filters, templates in the bot (01-17); `/vault` (01-20); tag v0.91.0. |
| 2026-01-22 to 01-27 | TUI forms/validation, tabs (Analytics, Accounts), flow threshold alerts, dialogs, undo timeout, quick-add improvements, i18n. |
| 2026-02-02 to 02-09 | Large TUI refactor in numbered phases (PR #73), stats fixes. |
| 2026-02-09 to 02-10 | `allow_negative` flows (PR #77); recurring templates with pending detection and user-approved execution (PR #78); engine consolidation (PR #79); tag v0.92.0. |
| 2026-02-12 to 02-13 | Shared flows + `max_balance` in snapshot; flow_references (cross-vault sharing), share/unshare endpoints, `owner_user_id`; `SPEC.md`/`ARCH.md` first committed 02-12 (SPEC header: "Versione SPEC: v1 (approved baseline), Ultimo aggiornamento 2025-12-29"); tag v0.93.0 (last commit, 2026-02-13). |

Engine v1 spec: `crates/engine/SPEC.md` first appears in git on 2026-02-12, but its header dates the v1 baseline to 2025-12-29, matching the engine `ops` split of that day.

## B.2 Added then removed or reverted (lessons)
- Cash-flow type hierarchy (`Unbounded`/`Bounded`/`HardBounded` + trait, Dec 2022) -> collapsed into one struct (2023-01-22) -> later re-expressed as `FlowMode` rules with caps (2025-12-15) and `allow_negative` (2026-02-09).
- rusqlite + hand-written SQL wrapper (2023-01) -> removed for sea-orm (2023-03-20).
- `VaultBuilder` and the DB handle inside `Vault` removed 2023-06-01 ("The upcoming `Engine` will handle database connection").
- Id type flip-flop: `Entry.id` String (2023-02-04) -> Uuid (2023-06-16, author doubted it) -> String (2024-01-17) -> Uuid (2025-12-14) -> UUID as blobs (2025-12-30, breaking).
- `Entry` model fully deleted for `Transaction` + `Leg` (2025-12-16 "completely delete entry from tg_bot and server").
- In-memory engine state -> "make engine stateless (DB source of truth)" (2025-12-16); SPEC notes removed legacy in-memory APIs (`Vault::new_flow`, `Vault::delete_flow`, `Vault::iter_*`).
- The whole first Telegram bot (2023-05 to 2024-05: positional `/entrata amount category note`, dialogue delete, `/pair` `/unpair`) deleted 2025-12-18 and rewritten.
- Bot `get_check`/`delete_check` macros removed 2025-12-17; admin commands in the bot deleted 2026-01-15; unused routes removed 2026-01-15.
- `Cross.toml` cross-compilation added and reverted the same day (2026-01-13, the only `revert:` commit).
- Server dropped from release artifacts (2026-01-13 "There's no server").
- Dead-code purges during the TUI refactor: `TextInputField` trait (added and removed 2026-02-05), theme aliases `dim`/`error`, `load_pending_count`, `recurring_get`, `RECURRING_NOT_FOUND`, 8 unused `TextKey` variants (2026-02-13), chart helpers, `DateValidator`, `FormField` trait.
- Temporarily disabled endpoints: `cash_flow`/`entry` endpoints disabled 2024-01-10, restored 2024-01-12.

## B.3 Fix clusters (39 `fix` commits)
By scope: `fix(server)` 9, `fix(engine)` 7, `fix(telegram)` + `fix(telegram_bot)` 8, `fix(tui)` 4, unscoped 10, `fix(tests)` 1.
- Server/bot wiring during the 2024-01 pairing sprint (12 fixes in ten days): pair endpoint, unpair, `Vault` fields `Option`/public, `vault_new`, restoring the entries endpoint, passing `user_id`, `save` vs `insert`, response after adding an entry, "expenses have a negative amount". Lesson: sign convention and DTO shape churn between bot and server.
- Money/sign and domain invariants in the engine: "expenses have a negative amount" (2024-01-20), composite primary keys (2024-05-27), "fix: refunds" (2025-12-17), "enforce domain validations (names, FlowMode invariants, leg invariants)" (2025-12-17).
- Sharing / multi-vault resolution: same-name own vs shared vault (2026-01-05), "vault loading issues" (2026-01-12), `resolve_flow_id` for flow references, archived flows leaking into snapshots, non-deterministic target vault ordering (2026-02-12/13).
- Build/CI/tooling: dependency bumps breaking crates (2024-01-21), `uuid` re-added, dockerfile/image (2026-01-12 x2), clippy `too_many_arguments`, `expect_used` lint, nightly rustfmt.
- Bot copy/help text: /help, /start, /pair messages (2024-05).
- TUI rendering/input: 'q' swallowed by the global keymap in text fields, "7 statistics tab issues" (month vs all-time data, raw minor units shown, emoji width alignment), list layout/colors, one-line entries.
- Concurrency: "deadlock causing tests running infinitely" (2026-01-15, bot mocks with tokio Mutex).

## B.4 Design decisions recorded in commit bodies (verbatim excerpts)
- 2023-01-22: "A unique structure is simpler to handle and to serialize/deserialize in a db."
- 2023-03-06: "`rusqlite` is not thread safa and it cannot be used with axum in a web server hence replace with `sea-orm`."
- 2023-06-01: "The bot is now opt in. In future can also be activated/deactivated with cli."
- 2023-09-08: "Instead of creating an instance of `DatabaseConnection` inside the engine and the server, create the connection in `main` and pass it."
- 2024-02-07: "It is not ideal to store the database inside the config folder but in a container, `config` is a volume."
- 2025-12-12: "Move HTTP request/response types from server into new `api_types` crate and update telegram_bot to depend on it instead of server."
- 2025-12-16: "refactor(engine,server): make engine stateless (DB source of truth)".
- 2026-01-14: "Routing stays in crates/telegram_bot/src/routing.rs; handlers are slimmer and focused on orchestration."
- 2026-02-05: "Move 'q' quit shortcut from the global keymap (which runs before form input) to handle_non_login_key… This lets users type 'q' in any text field."
- 2026-02-09 (allow_negative): "Relax non-negativity guard … when the flag is set (or when the flow is Unallocated)"; "rejects disabling when balance is negative, rejects system flows"; DTO defaults "preserve backward compatibility for existing API clients."
- 2026-02-09 (recurring): "pending detection via `list_pending_recurring`, and user-approved execution via `execute_recurring` with idempotency key and atomic transaction+last_executed_date update." Materialization is explicit and user-approved, never automatic.
- 2026-02-09 (engine access): "Add AccessLevel enum (Read, Write, Owner) … Single source of truth for authorization decision tree."
- 2026-02-12 (flow references): "enabling flows to appear in multiple vaults without duplicating data"; "Handles name conflicts with display_name override ("{name} ({owner})")"; "Idempotent: skips creation if reference already exists"; "For referenced flows: flow_membership required (no vault access alone)"; unshare "Does NOT remove flow_membership (can be re-shared later)".
- 2026-02-12: "vault_snapshot() now filters archived flows by default (correct behavior per design)."
- 2026-02-13: "selects the target user's vault in alphabetical order by name, making the behavior predictable"; "Previously used .one() without .order_by(), which could return any vault in non-deterministic order."
- 2026-02-13 (UX): cap shown as "45.50 / 100.00 EUR" instead of "[limitato]" so "it is immediately clear how much budget is used vs available"; "[condiviso da matteo]" instead of a generic badge.
- 2026-02-13: tests use `Result<(), Box<dyn Error>>` with `?` "instead of .expect() calls, following workspace lint policy."
