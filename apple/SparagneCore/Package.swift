// swift-tools-version: 6.0
import PackageDescription

// `SparagneCoreFFI.xcframework` and `Sources/SparagneCore/SparagneCore.swift`
// are produced by `apple/build-core.sh`. The XCFramework is not in git: run
// that script once after cloning, and again after every change to the public
// surface of the Rust core.
let package = Package(
    name: "SparagneCore",
    platforms: [.macOS("27.0")],
    products: [
        .library(name: "SparagneCore", targets: ["SparagneCore"])
    ],
    targets: [
        .binaryTarget(
            name: "SparagneCoreFFI",
            path: "SparagneCoreFFI.xcframework"
        ),
        .target(
            name: "SparagneCore",
            dependencies: ["SparagneCoreFFI"]
        ),
        .testTarget(
            name: "SparagneCoreTests",
            dependencies: ["SparagneCore"]
        ),
    ]
)
