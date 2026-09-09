# SparagneCore

Swift package wrapping the Rust core through UniFFI.

Everything in `Sources/SparagneCore/SparagneCore.swift` is generated; do not
edit it. Hand-written additions live beside it (currently `ErrorCodes.swift`).

## Building

```sh
bash apple/build-core.sh          # from anywhere in the repo
swift test --package-path apple/SparagneCore
```

`SparagneCoreFFI.xcframework` is git-ignored, so a fresh clone must run the
build script before the package resolves.

## Conventions

- Rust `snake_case` becomes Swift `lowerCamelCase` for functions, methods,
  parameters, record fields and enum cases. Error cases keep `UpperCamelCase`.
- Ids and timestamps travel as strings: `Uuid`, `OffsetDateTime` (RFC 3339 with
  offset), `UtcDateTime` (RFC 3339, `Z`) and `NaiveDate` (`yyyy-MM-dd`) are all
  `typealias … = String`.
- Command ids are minted in Rust. Build envelopes with `newEnvelope` or
  `createVaultEnvelope`, never by calling `CommandEnvelope.init` directly.
