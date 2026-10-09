#!/bin/sh
#
# Updates a Sparagne server installed by install.sh (docs/DEPLOY.md §3.3) to
# another release, keeping its data: a backup first, then the new binary,
# then a health check. A release that does not come up is rolled back, the
# databases included, since the new binary may already have migrated them.
#
#   sh update.sh                 # the latest release
#   sh update.sh --pre           # the latest, pre-releases included
#   sh update.sh 2.1.0           # that release
#   sh update.sh 2.1.0 --force   # again, even if it is the one installed
#
# install.sh puts this script at /usr/local/sbin/sparagne-update, with
# /root/update.sh pointing at it, and each update replaces it with the
# release's own copy. POSIX sh, so `sh update.sh` works with dash.
#
# SPARAGNE_RELEASE_DIR=<dir> takes the release files from a directory instead
# of GitHub (an offline host, a test).

set -eu

REPO=Oghma/Sparagne
BIN=/usr/local/bin/sparagne-server
BACKUP_BIN=/usr/local/bin/sparagne-backup
UPDATE_BIN=/usr/local/sbin/sparagne-update
UNIT=/etc/systemd/system/sparagne-server.service
ENV_FILE=/etc/sparagne.env
DATA_DIR=/var/lib/sparagne
BACKUP_DIR=/var/backups/sparagne
STATE_DIR=/usr/local/lib/sparagne
SERVICE=sparagne-server

info() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

usage() {
    echo "usage: sh update.sh [VERSION] [--pre] [--force]" >&2
    exit 2
}

# --- shared with install.sh -------------------------------------------------

target_triple() {
    case "$(uname -m)" in
        x86_64 | amd64) echo x86_64-unknown-linux-musl ;;
        aarch64 | arm64) echo aarch64-unknown-linux-musl ;;
        *) die "no release binary for $(uname -m)" ;;
    esac
}

# The newest release on GitHub, without its leading v; with "pre" as the
# argument, pre-releases count too.
latest_version() {
    if [ -n "${SPARAGNE_RELEASE_DIR:-}" ]; then
        die "pass the version to install from SPARAGNE_RELEASE_DIR"
    fi
    command -v jq >/dev/null 2>&1 || die "jq is missing (apt install jq), or pass a version"
    # shellcheck disable=SC2016 # $pre is jq's variable, not the shell's
    filter='map(select(.draft == false and (.prerelease == false or $pre)))[0].tag_name // empty'
    pre=false
    [ "${1:-}" = pre ] && pre=true
    json=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=30") ||
        die "cannot list the releases of $REPO"
    tag=$(printf '%s' "$json" | jq -r --argjson pre "$pre" "$filter") ||
        die "cannot read the releases of $REPO"
    if [ -z "$tag" ]; then
        if [ "$pre" = true ]; then
            die "$REPO has no release yet"
        fi
        die "$REPO has no stable release yet: pass --pre, or a version"
    fi
    echo "${tag#v}"
}

# Downloads (or copies) the archive of $1 for $2 into $3, checks it against
# its checksum and, where `gh` is logged in, against its provenance, then
# unpacks it. Prints the unpacked directory.
fetch_release() {
    version=$1 target=$2 dir=$3
    name="sparagne-server-$version-$target"
    if [ -n "${SPARAGNE_RELEASE_DIR:-}" ]; then
        cp "$SPARAGNE_RELEASE_DIR/$name.tar.gz" "$SPARAGNE_RELEASE_DIR/$name.tar.gz.sha256" "$dir/" ||
            die "$name.tar.gz is not in $SPARAGNE_RELEASE_DIR"
    else
        base="https://github.com/$REPO/releases/download/v$version"
        curl -fsSL -o "$dir/$name.tar.gz" "$base/$name.tar.gz" ||
            die "cannot download $name.tar.gz (is v$version a release?)"
        curl -fsSL -o "$dir/$name.tar.gz.sha256" "$base/$name.tar.gz.sha256" ||
            die "cannot download $name.tar.gz.sha256"
    fi
    (cd "$dir" && sha256sum --quiet -c "$name.tar.gz.sha256") >&2 ||
        die "$name.tar.gz does not match its checksum"
    if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
        gh attestation verify "$dir/$name.tar.gz" --repo "$REPO" >/dev/null ||
            die "$name.tar.gz has no valid provenance from $REPO"
        info "provenance checked: built by $REPO's release workflow" >&2
    else
        info "checksum checked (log in with gh to check the provenance too)" >&2
    fi
    tar -xzf "$dir/$name.tar.gz" -C "$dir"
    echo "$dir/$name"
}

# Replaces $2 with $1 by a rename, so a running copy (this script, the
# server) keeps the file it opened.
replace() {
    cp "$1" "$2.new"
    chmod "$3" "$2.new"
    mv -f "$2.new" "$2"
}

# Where the server listens, as the service sees it (the environment file
# wins over the unit), with 0.0.0.0 asked on the loopback.
health_url() {
    bind=$(sed -n 's/^SPARAGNE_BIND=//p' "$ENV_FILE" 2>/dev/null | tail -n 1 | tr -d "\"'")
    if [ -z "$bind" ]; then
        bind=$(systemctl show "$SERVICE" -p Environment --value | tr ' ' '\n' |
            sed -n 's/^SPARAGNE_BIND=//p' | tail -n 1)
    fi
    [ -n "$bind" ] || bind=127.0.0.1:3000
    port=${bind##*:}
    host=${bind%:*}
    case "$host" in 0.0.0.0 | '[::]' | '') host=127.0.0.1 ;; esac
    echo "http://$host:$port/health"
}

wait_healthy() {
    url=$(health_url)
    i=0
    while [ "$i" -lt 30 ]; do
        if curl -fsS --max-time 2 "$url" >/dev/null 2>&1; then
            return 0
        fi
        i=$((i + 1))
        sleep 1
    done
    return 1
}

# --- update -----------------------------------------------------------------

version=""
pre=""
force=false
for arg in "$@"; do
    case "$arg" in
        --pre) pre=pre ;;
        --force) force=true ;;
        -h | --help) usage ;;
        -*) usage ;;
        *) [ -z "$version" ] || usage; version=${arg#v} ;;
    esac
done

[ "$(id -u)" -eq 0 ] || die "run it as root"
[ -x "$BIN" ] || die "$BIN is missing: install with install.sh first"
for tool in curl sqlite3 sha256sum systemctl runuser; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool is missing (apt install curl sqlite3)"
done

installed=$(cat "$STATE_DIR/version" 2>/dev/null || echo unknown)
[ -n "$version" ] || version=$(latest_version "$pre")

if [ "$installed" = "$version" ] && [ "$force" = false ]; then
    info "Sparagne $version is already installed (--force installs it again)"
    exit 0
fi
info "Updating Sparagne $installed -> $version"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
release=$(fetch_release "$version" "$(target_triple)" "$work")

# A consistent copy of both databases before anything changes; the rollback
# restores it, since the new binary migrates the schema as it starts.
backup=""
if [ -f "$DATA_DIR/vaults.sqlite" ] && [ -f "$DATA_DIR/server.sqlite" ]; then
    info "Backing up the databases"
    install -o sparagne -g sparagne -m 700 -d "$BACKUP_DIR"
    # From /, which the service user can enter: backup.sh's `find` returns to
    # the starting directory, and root's home is closed to it.
    log=$(cd / && runuser -u sparagne -- env DATA_DIR="$DATA_DIR" BACKUP_DIR="$BACKUP_DIR" \
        RETENTION_DAYS="${RETENTION_DAYS:-30}" "$BACKUP_BIN") || die "the backup failed: nothing was changed"
    backup=$(printf '%s\n' "$log" | sed -n 's/^backup\.sh: wrote //p')
    [ -d "$backup" ] || die "the backup did not say where it went: nothing was changed"
    info "backup in $backup"
else
    info "No databases yet: nothing to back up"
fi

# What the rollback puts back.
mkdir -p "$STATE_DIR/previous"
cp -p "$BIN" "$STATE_DIR/previous/sparagne-server"
cp -p "$BACKUP_BIN" "$STATE_DIR/previous/backup.sh"
cp -p "$UNIT" "$STATE_DIR/previous/sparagne-server.service"
echo "$installed" > "$STATE_DIR/previous/version"

info "Installing $version"
systemctl stop "$SERVICE"
replace "$release/sparagne-server" "$BIN" 755
replace "$release/backup.sh" "$BACKUP_BIN" 755
replace "$release/sparagne-server.service" "$UNIT" 644
cp "$release/.env.example" "$STATE_DIR/env.example"
systemctl daemon-reload
systemctl start "$SERVICE"

if wait_healthy; then
    echo "$version" > "$STATE_DIR/version"
    # This script's own copy from the release, for the next update. Older
    # releases did not carry it.
    if [ -f "$release/update.sh" ]; then
        replace "$release/update.sh" "$UPDATE_BIN" 755
        [ -L /root/update.sh ] || ln -sf "$UPDATE_BIN" /root/update.sh
    fi
    # Settings the new release knows and /etc/sparagne.env does not mention.
    sed -n 's/^#* *\([A-Z_][A-Z0-9_]*\)=.*/\1/p' "$STATE_DIR/env.example" | sort -u |
        while read -r key; do
            case "$key" in DOMAIN | SPARAGNE_VERSION) continue ;; esac
            grep -q "^#* *$key=" "$ENV_FILE" 2>/dev/null ||
                info "new setting in this release: $key (see $STATE_DIR/env.example)"
        done
    info "Sparagne $version is up and healthy"
    exit 0
fi

warn "Sparagne $version did not come up; its last log lines:"
journalctl -u "$SERVICE" -n 20 --no-pager >&2 || true
info "Rolling back to $installed"
systemctl stop "$SERVICE" || true
replace "$STATE_DIR/previous/sparagne-server" "$BIN" 755
replace "$STATE_DIR/previous/backup.sh" "$BACKUP_BIN" 755
replace "$STATE_DIR/previous/sparagne-server.service" "$UNIT" 644
if [ -n "$backup" ]; then
    for db in vaults.sqlite server.sqlite; do
        rm -f "$DATA_DIR/$db-wal" "$DATA_DIR/$db-shm"
        install -o sparagne -g sparagne -m 600 "$backup/$db" "$DATA_DIR/$db"
    done
    info "databases restored from $backup"
fi
systemctl daemon-reload
systemctl start "$SERVICE"
if wait_healthy; then
    die "the update failed; Sparagne $installed is back and healthy"
fi
die "the update failed and $installed did not come back either: see journalctl -u $SERVICE"
