# Sparagne (macOS app)

SwiftUI/macOS front end for Sparagne v2 (see `docs/v2/ARCH.md` §2.2 and
`docs/v2/UI.md`). It depends on the local `../SparagneCore` Swift package,
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
| `Sparagne/Views/Ledger/` | The single window: `LedgerWindow` (month header, view switcher, status bar), `LedgerHeader`, the `LedgerGrid` spreadsheet, `LedgerRow`, `SummaryPanel` |
| `Sparagne/Views/Summary/` | The RIEPILOGO view (`SummaryView`): the year up to the month on screen |
| `Sparagne/Views/Setup/` | The SETUP view (⌘⇧C): envelopes and categories as two editable tables, the merge sheet |
| `Sparagne/Model/YearModel.swift` | `YearSummary.build`, the arithmetic behind the RIEPILOGO |
| `Sparagne/Views/ManagementSheet.swift` | Vault picker plus wallet and envelope balances and management (⌘⇧M); the window has no sidebar |
| `Sparagne/Views/RecurringPanel.swift` | Recurring templates: list, create, edit, archive |
| `Sparagne/Views/Sheets.swift` | Onboarding, new wallet/envelope, rename and edit-envelope sheets |
| `Sparagne/Views/SyncViews.swift` | The toolbar's sync status button and the rejected-changes and share sheets |
| `Sparagne/Views/UndoToast.swift` | The undo bar after a void |
| `Sparagne/Sync/` | Transport, typed HTTP API, account and token store, and the sync engine (`docs/v2/SYNC.md` §5) |
| `Sparagne/Support/` | Money and date formatting (`Formatters.swift`), the mockup's own money format (`LedgerFormat.swift`), the fixed dark palette (`Palette.swift`), the quick-add preview line (`QuickAddSummary.swift`), error headlines (`ErrorMessages.swift`) |

Sync is off until an account is set up in Settings: server address, then
Register or Log In. From then on the account's username signs every command,
the engine pushes and pulls each vault after every change and every 60 s, and
the owner of a vault can share it from the Vault menu.

The database lives at
`~/Library/Application Support/Sparagne/sparagne.sqlite`, which the sandbox
resolves inside the app container.

## Keyboard

The full table is `docs/v2/UI.md` §6; the shortcuts that reach across the
whole window (`SparagneApp.swift`'s menu commands) are:

| Key | Action |
|---|---|
| `⌘K` | quick-add line over the grid |
| `⌘D` | duplicate the last row |
| `⌘F` | focus the search field |
| `⌘E` | export the rows on screen as CSV |
| `⌘⇧M` | management sheet: vault, wallets, envelopes, recurring |
| `⌘⇧C` | Setup view: envelopes and categories |
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
