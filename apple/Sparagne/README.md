# Sparagne (macOS app)

SwiftUI/macOS front end for Sparagne v2. It depends on the local `../SparagneCore` Swift package,
which wraps the Rust core over UniFFI; run `bash ../build-core.sh` once after
cloning to produce its XCFramework.

The app owns no domain state: every view is a query on the core and every
action is a command (`AppStore` is the only thing that talks to `CoreClient`,
which is the only thing that talks to `CoreHandle`).

## Layout

| Path | What |
|---|---|
| `Sparagne/Core/CoreClient.swift` | The `CoreHandle` wrapper, the database location and the `Date` ↔ RFC 3339 conversions |
| `Sparagne/Model/AppStore.swift` | `@Observable` store: vaults, snapshot, transactions, filters, deferred undo, errors |
| `Sparagne/Model/TransactionRow.swift` | Table row derived from a `TransactionView` plus the vault snapshot |
| `Sparagne/Model/LedgerModel.swift` | `MonthKey` and the calendar arithmetic behind it, `LedgerDirection`, `LedgerTab` and `LedgerSummary` |
| `Sparagne/App/` | The app entry (`SparagneApp`: menus and shortcuts), `ContentView`, `WindowChrome` (the traffic lights inside the top bar, a draggable bar) and `LaunchOptions` (`-SparagneDatabase`, `-SparagneTab`) |
| `Sparagne/Views/Chrome/` | The window's own chrome: the 44 pt `TopBar` (vault selector, month stepper, search, due pill, Add, `SyncPill`), the `SheetTabBar` along the bottom with each tab's `StatusLine` |
| `Sparagne/Views/Ledger/` | The Mastro tab: `LedgerWindow` (top bar, sheet, tab bar), `FilterBar`, the `LedgerGrid` ruled sheet, `LedgerRow` (with the pending recurring rows), `SelectionBar`, `SummaryPanel`, the command palette |
| `Sparagne/Views/QuickAdd/` | The ⌘K panel: the parsed line as chips, and the command palette after `>` |
| `Sparagne/Views/Summary/` | The Riepilogo tab: KPI cards, fund bars, the month-by-month table, two charts |
| `Sparagne/Views/Recurring/` | The Ricorrenze tab: to confirm, next 30 days, templates table and inspector |
| `Sparagne/Views/Setup/` | The Setup tab (⌘4, ⌘⇧C): vault card, wallets, envelopes and categories as editable tables, the merge sheet |
| `Sparagne/Model/YearModel.swift` | `YearSummary.build`, the arithmetic behind the Riepilogo |
| `Sparagne/Model/` (other) | Pure models, tested without a window: `LedgerLines` (due periods among the rows), `SheetStats`, `QuickAddTokens`, `SyncPillState`, `RecurringAgenda` / `RecurringDraft` / `RecurringNext` / `RecurringMonthly` |
| `Sparagne/Views/ManagementSheet.swift` | Vault picker plus wallet and envelope balances and management (⌘⇧M); the window has no sidebar |
| `Sparagne/Views/Sheets.swift` | Onboarding, new wallet/envelope, rename and edit-envelope sheets |
| `Sparagne/Views/SyncViews.swift` | The rejected-changes and share sheets (the sync status is the top bar's `SyncPill`) |
| `Sparagne/Views/UndoToast.swift` | The undo bar after a void |
| `Sparagne/Sync/` | Transport, typed HTTP API, account and token store, and the sync engine |
| `Sparagne/Support/` | Money and date formatting (`Formatters.swift`), the mockup's own money format (`LedgerFormat.swift`), the fixed dark palette, type scale and metrics (`Palette.swift`), the quick-add preview line (`QuickAddSummary.swift`), error headlines (`ErrorMessages.swift`) |

Sync is off until an account is set up in Settings: server address, then
Register or Log In. From then on the account's username signs every command,
the engine pushes and pulls each vault after every change and every 60 s, and
the owner of a vault can share it from the Vault menu.

The database lives at
`~/Library/Application Support/Sparagne/sparagne.sqlite`, which the sandbox
resolves inside the app container.

## Keyboard

The window has no toolbar: the top bar and the sheet tabs along the bottom
(Riepilogo · Mastro · Ricorrenze · Setup) are drawn by the app. The shortcuts
that reach across the whole window (`SparagneApp.swift`'s menu commands) are:

| Key | Action |
|---|---|
| `⌘1` `⌘2` `⌘3` `⌘4` | the Riepilogo, Mastro, Ricorrenze and Setup tabs (View menu) |
| `⌘K` | quick-add panel over the sheet; a leading `>` turns it into the command palette |
| `⌘D` | duplicate the last row |
| `⌘F` | go to the Mastro and focus the search field in the top bar |
| `⌘E` | export the rows on screen as CSV |
| `⌘⇧M` | management sheet: vault, wallets, envelopes, recurring |
| `⌘⇧C` | the Setup tab: wallets, envelopes and categories |
| `⌘⇧V` / `⌘⇧T` | show voided rows / show transfers |
| `⌥←` / `⌥→` | previous / next month |

## Commands

```sh
xcodegen generate
xcodebuild -project Sparagne.xcodeproj -scheme Sparagne -destination 'platform=macOS' build
xcodebuild -project Sparagne.xcodeproj -scheme Sparagne -destination 'platform=macOS' test
```

The generated `Sparagne.xcodeproj`, build products and `.build` directories
are git-ignored; regenerate the project with `xcodegen generate` any time
`project.yml` changes.

## Demo data

The core's fixture writes a year of a two-person household, up to today,
into a database of its own. The app opens it with `-SparagneDatabase
demo.sqlite` (the **Sparagne Demo** scheme passes it) and never syncs it; the
top bar shows a DEMO badge next to the vault and the sync pill says "Local
only". `-SparagneTab summary|ledger|recurring|setup` opens the window on one
tab:

```sh
# Run from Xcode, sandboxed:
cargo run -p sparagne_core --example seed -- \
  "$HOME/Library/Containers/it.oghma.sparagne/Data/Library/Application Support/Sparagne/demo.sqlite" --replace
# Built with CODE_SIGNING_ALLOWED=NO:
cargo run -p sparagne_core --example seed -- \
  "$HOME/Library/Application Support/Sparagne/demo.sqlite" --replace
open Sparagne.app --args -SparagneDatabase demo.sqlite -SparagneTab ledger
```
