#!/usr/bin/env bash
#
# End-to-end test of the app against the real sync server: builds and starts
# `sparagne-server` on a free port with a throwaway data directory, then runs
# `SparagneTests/ServerE2ETests` against it over HTTP.
#
# Runnable from any working directory. Without this script the suite skips,
# because it only runs when SPARAGNE_E2E_SERVER is set.
#
#   DERIVED_DATA=/tmp/dd bash scripts/e2e.sh
#
# DERIVED_DATA (optional) becomes `-derivedDataPath`, so a run can keep its
# build products out of the shared DerivedData another build may be using.
# BUNDLE_ID (optional) gives the test host its own bundle identifier
# (`$BUNDLE_ID.<target>`), so the run does not wait on a Sparagne already
# running with the real one, or on another test run.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"
app_dir="${root_dir}/apple/Sparagne"

server_pid=""
data_dir=""
log_file=""

cleanup() {
    local status=$?
    if [[ -n "${server_pid}" ]] && kill -0 "${server_pid}" 2>/dev/null; then
        kill "${server_pid}" 2>/dev/null || true
        wait "${server_pid}" 2>/dev/null || true
    fi
    if [[ ${status} -ne 0 && -n "${log_file}" && -s "${log_file}" ]]; then
        echo "==> Server log"
        cat "${log_file}"
    fi
    [[ -n "${data_dir}" ]] && rm -rf "${data_dir}"
    return ${status}
}
trap cleanup EXIT

# A port nobody is listening on, from the ephemeral range. The server binds it
# itself, so this is a guess that is checked, not a reservation.
pick_port() {
    local port
    for _ in $(seq 1 50); do
        port=$(((RANDOM % 16384) + 49152))
        if ! nc -z 127.0.0.1 "${port}" >/dev/null 2>&1; then
            echo "${port}"
            return 0
        fi
    done
    echo "error: no free port found in 49152-65535" >&2
    return 1
}

cd "${root_dir}"

echo "==> Building the server"
cargo build --package sparagne_server

port="$(pick_port)"
data_dir="$(mktemp -d)"
log_file="${data_dir}/server.log"
echo "==> Starting sparagne-server on 127.0.0.1:${port} (data in ${data_dir})"
SPARAGNE_BIND="127.0.0.1:${port}" \
    SPARAGNE_DATA_DIR="${data_dir}" \
    SPARAGNE_ALLOW_REGISTRATION=true \
    RUST_LOG="${RUST_LOG:-sparagne_server=info}" \
    "${root_dir}/target/debug/sparagne-server" >"${log_file}" 2>&1 &
server_pid=$!

echo "==> Waiting for GET /health"
ready=false
for _ in $(seq 1 100); do
    if ! kill -0 "${server_pid}" 2>/dev/null; then
        echo "error: the server exited before answering /health" >&2
        exit 1
    fi
    if curl --silent --fail --max-time 2 "http://127.0.0.1:${port}/health" >/dev/null; then
        ready=true
        break
    fi
    sleep 0.2
done
if [[ "${ready}" != true ]]; then
    echo "error: the server never answered /health on port ${port}" >&2
    exit 1
fi

echo "==> Generating the Xcode project"
(cd "${app_dir}" && xcodegen generate)

derived_data_args=()
if [[ -n "${DERIVED_DATA:-}" ]]; then
    derived_data_args=(-derivedDataPath "${DERIVED_DATA}")
fi
bundle_args=()
if [[ -n "${BUNDLE_ID:-}" ]]; then
    bundle_args=("PRODUCT_BUNDLE_IDENTIFIER=${BUNDLE_ID}.\$(TARGET_NAME)")
fi

echo "==> Running SparagneTests/ServerE2ETests against http://127.0.0.1:${port}"
# xcodebuild forwards a TEST_RUNNER_-prefixed variable of its own environment
# to the test process with the prefix stripped, which is how the suite learns
# the server's address. It also goes in as a build setting, which lands in the
# .xctestrun, so both shapes of the mechanism are covered.
set +e
(cd "${app_dir}" && TEST_RUNNER_SPARAGNE_E2E_SERVER="http://127.0.0.1:${port}" xcodebuild \
    -project Sparagne.xcodeproj \
    -scheme Sparagne \
    -destination 'platform=macOS' \
    -only-testing:SparagneTests/ServerE2ETests \
    ${derived_data_args[@]+"${derived_data_args[@]}"} \
    test \
    CODE_SIGNING_ALLOWED=NO \
    ${bundle_args[@]+"${bundle_args[@]}"} \
    "TEST_RUNNER_SPARAGNE_E2E_SERVER=http://127.0.0.1:${port}")
status=$?
set -e

if [[ ${status} -eq 0 ]]; then
    echo "==> e2e passed"
else
    echo "==> e2e failed (xcodebuild exit ${status})" >&2
fi
exit ${status}
