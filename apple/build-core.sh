#!/usr/bin/env bash
#
# Builds the Rust core for Apple platforms and refreshes the SparagneCore
# Swift package: the XCFramework with the static library and C headers, and the
# generated Swift wrapper.
#
# Runnable from any working directory. Re-run it after every change to the
# public surface of the core.

set -euo pipefail

# Rust targets to build. Add "x86_64-apple-darwin" here for Intel Macs; the
# slices are merged with lipo into the single macOS slice of the XCFramework.
TARGETS=(aarch64-apple-darwin)

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

installed_targets="$(rustup target list --installed)"
for target in "${TARGETS[@]}"; do
    if ! grep -qx "${target}" <<<"${installed_targets}"; then
        echo "error: Rust target '${target}' is not installed." >&2
        echo "       Run: rustup target add ${target}" >&2
        exit 1
    fi
done

work_dir="$(mktemp -d)"
trap 'rm -rf "${work_dir}"' EXIT

echo "==> Building ${CRATE} (release) for: ${TARGETS[*]}"
# Must match `platforms:` in Package.swift, or the linker warns that the
# archive was built for a newer macOS than the one being linked against.
export MACOSX_DEPLOYMENT_TARGET=27.0
slices=()
for target in "${TARGETS[@]}"; do
    cargo build --release --target "${target}" --package "${CRATE}"
    slices+=("${root_dir}/target/${target}/release/${LIB_FILE}")
done

if [[ ${#slices[@]} -eq 1 ]]; then
    library="${slices[0]}"
else
    echo "==> Merging ${#slices[@]} slices with lipo"
    library="${work_dir}/${LIB_FILE}"
    lipo -create "${slices[@]}" -output "${library}"
fi

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
