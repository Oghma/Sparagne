# Sparagne v2 — Statement import rules

> The rules `core/src/statement/` follows when it previews and imports a bank
> or card CSV, with their thresholds and known limits. The fields and codes
> the app reads are documented on the types themselves (`StatementRow`,
> `StatementRowStatus`, `StatementMatchChoice`, `StatementOptions`), which
> the Swift binding copies; `core/tests/statement.rs` pins every rule below.

The app reads the file, `detect` guesses the delimiter and a built-in preset
from the header, the user adjusts a `StatementMapping`, and
`Core::preview_statement` shows what would happen to each row with the
user's `StatementRowOverride`s; `Core::import_statement`, fed the same
overrides, does exactly that. Re-importing the same statement is a no-op:
every row's command id is a UUID v5 of its content (§6), and what an import
decided about a row it left out (which transaction stands for it) is in the
log too (§5).

## 1. What a row is taken for

The action is the first of: the user's; the first description rule whose
`contains` the description holds (case-insensitive, inner runs of spaces
collapsed on both sides, a blank `contains` is no rule, and a rule without
an action leaves the action to what follows); the type rule whose value
equals the type column (case-insensitive); the default action.

A sign that contradicts the action (a refund written as money out) keeps
the action with the absolute amount: the action says what the row is, and
card exports write top-ups with the sign of a spend.

The note is the user's, else the matching rule's, else the payee; the
category is the user's (a blank one is Uncategorized), else the rule's,
else the bank's matched one. A bank category never creates one: it only
selects an active category whose name or alias it equals, with or without a
`5462 - ` merchant code in front. A category typed in a description rule or
an override goes through the normal resolution, so it may create one.

A type or description rule naming a wallet not in the vault, or the
imported one, is refused for the whole statement, as is a rule's category
that cannot be a category's name.

Amounts are read exactly from the text. More decimals than the currency
keeps are rounded half away from zero; `rounded` is set only when that
changed the value (`12.500` is not rounded). The original amount, when the
row has one in another currency or with another value, is appended to the
note in parentheses, after the payee, the rule's or the user's note.

## 2. What becomes of a row

The first reason that applies wins:

1. a row with fewer columns than the header, or more that are not empty,
   or that swallowed the rest of the file in an unclosed quote: `Invalid`
   (`invalid_row`);
2. its id, or the id of the move, the claim or the replacement an earlier
   import wrote for it, already in the vault's log: `AlreadyImported`,
   whatever the rules or the user now say about it;
3. an account going to no wallet: `Skipped` (`skipped_account`); then a
   `Skip` action: `skipped_by_you` when it is the user's, `skipped_by_rule`
   when it is a rule's or the default action; then a status in
   `skip_statuses` (`skipped_status`);
4. an unreadable date: `Invalid` (`invalid_date`); then a day before
   `since`, in the row's own offset: `Skipped` (`before_start`), whatever
   else is wrong with the row; then a currency other than the vault's
   (`currency_mismatch`) or an unreadable amount (`invalid_amount`):
   `Invalid`;
5. a zero amount: `Skipped` (`zero_amount`);
6. one half of a move between two accounts of the file (§3): both
   `Skipped` (`internal_transfer`) when the two go to one wallet, else the
   money-out half `New` as the transfer and the other `Skipped`
   (`transfer_pair`);
7. the other side of a transfer already in the vault: `MatchedTransfer`;
8. a transfer without a usable other wallet: `Skipped` (`needs_wallet`);
9. an entry of the vault that looks like it, or that stands for the
   transfer it is: `PossibleDuplicate`;
10. otherwise `New`.

The user's `skip` leaves any row of 3 to 10 out (`skipped_by_you`), a
matched one too, and nothing is written or claimed for it. The user's
override changes nothing on a row of 1 or 2, nor on a row the mapping alone
reads as invalid: only a row the mapping could import is the user's to
change.

## 3. Accounts and moves between them

An export may hold every account of the user's, as the BudgetBakers Wallet
app's does (preset `budgetbakers-wallet`): the account column says whose a
row is, and `account_wallets` the wallet each account goes to; an account
not listed goes to `wallet_id`, one listed with no wallet is left out. Each
row then goes to its own wallet, and everything said here of "the imported
wallet" is said of that row's: its commands, the rows it matches, what its
import claims or moves. The wallets a file's rows go to are all statements'
wallets.

The transfer column marks the rows that are one half of a move between two
of the user's accounts. Before anything is matched against the vault, the
halves the file holds are paired: a money-out half and a money-in half of
the same amount, from two accounts, at most a day apart, one to one, as many
as can be and then the closest in all. Neither matches anything in the
vault. When both go to one wallet the move changes nothing there: both are
skipped and nothing is written. When they go to two, the money-out half
becomes the transfer from its wallet to the other's, under its own id, and
the import writes it with the claims of both its sides in one batch, the
money-in half's under that half's claim id, so both read as imported from
then on. A half the user changed (its action, or left out) is no half, and a
half whose account is left out leaves the other unpaired. An unpaired half
is imported as what its type says, matched against the vault as any row is;
the user may make it a transfer with an action of their own.

## 4. Rows already in the vault

Every preview matches the rows against the vault again; what earlier
imports decided is read back from their claims, moves and replacements in
the log. Days apart are counted between calendar dates, each in its own
offset. A transfer between the imported wallet and one also checked dated
before `since` (the bank funded from Cash the day before the start) is
never a match, nor an entry of a wallet also checked dated more than 3 days
before it (a pizza typed on the 30th of August may be booked by the bank on
the 1st of September); the imported wallet's own rows and the other banks'
sides of a transfer are, as a bank's start day is not the others'.

- **The other side of a transfer** (4 days): a row that would be new or
  waits for its wallet, taken for a transfer or by its sign, matches a live
  transfer between wallets of the same amount at most 4 days away that goes
  the row's way: out of the imported wallet for money going out, into it
  for money coming in, and to or from the wallet the row names, if it names
  one. Only the imported wallet's transfers count. An expense, income or
  refund action never matches a transfer. The
  matched row reads as the transfer it is: kind `TransferWallet`, the other
  wallet in `counter_wallet_id`.
- **A row typed by hand** (3 days): a new income, expense or refund matches
  a live entry of the same amount moving the money the same way (an expense
  for money going out, an income or a refund for money coming in) at most 3
  days away, in the imported wallet or in a wallet also checked.
- **An entry standing for a transfer** (4 days): a new transfer between the
  imported wallet and the one it names matches an entry of the same amount
  in the wallet it names moving the money the other way (the other bank's
  row imported without a rule), or one in the imported wallet or a wallet
  also checked moving it the row's way (the bonifico typed by hand).

Whose the match is decides whether it may stand for the row:

- in the imported wallet, an entry typed by hand or recorded from a
  recurring template that no claim or move names always may (one typed on
  the 30th may be booked on the 1st), as may a transfer whose side there no
  claim names;
- one a claim or a move names, or an entry an import wrote under its own
  command's id (a UUID v5), stands for a row of an export already in: it
  may only when it is dated within the file's own days, from its first
  readable row on or after the start day to its last, as it is when the
  same file is read with another mapping; and within the match's days (3,
  or 4 for a transfer) of the first or the last of them, where it is more
  likely the next or the previous export's neighbour, the row is imported
  by default;
- in a wallet also checked, an entry a claim or a move names stands for a
  row of another statement and never matches; one an import wrote does (the
  user's own sheet, imported there once);
- in the wallet a transfer row names, any live entry may.

Opening balances, voided rows and the transactions that stand for the rows
of this statement already imported (their own, or the one their claim,
move or replacement names) never match: a row is never its own duplicate,
nor are the rows next to it in an overlapping export. Each amount is paired
in one pass
for every kind of match, one to one: as many rows as can be get a
transaction, and among the ways to pair that many, the one closest in all,
fewest days apart summed, then fewest seconds; on a tie a transfer, then an
entry of the row's kind, then an entry standing for a transfer. So rows on
the 10th and the 13th both pair with rows typed on the 8th and the 11th,
and a row typed on the same day wins over a transfer three days away.

The default choice on a matched row is `ImportAnyway` for a match near the
file's edges that stands for another export's row, `MoveExisting` for a
duplicate found in a wallet also checked, `Skip` for any other duplicate or
transfer, and for an entry standing for the transfer the rule below. On a
duplicate in a wallet also checked `Skip` reads as `MoveExisting`: kept
there, the entry would leave the bank short and be claimed for good.
`MoveExisting` reads as `Skip` on any other duplicate or transfer;
`ReplaceExisting` reads as the default but for an entry standing for the
transfer, where `Skip` and `MoveExisting` read as `ImportAnyway`.
`ImportAnyway` gives the row back what it was (new with its command, or
waiting for its wallet) with its match still in `existing`; the transaction
stays taken in this preview, so no other row gets it, and nothing records
it.

**An entry standing for the transfer is replaced by default only when it
very likely is its other side:** the other bank's statement row (in a
statement's wallet not also checked, an entry an import wrote or a claim or
a move names) at most 4 days away, as a SEPA transfer sent on a Friday
arrives on the Monday; else an entry dated the row's day or a day off in no
wallet also checked (Cash holds the user's own sheet, where an ATM
withdrawal is not the dinner a friend paid back two days on). Never one
dated before the start day (the user typed it, and its own row was left out
as before the start), nor one near the file's edges: the row is then
imported beside it by default, its match still shown. A row replacing its
match stays `PossibleDuplicate`, and is written.

The statements' wallets are those a claim or a move of the log names, and
those the app says receive statements (`statement_wallets`): a statement
that only wrote new rows leaves no claim, so the app's word is needed for
it.

`lookalikes` is advice, and matching never reads it: for every wallet
neither imported into, nor checked, nor a statement's, the new rows that
matched nothing and would match one of its entries as a duplicate in a
wallet checked would. Twelve rows looking like entries of Cash say Cash
should be checked: forgotten, it makes every one of them a second movement,
which importing again does not undo. A statement's wallet is never advised:
checked, its own rows would be moved out of it.

## 5. The import

`Core::import_statement` plans the rows again and runs one command per new
row, oldest first, under the row's id. A new transfer between wallets is
claimed on the imported wallet's side right after it. A move between two
accounts of the file is written as one batch (§3). A row replacing its
match is written as one batch, all of it or nothing on this device: its
transfer, the `VoidTransaction` of the entry (`replace`), the claim of its
own side and, when the entry stood for the named wallet's statement row,
the claim of that wallet's side under the entry's own claim id (the other
row's id when its import wrote the entry), so a preview of the other
statement follows the entry its row names to the
transfer. Then, oldest first, every flagged row left out is recorded:

- a duplicate whose choice is `MoveExisting` is moved into the imported
  wallet (`move-existing`): an `UpdateTransaction` of its `wallet_id`
  alone, so its kind, note, category, envelope, date and person stay;
- a duplicate whose choice is `Skip` is claimed (`claim`): an
  `UpdateTransaction` naming the `wallet_id` it already has;
- the other side of a transfer whose choice is `Skip` is claimed: an
  `UpdateTransaction` naming the imported wallet as the `from_id` or the
  `to_id` it already is.

Each of these ids is derived from the row's (or the entry's), so writing
it twice is a no-op and the row reads as imported from then on. A claim
changes nothing in the vault; it says, in the log, that the transaction
stands for a row of a statement. A claim or a move is told from any other
command by its shape, reserved to the import: an `UpdateTransaction` under
a UUID v5 whose patch carries one of `wallet_id`, `from_id` and `to_id` and
nothing else. The app mints v7 ids, and the script that relabels rows under
v5 ids patches their category only. A claim names the whole entry, or the
side of the transfer on the wallet it names; once both banks of a transfer
are imported, both its sides are claimed.

Refused rows, replacements, claims and moves land in the report on their
line (a claim is refused, like any update, on an entry whose envelope or
wallet is archived, or on a transfer touching an archived wallet; a refused
replacement writes nothing at all); a storage
error stops the import, every command before it stays, and a new run
deduplicates them. A replacing row counts among the possible duplicates and
its transfer among the rows executed and rounded, in the preview as in the
report; claims are counted nowhere else, moves in `moved`.

## 6. Ids

The key of a row is `v1|vault|wallet|date|type|amount|payee|n`: the UTC
instant (`2026-09-16T08:54:40Z`) when the date column fixes one, else the
wall-clock time or the day as written, so importing from another timezone
finds the same rows; the type column's value, or without a type column the
kind the type rules and the default action give the row, never the one a
description rule or the user give it (ids handed out before either existed
stay what they were, and changing what a row is never makes it new again);
the amount in minor units signed as
the file wrote it; the payee lowercased; and `n` the number of rows with
the same key earlier in the file, so two identical rows in the same second
both import. The start day, the envelope or a match never change an id.
With an account column the wallet part is `acct:` and the row's account,
trimmed and lowercased, never the wallet the account goes to. The namespace
(`STATEMENT_NAMESPACE`) and the format are frozen: changing either would
make the next import duplicate every row. Line numbers are those of the
file, header included; a row with a newline inside quotes starts on its
first.

## 7. Known limits

- Matching sees only what this device holds. A device that has not pulled
  what another imported may plan the same rows otherwise, and the server
  takes a command whose id it knows without comparing what it says. A
  claim, a no-op when written, replayed after another device changed the
  entry's wallet, puts the wallet back. A move or a claim the server
  refused stays in the log as refused and still counts on this device: the
  row reads as imported there, and the transaction as claimed. What to do:
  sync before previewing and importing (the app does); dismissing a refused
  command makes its row importable again.
- The import plans the rows again rather than trusting the preview: if the
  vault changed in between, it may pair, move, replace or claim otherwise
  than the preview showed. What to do: preview again after a sync or an
  edit.
- Never check a wallet that is itself a statement's for rows typed by
  hand: the rows its own statement imported would be moved out of it. What
  to do: check only wallets typed into by hand, Cash for one.
- Forgetting to check a wallet typed into by hand is not undone by
  importing again: its rows the statement also holds are in twice, and a
  new import finds the statement's own rows imported. What to do: void the
  hand-typed twins by hand. `lookalikes` names such a wallet before the
  import, to prevent exactly this.
- The app remembers a mapping and its options by the file's header, so two
  banks whose exports share a header share them too. What to do: check the
  wallet and the wallets also checked before each import.
- The statements' wallets the app sends (`statement_wallets`) are those its
  own device remembers importing into: on another device, a bank imported
  only there and whose statement wrote nothing but new rows is no
  statement's. It may then be advised as a wallet to check, and its rows
  standing for a transfer are replaced by default only a day or less away.
  What to do: import each bank from one device, or leave such a wallet
  unchecked.
- The shape of a claim is the import's alone: a bulk script writing
  `UpdateTransaction` under v5 ids must not patch `wallet_id`, `from_id` or
  `to_id` alone.
- Keeping a match (`Skip`) is not undone by importing again: the row reads
  as imported from then on. What to do: type the row by hand. Leaving a row
  out (`skip`) records nothing, and the next import shows it again.
- The server applies a replacement one command at a time, as it does every
  batch: if another device archives either wallet first, it may refuse the
  transfer and accept the void. What to do: type the movement again.
- Replacing an income that was already shared out into envelopes takes it
  out of Unallocated, which can go below zero. What to do: move the money
  back, or decide the allocation again.
- A transfer row replaces one entry only: a bonifico typed by hand into one
  bank and imported as an income into the other leaves one of the two.
  What to do: void it by hand.
- A short file read again with another mapping has most of its rows near
  its edges, and these are imported again by default, their match shown.
  What to do: choose `Skip` on them, or keep the mapping of a file already
  imported.
- Giving the accounts their own wallets later leaves the rows already
  imported where they are (in Cash, say), as their ids hold the account and
  not the wallet. But a move between two accounts that both went to Cash
  wrote nothing then, and read again it is a transfer between the two new
  wallets, dated in the past their other rows stayed behind in. What to do:
  set the start day (`since`) to the day the accounts get their wallets.
- The two halves of a move are paired only when both are new: one half
  imported on its own from an earlier export (its other half then in no
  account of the file) stays as it was imported, and the other half is
  imported as what its type says. The money is right in each wallet, as an
  expense and an income rather than one transfer. What to do: void the two
  and type the transfer, if it matters.
- The transfer column is the export's word: a half it does not mark, or one
  it marks whose other half is not a day away, is imported as what its type
  says.
