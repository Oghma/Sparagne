# Sparagne (macOS app)

SwiftUI/macOS front end for Sparagne v2 (see `docs/v2/ARCH.md` §2.2 and
`docs/v2/DISTILLATO_V1.md` §3). It depends on the local `../SparagneCore`
Swift package, which wraps the Rust core over UniFFI; run
`bash ../build-core.sh` once after cloning to produce its XCFramework.

The app owns no domain state: every view is a query on the core and every
action is a command (`AppStore` is the only thing that talks to `CoreClient`,
which is the only thing that talks to `CoreHandle`).

## Layout

| Path | What |
|---|---|
| `Sparagne/Core/CoreClient.swift` | The `CoreHandle` wrapper, the database location and the `Date` ↔ RFC 3339 conversions |
| `Sparagne/Model/AppStore.swift` | `@Observable` store: vaults, snapshot, transactions, filters, deferred undo, errors |
| `Sparagne/Model/TransactionRow.swift` | Table row derived from a `TransactionView` plus the vault snapshot |
| `Sparagne/Model/Period.swift` | This month / last 30 days / all, as half-open UTC bounds |
| `Sparagne/Sync/` | Transport, typed HTTP API, account and token store, and the sync engine (`docs/v2/SYNC.md` §5) |
| `Sparagne/Views/` | Sidebar, detail (quick-add, filters, table), inspector, sheets, undo toast, sync status and sharing |
| `Sparagne/Support/` | Money and date formatting, theme, quick-add preview line |

Sync is off until an account is set up in Settings: server address, then
Register or Log In. From then on the account's username signs every command,
the engine pushes and pulls each vault after every change and every 60 s, and
the owner of a vault can share it from the vault menu in the sidebar.

The database lives at
`~/Library/Application Support/Sparagne/sparagne.sqlite`, which the sandbox
resolves inside the app container.

## Commands

```sh
xcodegen generate
xcodebuild -project Sparagne.xcodeproj -scheme Sparagne -destination 'platform=macOS' build
xcodebuild -project Sparagne.xcodeproj -scheme Sparagne -destination 'platform=macOS' test
```

The generated `Sparagne.xcodeproj`, build products and `.build` directories
are git-ignored; regenerate the project with `xcodegen generate` any time
`project.yml` changes.
