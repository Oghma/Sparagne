# Engine Architecture (Stateless by default)

Questo documento chiarisce in modo definitivo le responsabilità e i confini dell’engine.

---

## 1. Decisione: Engine stateless

**Scelta:** l’engine è **stateless**.  
Lo stato autorevole è nel DB; l’engine non mantiene una replica mutabile in RAM.

**Implicazioni**
- Ogni operazione legge dal DB ciò che serve, valida invarianti e scrive in una transazione DB.
- Dopo restart non c’è nessun “replay” o ricostruzione in memoria necessaria.
- Nessun altro componente (server/bot/TUI) accede al DB direttamente.

---

## 2. Confini di responsabilità

### Engine
- Contiene tutta la logica di dominio (SPEC).
- È l’unico layer che legge/scrive DB.
- Espone API di **write** e **read** stabili.
- Applica invarianti:
  - flow non negativi eccetto `Unallocated`,
  - cap dei flow,
  - kind e legs coerenti,
  - soft delete, idempotency, currency.

### Server HTTP / Telegram bot / TUI
- Chiamano solo API dell’engine.
- Non eseguono SQL né ricostruiscono regole di dominio.
- Si occupano solo di:
  - input validation superficiale (shape/typing),
  - presentazione/UX,
  - autenticazione esterna (credenziali/telegram).

---

## 3. Source of truth e dati derivati

**Verità:** `transactions` + `transaction_legs`.  
**Derivato/cache:** `wallets.balance_cents` e `cash_flows.balance_cents`.

Regola:
- I bilanci denormalizzati sono aggiornati **atomically** nella stessa transazione DB della write.
- L’engine fornisce `recompute_balances(vault_id)` per rigenerarli da legs.

---

## 4. Flusso di una write (sequenza)

Esempio: `spend(vault_id, wallet_id, flow_id, amount)`

1. **Read pre‑check**
   - carica dal DB:
     - vault (currency),
     - wallet (saldo),
     - flow target (saldo, mode/cap), o `Unallocated`.
2. **Validate**
   - amount ≠ 0
   - currency coerente
   - kind/legs rispettano SPEC
   - cap rispettato su `flow:+x`
   - flow sorgente non va sotto 0
3. **DB transaction**
   - inserisce `transactions` + `transaction_legs`
   - aggiorna bilanci denormalizzati di wallet/flow coinvolti
   - commit
4. **Return**
   - ritorna `transaction_id`

Se una validazione fallisce o il DB fallisce, non viene scritto nulla.

---

## 5. Flusso di una read

Esempio: `list_transactions_for_flow(flow_id, filter, page)`

1. Query su `transactions` + `legs` filtrando:
   - `voided_at IS NULL` per default
   - kind/date secondo filter
2. Ordinamento per data desc.
3. Paginazione (limit/offset o cursor, secondo Issue 22).
4. Return `Page<TransactionWithLegs>`.

Le statistiche usano le stesse regole:
- escludono kind `Internal*Transfer`
- trattano `Refund` come riduzione spese
- escludono voidate.

---

## 6. Cache (solo opzionale)

L’engine può aggiungere **in futuro** una cache read‑only per query hot.

Vincoli della cache:
- **mai** fonte di verità,
- invalida o TTL breve,
- non modifica il contratto stateless del core write path.

---

## 7. Motivazione sintetica

L’app ha più client e potenzialmente più istanze dell’engine.  
Uno stato mutabile in RAM richiederebbe sincronizzazione e invalidazione complesse.  
Stateless + DB come source of truth riduce bug e prepara sharing/permessi senza riscritture future.

---

## 8. TUI: modalità “standalone” e “remota”

**Decisione:** la TUI parla **sempre** con la stessa API HTTP del server.

### Modalità remota
- La TUI si collega a un server remoto configurando un `api_url`.

---

## 9. Cross-Vault Flow Sharing Architecture

**Decisione:** I flow possono essere condivisi tra utenti usando un sistema di **flow references**.

### Architettura Flow References

**Single Source of Truth**:
- Un flow **vive fisicamente in un solo vault** (owner vault).
- Tutti i dati del flow (balance, mode, cap, archived) risiedono nel record `cash_flows` originale.
- I `flow_references` sono **puntatori virtuali read-only** che fanno apparire il flow in altri vault.

**Tabella flow_references**:
```sql
CREATE TABLE flow_references (
    id BLOB PRIMARY KEY,
    vault_id BLOB NOT NULL,         -- Vault dove appare il riferimento
    target_flow_id BLOB NOT NULL,   -- Flow originale (in altro vault)
    display_name TEXT,               -- Override nome (per conflitti)
    created_at TIMESTAMP NOT NULL,
    FOREIGN KEY (vault_id) REFERENCES vaults(id) ON DELETE CASCADE,
    FOREIGN KEY (target_flow_id) REFERENCES cash_flows(id) ON DELETE CASCADE,
    UNIQUE(vault_id, target_flow_id)
);
```

**Flusso di condivisione**:
1. Owner chiama `share_flow_with_user(vault_id, flow_id, target_user, role)`
2. Engine crea:
   - `flow_memberships` entry (permessi di accesso)
   - `flow_references` entry (visibilità nel vault target)
3. Il flow appare in `vault_snapshot()` del recipient come flow "condiviso" (`is_reference=true`)

**Cross-Vault Transactions**:
- Utente B (recipient) può creare transazioni che coinvolgono il flow condiviso
- Es: `income` da wallet di B al flow condiviso (che vive nel vault di A)
- `resolve_flow_vault()` risolve automaticamente il vault corretto:
  1. Controlla se flow è diretto nel vault corrente
  2. Altrimenti cerca in `flow_references` per trovare il target
  3. Restituisce il `vault_id` dove il flow vive fisicamente
- Le leg updates vengono applicate al vault owner, mantenendo single source of truth

**Vantaggi architetturali**:
- ✅ Nessuna sincronizzazione necessaria
- ✅ Bilancio sempre consistente (unica sorgente)
- ✅ UX intuitiva (flow appare nel proprio vault)
- ✅ Transazioni cross-vault trasparenti
- ✅ Backward compatible (vault sharing esistente non cambia)

**Limitazioni note**:
- Flow references sono read-only pointers; non supportano override locale di balance/mode
- Unallocated flow non può essere condiviso (constraint: system flow)
- Archiviazione: quando owner archivia, tutti i riferimenti vedono flow archiviato

---

## 10. Spunti post‑revamp (future)

- Storage: pianificare upgrade SeaORM (0.12 → 1.x/2.x) o switch a SQLx, come task dedicato.
- TUI: definire un unico API client riusabile (TUI + bot) e supportare modalità remota/standalone senza duplicare logica.
- Auth/users: migliorare autenticazione (oggi Basic auth + password in chiaro) e definire una roadmap per hashing + token/session.
- Release hygiene: aggiungere controlli qualità extra (es. `cargo deny`/`audit`), policy versioning/tag e “how to run locally” più completo.
- Best practice: TLS + auth robusta (Bearer token / OAuth in futuro).

### Modalità standalone (locale)
- La TUI avvia una **istanza locale** del server (stessa crate `crates/server`) e poi si connette via HTTP.
- Il server locale deve bindare solo su `127.0.0.1` e preferibilmente usare **porta random** (bind `:0`) per evitare collisioni.
- Best practice auth locale: token di sessione generato all’avvio e passato via IPC/env (non “no-auth” su porte esposte).

### Motivazione
- Nessuna duplicazione di logica: engine/DB rimangono dietro al server.
- Un solo contratto di integrazione per TUI, bot e futuri client.
