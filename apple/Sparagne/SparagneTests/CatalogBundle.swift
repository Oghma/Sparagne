import Foundation
import Testing

/// A string catalog built on disk for one language, holding only the keys a
/// test gives it. The helpers under test read it through their `bundle`
/// parameter, so the keys and the plural rules are checked whatever the
/// app's own catalog holds.
enum CatalogBundle {
    /// A bundle in the temporary directory with `plurals` as plural rules
    /// (a `%lld` counted as `one` or `other`) and `strings` as plain
    /// translations. `name` tells one test's bundles from another's in the
    /// folder name and the bundle identifier.
    static func make(
        language: String,
        plurals: [String: (one: String, other: String)],
        strings: [String: String] = [:],
        name: String = "catalog",
        sourceLocation: SourceLocation = #_sourceLocation
    ) throws -> Bundle {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "sparagne-\(name)-\(UUID().uuidString).bundle", directoryHint: .isDirectory)
        let contents = root.appending(path: "Contents", directoryHint: .isDirectory)
        let lproj = contents.appending(path: "Resources/\(language).lproj", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: lproj, withIntermediateDirectories: true)
        let info: [String: Any] = [
            "CFBundleIdentifier": "it.oghma.sparagne.tests.\(name).\(language)",
            "CFBundleDevelopmentRegion": language,
            "CFBundlePackageType": "BNDL",
        ]
        try plist(info).write(to: contents.appending(path: "Info.plist"))
        var rules: [String: Any] = [:]
        for (key, forms) in plurals {
            rules[key] = [
                "NSStringLocalizedFormatKey": "%#@count@",
                "count": [
                    "NSStringFormatSpecTypeKey": "NSStringPluralRuleType",
                    "NSStringFormatValueTypeKey": "lld",
                    "one": forms.one,
                    "other": forms.other,
                ],
            ]
        }
        try plist(rules).write(to: lproj.appending(path: "Localizable.stringsdict"))
        try plist(strings).write(to: lproj.appending(path: "Localizable.strings"))
        return try #require(Bundle(url: root), sourceLocation: sourceLocation)
    }

    private static func plist(_ value: Any) throws -> Data {
        try PropertyListSerialization.data(fromPropertyList: value, format: .xml, options: 0)
    }
}
