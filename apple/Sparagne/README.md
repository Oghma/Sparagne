# Sparagne (macOS app scaffold)

SwiftUI/macOS shell for Sparagne v2 (see `docs/v2/ARCH.md` §2.2 and
`docs/v2/DISTILLATO_V1.md`). This builds standalone, without the
UniFFI-generated `SparagneCore` Swift package: `Sparagne/Model/Placeholders.swift`
stands in for the core's types until that package exists next to this one at
`apple/SparagneCore`.

## Commands

```sh
xcodegen generate
xcodebuild -project Sparagne.xcodeproj -scheme Sparagne -destination 'platform=macOS' build
xcodebuild -project Sparagne.xcodeproj -scheme Sparagne -destination 'platform=macOS' test
```

The generated `Sparagne.xcodeproj`, build products and `.build` directories
are git-ignored; regenerate the project with `xcodegen generate` any time
`project.yml` changes.

## Wiring up the core later

`project.yml` has a commented-out `packages:`/`dependencies:` block showing
how to add the local `../SparagneCore` Swift package once it's generated.
After that, delete `Sparagne/Model/Placeholders.swift` and point the views at
the generated types instead of `SampleData`.
