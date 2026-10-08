#!/usr/bin/env bash
#
# Builds Sparagne.app from the sources, for the Mac that runs this: the Rust
# core (`apple/build-core.sh`), the Xcode project (xcodegen), then a Release
# build signed ad hoc, so the app sandbox applies as it does to a run from
# Xcode and the database is the same one. The app lands in dist/.
#
#   bash scripts/build-app.sh             # dist/Sparagne.app
#   bash scripts/build-app.sh --install   # and copy it to /Applications
#
# Needs Xcode 27, Rust (rustup reads the version from rust-toolchain.toml)
# and xcodegen. Runnable from any working directory.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"
app_dir="${root_dir}/apple/Sparagne"
dist_dir="${root_dir}/dist"
installed="/Applications/Sparagne.app"

install=false
case "${1:-}" in
    "") ;;
    --install) install=true ;;
    *)
        echo "usage: $0 [--install]" >&2
        exit 2
        ;;
esac

for tool in xcodebuild cargo xcodegen; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
        echo "error: ${tool} is not installed (see README, Install)." >&2
        exit 1
    fi
done

# Before the long part: the copy in /Applications cannot be replaced while it
# runs.
if ${install} && pgrep -x Sparagne >/dev/null; then
    echo "error: Sparagne is running; quit it first." >&2
    exit 1
fi

derived="$(mktemp -d)"
trap 'rm -rf "${derived}"' EXIT

bash "${root_dir}/apple/build-core.sh"

echo "==> Generating the Xcode project"
(cd "${app_dir}" && xcodegen generate --quiet)

echo "==> Building Sparagne (Release)"
xcodebuild -project "${app_dir}/Sparagne.xcodeproj" -scheme Sparagne \
    -configuration Release -destination 'platform=macOS' \
    -derivedDataPath "${derived}" build -quiet

product="${derived}/Build/Products/Release/Sparagne.app"
codesign --verify --strict "${product}"

rm -rf "${dist_dir}/Sparagne.app"
mkdir -p "${dist_dir}"
ditto "${product}" "${dist_dir}/Sparagne.app"
version="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "${dist_dir}/Sparagne.app/Contents/Info.plist")"
echo "==> Sparagne ${version}: ${dist_dir}/Sparagne.app"

if ${install}; then
    rm -rf "${installed}"
    ditto "${dist_dir}/Sparagne.app" "${installed}"
    echo "==> Installed in ${installed}"
fi
