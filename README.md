# Sparagne

Sparagne (in Italian "risparmiare") is a Furlan word that means "savings".

Version 2 is a local-first personal finance app for macOS. A Rust core keeps
the domain rules and a SQLite file where every change is a command appended
to a per-vault log; the state tables are a projection of that log. The macOS
app (SwiftUI) talks to the core in-process through UniFFI and never touches
SQL. A small sync server, built on the same core, shares vaults between
accounts and Macs.

Version 1 (Rust engine, HTTP server, Telegram bot, terminal UI) lives at tag
`v0.93.0`.

## Install

Server and app share one version: a release tag `vX.Y.Z` names both. Upgrade
the server before the apps (`docs/DEPLOY.md` §6).

### Server

The release image, for linux/amd64 and linux/arm64, is
`ghcr.io/oghma/sparagne-server:<version>`. On the host:

```sh
cd server/deploy && cp .env.example .env   # set DOMAIN and SPARAGNE_VERSION
docker compose up -d
```

`gh attestation verify oci://ghcr.io/oghma/sparagne-server:<version> --repo
Oghma/Sparagne` checks that an image was built by this repository's release
workflow. `docs/DEPLOY.md` has the full setup (TLS via Caddy, login
limits, backups, upgrades, building the image from the sources).

Accounts can also be managed from the command line, which is the way to add
them once registration is closed: `sparagne-server user add <name>` and
`user passwd <name>` read the password from the first line of stdin
(`passwd` also logs the account out everywhere), `user list` prints every
account, `user revoke <name>` revokes its tokens. They work on the data
directory of a running server, e.g.
`printf '%s\n' "$PW" | docker compose exec -T sparagne sparagne-server user add alice`.

### App

The app is built from the sources on the Mac that runs it (Apple silicon,
macOS 27). There is no download: without an Apple Developer ID, macOS would
stop a downloaded copy, while one built here needs no exception. It takes
Xcode 27, Rust through rustup (the version comes from `rust-toolchain.toml`)
and xcodegen (`brew install xcodegen`). From a release tag:

```sh
bash scripts/build-app.sh --install   # or without --install: dist/Sparagne.app
```

The build is signed ad hoc with the app sandbox, like a run from Xcode, so
both use the same database (below).

## Layout

| Path | What |
|---|---|
| `core/` | Rust crate: domain, commands, log, projection, queries, quick-add |
| `apple/` | Swift package generated from the core and the macOS app |
| `server/` | sync server |
| `docs/` | how to deploy the server (`DEPLOY.md`) |

## Build

```sh
cargo test
cargo clippy --workspace --all-targets
```

### Summary

The Riepilogo tab (⌘1) is the year at a glance, up to the month on screen:
a strip of what needs doing (recurring periods to decide, an allocation
period waiting, rows without a category; a click opens the tab that deals
with it), two rows of cards, the year so far over the month on screen
(income, expenses, savings and the savings rate in both, the year's rate
with the last twelve months' under it and each month's figure with the
month before's; the year's row ends with the months of autonomy, the total
over the average month of expenses of the last twelve finished months, the
month's with the fixed costs the recurring expenses commit), a card per
flow (its balance at the end of the month, what came in and went out in
the month, moves between flows included, and the bar of its cap: the capped
flows still filling first, then the open ones and the full ones;
Unallocated has none), the month-by-month table (with a column per
person), the people
and the flows of the year (what came into each and went out, moves
between flows included), income and expenses and the cash fund as charts,
the year's expenses by category beside its ten heaviest (a click opens the
row in the Mastro), and a months × categories grid. Every flow's
expenses are expenses of the month, whatever its cap. Only the month today
falls in is drawn as still going, its last segment dashed and the cash fund
projected to December at the pace of the finished months; a past month is a
fact. The person filter sits in the top bar, and the person chosen there is
the Mastro's too.

### Allocation plan

The Riparto tab (⌘4) shares out the incomes that reached Unallocated among
the flows, once per period of a schedule (usually monthly). A plan is an
ordered list of lines, each a flow with a fixed amount, a percentage of
the total or whatever fills it up to its cap; the order is the priority when
the total runs short. Nothing moves by itself: when a period is due the tab
shows what each line would get, and you share it out (one transfer per
flow), skip the period, or reopen the latest decision to undo it. The
tabs are Riepilogo (⌘1), Mastro (⌘2), Ricorrenze (⌘3), Riparto (⌘4),
Patrimonio (⌘5) and Setup (⌘6). The plan lives in the vault, so every member sees it and a
period is decided once; before creating one, upgrade every Mac that syncs
(`docs/DEPLOY.md` §6.3). An income counts in one period only: edited or
deleted after its period was shared out, it is not shared again; reopen the
period to redo it.

For a one-off share, Move Between Flows… (Sposta… in the Riparto tab's
top bar, the Vault menu or the palette) moves money from one flow, Unallocated by
default, into several at once: one transfer per line, with the same date and
note, sent all together or not at all (on a shared vault the server still
judges each transfer on its own). A line over its flow's cap, or a
source that would go below zero when it may not, keeps it from moving;
Unallocated below zero is only pointed out. The balances are those of the
date chosen: a move dated in the past can take only what the source held
that day and still holds at its lowest afterwards, so money that came in
later is not there to move, and the sheet names the day of that lowest
point. It decides no period and leaves
the plan as it is. Each transfer is an ordinary ledger row, the same as
`tf> 500 >unallocated >emergenze` typed in the quick-add line.

The same sheet moves money between wallets: its Between switch (or Move
Between Wallets… in the Vault menu and the palette) turns it to the wallets,
the first one the source, Conto into Contanti for a withdrawal. A wallet has
no cap and may go below zero, which is only pointed out, and the flows stay
as they are: the money changes wallet, not flow. The balances are again
those of the date chosen.

### Wealth

The Patrimonio tab (⌘5) sets the wallets' money against what the loans and
mortgages still owe, as of today (it ignores the month and the person on
screen). Three cards: the liquidity, the sum of the wallets' balances; the
remaining debt, the residual of every running loan; and the net worth,
liquidity minus debt. Below, the loans table (name, installment, TAN,
progress, remaining debt, end; the paid-off ones tagged, the archived ones
last) and, for the selected loan, an inspector with its terms, TAEG, what
has been repaid, the interest paid and still to pay, the total cost, the
residual-debt chart, its events and the full amortization schedule.

A loan is entered with the amount, the TAN or the TAEG, the duration, the
first installment's date, and the installment either worked out or typed as
the contract has it: with the amount and the duration, the contract's
installment gives the rate. An installment above the formula's ends the loan
sooner (the last one is smaller); one below it leaves a final balloon
installment, pointed out in a warning. The amortization is French: constant
installment, interest on the remaining debt at the TAN over twelve, rounded
to the cent at every installment, the last one settling the residual. The
schedule is worked out again every time it is read, in integers, from the
terms and the events, so correcting a term or adding a past event redraws it
and every device draws the same one.

Two kinds of event change it. A rate change applies to the installments
falling due after its date, with the installment worked out again or the one
in the bank's letter. An early repayment cuts the debt and either keeps the
installment and ends sooner (shorten) or keeps the end and lowers the
installment (lower), with a penalty if there is one; a repayment above the
debt is cut to the debt, which closes the loan. The sheet can record the
payment in the ledger as an expense, penalty included. Warnings (an
installment that does not cover the interest, a balloon, an event after the
end) never refuse anything: they depend on the schedule, which depends on the
order of the events.

Fees are contractual: upfront ones (application, survey, substitute tax) and
one paid with every installment. The TAEG follows EU Directive 2008/48: a
standard year of twelve equal months, the rate held for the whole loan,
counting interest and those fees but not the notary's; the rate comes from
the TAN. Typing only the TAEG gives the TAN back as an estimate: exact with
no fees, otherwise it overstates the installment. The total cost, on the
other hand, comes from the real schedule, events included.

A loan can be linked to a recurring entry, created from the loan form or an
existing one (a mortgage already in Ricorrenze). The installment is then
recorded at the schedule's amount for that due date, the fee included,
whichever way it is confirmed (Ricorrenze, the Mastro's due rows, Confirm
All), so a rate change reaches the ledger; the entry's template keeps the
current installment and the last due date, updated with every change to the
loan (an amount typed by hand on the template is overwritten by the first
event). A linked entry cannot change kind. The progress is by date, since the
bank debits by itself and installments from before Sparagne count; "recorded
X of Y" next to it checks the ledger. Because the commands are new, upgrade
every Mac that syncs before creating a loan or recording a linked
installment (`docs/DEPLOY.md` §6.4).

Limits: monthly installments only; French amortization only (no Italian
or German schedules); the disbursement is not recorded (a mortgage goes to
the seller, a loan to the shop); interest is not pro rata between due
dates, so an early repayment takes effect at the next installment; and the
net worth counts no assets yet, only the wallets and the debt.

### Statements, full export and backup

File › Import Statement… (⇧⌘I) reads a bank or card CSV (UTF-8, falling back
to Windows-1252/Latin-1). It recognises the built-in `card-transactions`
preset from the header, or reopens the mapping last used for the same vault
and header, with that import's wallet, flow and start day (two banks
exporting the same header share them, wallet included). Pick the
columns, the date and amount format, what each row type becomes (expense,
income, refund, transfer from or to another wallet, skip, by sign), the
statuses to skip, and the target wallet and flow. Description rules come
before the types: the first one whose text the bank's description holds, case
aside, decides the row's kind, category and a cleaner note, so
`PAGAMENTO POS; BAR CENTRALE MILANO` can become an expense under Bar noted
`Bar Centrale`. A start day leaves the older rows out; an entry typed into a
wallet also searched up to 3 days before it can still stand for one of the
file's first rows, while that wallet's older entries and its transfers from
before the start day never do.

The preview shows what happens to every row. Each row can be given another
kind (a transfer names the other wallet), note, category (prefilled from the
vault's history) and flow, or be left out; its context menu makes a
description rule from it. A row with the same amount going the same way as a
transaction already in the vault (an expense for money out, an income or a
refund for money in), at most 3 days apart, in the statement's wallet or in
one ticked under "Duplicates also in" (chosen anew for every file), is a
possible duplicate and is not imported: the transaction found stands for it.
One found in another wallet, such as a spend typed by hand into Cash, is moved
into the statement's wallet or the row imported anyway; one in the statement's
wallet is kept or the row imported anyway. A wallet forgotten under
"Duplicates also in" cannot be made up for by importing the file again: its
rows are in by then. The review and the mapping page name the wallets whose
entries look like new rows of the file, never one a statement goes to, with a
button that ticks them. The other side of a transfer between two of your
wallets, already recorded from the other bank's statement or by hand, is
recognised at most 4 days apart and not imported twice; recorded as an income
or expense instead, it can be deleted and replaced by the transfer. That is
the default when the entry is at most a day apart, or up to four days when
another bank's export put it into a wallet statements go to (the app knows
them from the imports it remembers, so a bonifico that spans a weekend is
replaced by default once the receiving bank's export is in), never for an
entry in a wallet searched under "Duplicates also in" or dated before the
start day. Any matched row can be left out with its box, which writes and
records nothing. Near a file's first or last days, a match that another
export's row already stands for is more likely that row's neighbour: the row
is imported by default, and the review says so. A synced vault is synced
before the preview and again before the import; if the plan changed
meanwhile, or the preview had not caught up with the last change, nothing is
written and the review says so. Importing the same file again adds nothing.

An export holding every account of yours, as the BudgetBakers Wallet app
writes it (`;`, one row per movement with its `account`, its `type` saying
the way and a `transfer` flag), opens on the `Wallet (BudgetBakers)` preset.
The mapping page lists the accounts of the account column with their rows;
each goes to the file's wallet, to another wallet, or is left out, and the
choice is remembered with the import, those wallets then counting as wallets
statements go to. The two halves of a move between two accounts are paired,
at most a day apart: when both go to one wallet nothing is written, since the
money stays there; when they go to two, one transfer is written and the other
half reads as its pair. A half whose other half is not in the file is imported
as its type says, and the review notes it. Giving an account its own wallet
later leaves its rows already imported where they are, but a move between two
accounts that both went to Cash wrote nothing then, and read again it would
become a transfer between the new wallets, dated back among rows that stayed
in Cash: set the start day to the day the accounts get their own wallets.

Every rule the import follows, with its thresholds, and its known limits are
in [docs/STATEMENTS.md](docs/STATEMENTS.md).

File › Export All Transactions… writes every transaction of the vault as CSV,
voided rows and transfers included. File › Back Up Database… saves a
consistent copy, `Sparagne <date>.sqlite`. To restore it, quit Sparagne,
delete `sparagne.sqlite-wal` and `sparagne.sqlite-shm` if present, and replace
`sparagne.sqlite` with the backup renamed. The database lives in
`~/Library/Containers/it.oghma.sparagne/Data/Library/Application Support/Sparagne/`
for a signed (sandboxed) build and in `~/Library/Application Support/Sparagne/`
for an unsigned one; `lsof -p $(pgrep -x Sparagne) | grep sqlite` tells which.
