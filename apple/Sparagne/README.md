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
| `Sparagne/Views/` | Sidebar, detail (quick-add, filters, table), inspector, sheets, undo toast |
| `Sparagne/Support/` | Money and date formatting, theme, quick-add preview line |

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
