#!/usr/bin/env python3
"""Keeps apple/Sparagne/Sparagne/Resources/Localizable.xcstrings in step with
the code, since xcodebuild does not sync it.

    scripts/catalog.py check              keys used in code but missing, or without Italian
    scripts/catalog.py merge FRAGMENT...  adds each fragment's keys, then deletes the fragment
    scripts/catalog.py unused             catalog keys no String(localized:) in the code uses
    scripts/catalog.py remove KEY...      drops keys from the catalog

A fragment is a JSON object {key: {"en": value, "it": value}}. A value is a
string, or {"one": ..., "other": ...} for a plural (the key then holds one
%lld). The catalog is rewritten with json.dumps(indent=2, ensure_ascii=False,
sort_keys=True), which round-trips the file byte for byte.
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "apple/Sparagne/Sparagne"
CATALOG = APP / "Resources/Localizable.xcstrings"

LITERAL = re.compile(r'String\(localized:\s*"((?:[^"\\]|\\.)*)"')
PLACEHOLDER = re.compile(r"%(?:\d+\$)?(?:lld|ld|d|@|f|\.\d+f)")


def load():
    return json.loads(CATALOG.read_text())


def save(catalog):
    CATALOG.write_text(json.dumps(catalog, indent=2, ensure_ascii=False, sort_keys=True) + "\n")


def swift_key(literal):
    """The catalog key a Swift literal becomes: escapes decoded, every
    interpolation a placeholder."""
    out, i = [], 0
    while i < len(literal):
        c = literal[i]
        if c == "\\" and i + 1 < len(literal):
            n = literal[i + 1]
            if n == "(":
                depth, j = 1, i + 2
                while j < len(literal) and depth:
                    depth += {"(": 1, ")": -1}.get(literal[j], 0)
                    j += 1
                out.append("\0")
                i = j
                continue
            if n == "u" and literal[i + 2 : i + 3] == "{":
                end = literal.index("}", i)
                out.append(chr(int(literal[i + 3 : end], 16)))
                i = end + 1
                continue
            out.append({"n": "\n", "t": "\t", '"': '"', "\\": "\\"}.get(n, n))
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


def shape(key):
    return PLACEHOLDER.sub("\0", key)


def check():
    catalog = load()["strings"]
    known = {shape(k): k for k in catalog}
    missing, untranslated = set(), set()
    for path in APP.rglob("*.swift"):
        for literal in LITERAL.findall(path.read_text()):
            key = shape(swift_key(literal))
            if key not in known:
                missing.add(key.replace("\0", "%@"))
            elif "it" not in catalog[known[key]].get("localizations", {}):
                untranslated.add(known[key])
    for key in sorted(missing):
        print(f"missing: {key}")
    for key in sorted(untranslated):
        print(f"no italian: {key}")
    print(f"{len(catalog)} keys, {len(missing)} missing, {len(untranslated)} without Italian")
    return 1 if missing or untranslated else 0


def used_shapes():
    return {
        shape(swift_key(literal))
        for path in APP.rglob("*.swift")
        for literal in LITERAL.findall(path.read_text())
    }


def unused():
    used = used_shapes()
    keys = sorted(k for k in load()["strings"] if shape(k) not in used)
    for key in keys:
        print(key)
    print(f"{len(keys)} unused", file=sys.stderr)
    return 0


def remove(keys):
    catalog = load()
    for key in keys:
        if catalog["strings"].pop(key, None) is None:
            print(f"not in catalog: {key}", file=sys.stderr)
    save(catalog)


def unit(value):
    return {"stringUnit": {"state": "translated", "value": value}}


def localization(value):
    if isinstance(value, str):
        return unit(value)
    return {"variations": {"plural": {form: unit(text) for form, text in value.items()}}}


def merge(paths):
    catalog = load()
    for path in map(Path, paths):
        fragment = json.loads(path.read_text())
        for key, values in fragment.items():
            catalog["strings"][key] = {
                "localizations": {lang: localization(v) for lang, v in values.items()}
            }
        print(f"{path.name}: {len(fragment)} keys")
        path.unlink()
    save(catalog)


if __name__ == "__main__":
    if len(sys.argv) >= 2 and sys.argv[1] == "check":
        sys.exit(check())
    if len(sys.argv) == 2 and sys.argv[1] == "unused":
        sys.exit(unused())
    if len(sys.argv) >= 3 and sys.argv[1] == "remove":
        remove(sys.argv[2:])
        sys.exit(0)
    if len(sys.argv) >= 3 and sys.argv[1] == "merge":
        merge(sys.argv[2:])
        sys.exit(0)
    print(__doc__)
    sys.exit(2)
