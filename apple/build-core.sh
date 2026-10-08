#!/usr/bin/env bash
#
# Builds the Rust core for Apple platforms and refreshes the SparagneCore
# Swift package: the XCFramework with the static library and C headers, and the
# generated Swift wrapper.
#
# Runnable from any working directory. Re-run it after every change to the
# public surface of the core.

set -euo pipefail

# The one Rust target: macOS 27, the app's minimum, runs only on Apple
# silicon, so the XCFramework has a single arm64 slice.
TARGET=aarch64-apple-darwin

CRATE=sparagne_core
LIB_FILE="lib${CRATE}.a"
MODULE=SparagneCore
FFI_MODULE="${MODULE}FFI"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"
package_dir="${script_dir}/${MODULE}"
xcframework="${package_dir}/${FFI_MODULE}.xcframework"
swift_out="${package_dir}/Sources/${MODULE}/${MODULE}.swift"

cd "${root_dir}"

if ! rustup target list --installed | grep -qx "${TARGET}"; then
    echo "error: Rust target '${TARGET}' is not installed." >&2
    echo "       Run: rustup target add ${TARGET}" >&2
    exit 1
fi

work_dir="$(mktemp -d)"
trap 'rm -rf "${work_dir}"' EXIT

echo "==> Building ${CRATE} (release) for: ${TARGET}"
# Must match `platforms:` in Package.swift, or the linker warns that the
# archive was built for a newer macOS than the one being linked against.
export MACOSX_DEPLOYMENT_TARGET=27.0
cargo build --release --target "${TARGET}" --package "${CRATE}"
library="${root_dir}/target/${TARGET}/release/${LIB_FILE}"

echo "==> Generating Swift bindings"
generated="${work_dir}/generated"
mkdir -p "${generated}"
# --no-format keeps the committed file byte-stable across machines that may or
# may not have swift-format installed.
cargo run --quiet --package "${CRATE}" --features cli --bin uniffi-bindgen -- \
    generate \
    --library "${library}" \
    --language swift \
    --out-dir "${generated}" \
    --no-format

# An XCFramework headers directory needs the modulemap under its canonical
# name; uniffi writes it as <FfiModule>.modulemap.
headers="${work_dir}/headers"
mkdir -p "${headers}"
cp "${generated}/${FFI_MODULE}.h" "${headers}/${FFI_MODULE}.h"
cp "${generated}/${FFI_MODULE}.modulemap" "${headers}/module.modulemap"

echo "==> Assembling ${FFI_MODULE}.xcframework"
rm -rf "${xcframework}"
xcodebuild -quiet -create-xcframework \
    -library "${library}" \
    -headers "${headers}" \
    -output "${xcframework}"

mkdir -p "$(dirname "${swift_out}")"
cp "${generated}/${MODULE}.swift" "${swift_out}"

echo
echo "Artefacts:"
echo "  ${xcframework}"
echo "  ${swift_out}"
