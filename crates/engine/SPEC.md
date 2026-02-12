# Engine SPEC (Template)

Questo documento è la specifica di dominio dell’engine di budgeting.  
È **source of truth** per implementazione e test.  
Un agent AI che lavora sull’engine deve seguire questa SPEC senza assumere comportamenti non descritti.

---

## 0. Stato

- **Versione SPEC:** v1 (approved baseline)
- **Ultimo aggiornamento:** 2025‑12‑29
- **Owner:** @oghma
- **Note refactor:** rimosse API legacy in‑memory (`Vault::new_flow`, `Vault::delete_flow`, `Vault::iter_*`) e costruttori duplicati (`Wallet::with_id`, `CashFlow::with_id`).

---

## 1. Glossario

- **Vault**: rappresenta un utente; contiene wallet e flow.
- **Wallet**: luogo fisico dove risiedono i soldi (contante, conto bancario, carta).
- **Flow**: bucket/obiettivo logico sopra ai wallet (es. vacanze, emergenze). È agnostico al wallet.
- **Unallocated**: flow speciale del vault, che rappresenta il denaro non assegnato a flow (o spese non assegnate).
- **Flow reference**: puntatore virtuale che permette a un flow di apparire in più vault; i dati del flow vivono in un solo vault (owner vault), mentre il riferimento lo rende visibile in altri vault.
- **Transaction**: una singola operazione utente; è composta da una o più `Leg`.
- **Leg**: effetto signed della transaction su un target (`Wallet` o `Flow`).
- **Category**: etichetta user‑defined per le transaction, normalizzata per vault.
- **Soft delete / Void**: annullamento logico di una transaction (non più conteggiata in saldi/statistiche) senza cancellare i record.
- **Idempotency key**: chiave opzionale per rendere "create" ripetibile senza duplicare (utile con bot/HTTP retry).
- **Statistica**: report aggregato (mensile o per range) che deve escludere transfer interni e void.

---

## 2. Entità e campi

### 2.1 Vault

Campi minimi:
- `id: Uuid`
- `user_id: Uuid`
- `currency: Currency` (default `EUR`)

Invarianti:
- Esiste **sempre** un flow `Unallocated` per vault.
- Le risposte `VaultHeader` includono `owner` (user_id del vault).
- `name` è unico per owner (case-insensitive).
- Lookup per nome: se più vault accessibili condividono lo stesso nome, viene preferito quello di proprietà dell’utente; per riferire un vault condiviso usare `Nome (owner)`.
- La lista vault per utente include quelli di proprietà, condivisi e con flow condivisi.

### 2.2 Wallet

Campi minimi:
- `id: Uuid`
- `vault_id: Uuid`
- `name: String`
- `balance_cents: MoneyCents`
- `currency: Currency`
- `archived: bool`
- `kind: WalletKind` (per ora metadata: `Cash|Bank|CreditCard`)

Invarianti:
- Wallet **può andare negativo**.
- `name` è unico per vault (`unique(vault_id, name)`).

### 2.3 Flow

Campi minimi:
- `id: Uuid`
- `vault_id: Uuid`
- `name: String`
- `balance_cents: MoneyCents`
- `currency: Currency`
- `mode: FlowMode`
- `cap_value_cents: Option<MoneyCents>`
- `income_total_cents: Option<MoneyCents>` (solo `IncomeCapped`)
- `archived: bool`

Invarianti:
- Ogni flow diverso da `Unallocated` deve avere `balance_cents >= 0`.
- `Unallocated` può essere negativo.
- `Unallocated` è identificato da un nome interno riservato `unallocated` (non rinominabile); i client possono mostrarlo come “Non in flow” (localizzabile).
- `name` è unico per vault.
- Qualsiasi operazione che incrementa il flow (`flow:+x`) deve rispettare il cap del flow.

### 2.4 Category

Campi minimi:
- `id: Uuid`
- `vault_id: Uuid`
- `name: String` (display)
- `name_norm: String` (chiave normalizzata)
- `archived: bool`
- `is_system: bool`

Invarianti:
- `name_norm` è unico per vault.
- Ogni vault ha una categoria di sistema `Uncategorized` (non rimovibile).
- Le alias (`category_aliases`) sono univoche per vault (`alias_norm`).
- Categorie archiviate non possono essere selezionate per nuove transaction.
- Normalizzazione `name_norm`:
  - trim + collapse spazi
  - lowercase
  - rimozione punteggiatura/simboli (separatori)
  - Unicode NFKD + rimozione diacritici

### 2.5 Transaction

Campi minimi:
- `id: Uuid`
- `vault_id: Uuid`
- `kind: TransactionKind`
- `date: DateTime<Utc>`
- `category_id: Uuid` (FK -> categories.id, non null)
- `category: Option<String>` (display canonico, nullable)
- `note: Option<String>`
- `created_by_user_id: Uuid`
- `idempotency_key: Option<String>` (unica per `vault_id` quando presente)
- `voided_at: Option<DateTime<Utc>>`
- `voided_by_user_id: Option<Uuid>`

### 2.6 Leg

Campi minimi:
- `id: Uuid`
- `transaction_id: Uuid`
- `target_type: TargetType` (`Wallet|Flow`)
- `target_id: Uuid`
- `amount_cents: MoneyCents` (signed)
- `currency: Currency`
- `attributed_user_id: Uuid` (per flow condivisi; default = creator)

Invarianti:
- Tutte le legs di una transaction hanno la stessa currency del vault.

---

## 3. Enum e significato

### 3.1 Currency

Per ora:
- `EUR`

TODO: estendere quando servirà multi‑valuta.

### 3.2 WalletKind

Metadata UI/feature future:
- `Cash`
- `Bank`
- `CreditCard`

### 3.3 FlowMode

- `Unlimited`
  - nessun cap; non può scendere sotto 0 (se non `Unallocated`).
- `IncomeCapped(max_income_cents)`
  - somma entrate cumulative nel flow ≤ cap.
- `NetCapped(max_net_cents)`
  - saldo netto nel flow ≤ cap.

### 3.4 TransactionKind

- `Income`
  - movimento esterno positivo; **conta nelle statistiche**.
- `Spend`
  - movimento esterno negativo; **conta nelle statistiche**.
- `Refund`
  - rimborso/storno; **non gonfia le entrate** nei report (riduce spese).
- `InternalWalletTransfer`
  - transfer fisico wallet↔wallet; **non conta nelle statistiche**.
- `InternalFlowTransfer`
  - riallocazione virtuale flow↔flow (incluso `Unallocated`); **non conta nelle statistiche**.

TODO: se servono sottotipi futuri (es. interessi carta), aggiungere qui.

---

## 4. Invarianti di dominio (Locked)

1. **Entry unica**: ogni operazione utente è una `Transaction` con una o più `Leg`.
2. **Flow non negativi**: tutti i flow eccetto `Unallocated` restano `>= 0`.
3. **Wallet negativi OK**.
4. **Spend/Income/Refund su flow**:
   - una transaction di questo tipo deve avere:
     - 1 leg su wallet
     - 1 leg su flow (specifico o `Unallocated`)
   - i due importi hanno stesso valore assoluto e stesso segno.
5. **InternalWalletTransfer**:
   - esattamente 2 legs su wallet con segni opposti e stesso valore assoluto.
6. **InternalFlowTransfer**:
   - esattamente 2 legs su flow con segni opposti e stesso valore assoluto.
   - il flow sorgente non può scendere sotto 0 (se non `Unallocated`).
7. **Cap flow**:
   - qualsiasi `flow:+x` (anche riallocazioni interne) rispetta cap secondo `FlowMode`.
8. **Spesa oltre saldo flow**:
   - default strict: la transaction fallisce con errore di saldo insufficiente.
   - (opzione futura) split esplicito su `Unallocated`.
9. **Soft delete**:
   - una transaction con `voided_at != None` è considerata annullata.
   - le sue legs sono escluse da saldi e statistiche.
   - le Read API devono **escludere** le voidate per default e offrire un’opzione esplicita per includerle.
10. **Idempotency**:
   - create con stessa `(vault_id, idempotency_key)` non duplica dati e ritorna l’id esistente.
11. **Category**:
   - ogni transaction ha un `category_id` valido per il vault.
   - se input categoria è vuoto, usare `Uncategorized`.

---

## 5. Regole di calcolo

### 5.1 Saldi

- `wallet.balance_cents = sum(legs.amount_cents dove target=wallet)`
- `flow.balance_cents = sum(legs.amount_cents dove target=flow)`
- Le sums considerano solo legs di transaction **non voidate**.
- Bilanci sono denormalizzati nel DB ma sempre ricalcolabili (`recompute_balances`).

### 5.2 Statistiche mensili

Nel periodo `P`:
- `total_income = sum(legs.amount_cents > 0) solo per kind=Income`
- `total_expenses = sum(|legs.amount_cents| dove < 0) solo per kind=Spend`
- `Refund` riduce le spese nel report (implementazione: sottrazione dal totale spese o kind dedicato in query).
- I kind `Internal*Transfer` sono esclusi.
- Le statistiche considerano solo transaction **non voidate**.

TODO: formalizzare query SQL di riferimento.

---

## 6. Casi d’uso (Esempi)

Nota: negli esempi sotto, `amount_cents` è in centesimi (es. 100€ = `+10000`).

### 6.1 Income su flow

Operazione: “entrata 100€ su `W_cash` e `F_vacanze`”.

- `TransactionKind = Income`
- Legs:
  - `Wallet(W_cash): +10000`
  - `Flow(F_vacanze): +10000`
- Effetto:
  - stats: `total_income += 10000`
  - flow non va sotto 0 (OK), cap rispettato su `flow:+x`

### 6.2 Spend su flow (strict)

Operazione: “spesa 30€ su `W_cash` e `F_vacanze`”.

- `TransactionKind = Spend`
- Legs:
  - `Wallet(W_cash): -3000`
  - `Flow(F_vacanze): -3000`
- Precondizione:
  - `F_vacanze.balance_cents >= 3000` (altrimenti `InsufficientFlowFunds`)
- Effetto:
  - stats: `total_expenses += 3000`

### 6.3 Spend non assegnata (Unallocated)

Operazione: “spesa 30€ su `W_cash` senza scegliere flow”.

- `TransactionKind = Spend`
- Legs:
  - `Wallet(W_cash): -3000`
  - `Flow(Unallocated): -3000`
- Nota:
  - `Unallocated` può diventare negativo.

### 6.4 Refund / storno

Operazione: “rimborso 10€ relativo a una spesa (stesso flow)”.

- `TransactionKind = Refund`
- Legs:
  - `Wallet(W_cash): +1000`
  - `Flow(F_vacanze): +1000`
- Effetto:
  - stats: **riduce** le spese (non incrementa le entrate).

### 6.5 Transfer fisico Wallet↔Wallet (non gonfia i report)

Operazione: “sposto 50€ da `W_bank` a `W_cash`”.

- `TransactionKind = InternalWalletTransfer`
- Legs:
  - `Wallet(W_bank): -5000`
  - `Wallet(W_cash): +5000`
- Effetto:
  - stats: escluso (0 impatto su income/expense).

### 6.6 Allocazione tra flow (incluso Unallocated)

Operazione: “allocco 60€ verso `F_emergenze` prendendoli da `Unallocated`”.

- `TransactionKind = InternalFlowTransfer`
- Legs:
  - `Flow(Unallocated): -6000`
  - `Flow(F_emergenze): +6000`
- Effetto:
  - stats: escluso
  - `flow:+x` rispetta cap di `F_emergenze`
  - `Unallocated` può andare negativo; gli altri flow no.

---

## 7. Sharing e permessi

- **Vault owner**: `vault.user_id` è l'owner canonico del vault.
- **Vault memberships**: `vault_memberships(vault_id, user_id, role)` con ruoli
  `owner | editor | viewer`.
- **Flow memberships**: `flow_memberships(flow_id, user_id, role)` con ruoli
  `owner | editor | viewer`.
- **Flow references**: `flow_references(id, vault_id, target_flow_id, display_name, created_at)`
  permette a un flow di apparire in più vault contemporaneamente.
  - Il flow **vive fisicamente in un solo vault** (owner vault): tutti i dati
    (balance, mode, cap, archived) risiedono nel flow originale.
  - Un `flow_reference` è un **puntatore virtuale** che fa apparire il flow
    in un altro vault (recipient vault).
  - Quando un utente riceve un flow condiviso, viene creato:
    - Un `flow_membership` (permessi di accesso al flow)
    - Un `flow_reference` (visibilità nel vault dell'utente)
  - `display_name` (opzionale): override del nome per gestire conflitti quando
    il vault recipient ha già un flow con lo stesso nome.
  - **Single source of truth**: tutto il bilancio e stato del flow rimane nel
    vault owner; i riferimenti sono read-only pointers.
  - **Cross-vault transactions**: un utente con flow_reference può creare
    transazioni che coinvolgono il flow condiviso (es. income da proprio wallet
    al flow condiviso). Il sistema risolve automaticamente il vault corretto
    tramite `resolve_flow_vault()`.
  - **Rimozione riferimento**: un membro può rimuovere il `flow_reference` dal
    proprio vault (unshare); il flow continua a esistere nel vault owner.
  - **Archiviazione**: quando l'owner archivia il flow, appare archiviato anche
    nei vault con riferimenti (filtrato da `vault_snapshot` di default).
- **Access rules**:
  - `read` su vault: owner o qualsiasi membro del vault.
  - `write` su vault: owner o editor.
  - `stats` e gestione membership: **solo owner**.
  - `read/write` su flow: se si ha accesso al vault, il flow è accessibile.
    In alternativa, l'accesso può essere garantito da `flow_memberships`
    (solo per quel flow). Per flow referenziati, richiede sia `flow_membership`
    che `flow_reference` per accesso completo.
  - `resolve_flow_id()`: accetta flow sia direttamente nel vault che via
    `flow_reference`, abilitando transazioni cross-vault.
- **Vault header**: l'API può restituire `id/name/currency` del vault anche
  a chi ha accesso a un flow condiviso, per abilitare la discovery dei flow.
- **Blind 404**: accessi non autorizzati ritornano `KeyNotFound` per non
  rivelare l'esistenza del target.

---

## 8. Errori di dominio

Gli errori sono pensati per essere stabili e user‑friendly (server/bot/TUI possono mapparli a messaggi).

Minimi richiesti:
- `KeyNotFound`: id non trovato o target non accessibile nel vault.
- `ArchivedTarget`: wallet/flow archiviato non accetta nuove legs.
- `InvalidAmount`: `amount_cents == 0` oppure leg mismatch nei transfer.
- `CurrencyMismatch`: currency diversa da quella del vault.
- `FlowCapExceeded`: una operazione con `flow:+x` supera il cap (IncomeCapped o NetCapped).
- `InsufficientFlowFunds`: spend/transfer che porterebbe un flow (≠ Unallocated) sotto 0.
- `ForbiddenOperation`: violazione di regole speciali (es. rinominare/archiviare/cancellare `Unallocated`).
- `IdempotencyKeyConflict`: la key esiste ma con payload incompatibile (opzione futura; default: ritorna existing id).
- `Db(String)`: errore di persistenza non classificato.

---

## 9. Note future (non implementate)

### 9.1 Audit log leggero

Opzione futura: tabella `transaction_revisions` con before/after per conflitti e debug.

### 9.2 Passività / mutui

Opzione futura:
- `AccountType::Liability`
- modello `Loan`
- legs su liability per capitale/interessi.

### 9.3 Permessi wallet read‑only

Opzione futura:
- `legs.attributed_user_id` per saldo per utente.
