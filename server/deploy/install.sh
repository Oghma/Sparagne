#!/bin/sh
#
# Installs the Sparagne server on a fresh Debian host (a Proxmox LXC, a VM)
# from a release on GitHub (docs/DEPLOY.md §3.3): the binary as a hardened
# systemd service on 127.0.0.1:3000, a daily backup, the update script, and
# by default Caddy in front with HTTPS from its internal CA.
#
#   sh install.sh                          # latest release, HTTPS on this host's address
#   sh install.sh --pre                    # the latest, pre-releases included
#   sh install.sh 2.1.0 --address 192.168.178.81
#   sh install.sh --http                   # no Caddy: plain HTTP on port 3000
#   sh install.sh --allow-registration     # accounts from the app's sign-up
#
# Registration stays closed unless asked for: accounts are then made with
# the server's CLI, as the summary at the end shows. The data directory is
# never touched, so a second run on an installed host refuses; updates go
# through update.sh. POSIX sh, so `sh install.sh` works with dash.
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
CADDY_ROOT=/var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt

info() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

usage() {
    echo "usage: sh install.sh [VERSION] [--pre] [--http] [--address ADDRESS] [--allow-registration]" >&2
    exit 2
}

# --- shared with update.sh -------------------------------------------------

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

# --- install ----------------------------------------------------------------

version=""
pre=""
mode=https
address=""
registration=false
while [ $# -gt 0 ]; do
    case "$1" in
        --pre) pre=pre ;;
        --http) mode=http ;;
        --address) [ $# -ge 2 ] || usage; address=$2; shift ;;
        --allow-registration) registration=true ;;
        -h | --help) usage ;;
        -*) usage ;;
        *) [ -z "$version" ] || usage; version=${1#v} ;;
    esac
    shift
done

[ "$(id -u)" -eq 0 ] || die "run it as root"
command -v apt-get >/dev/null 2>&1 || die "this script is for Debian and its derivatives"
[ -d /run/systemd/system ] || die "systemd is not running here"
if [ -e "$BIN" ] || [ -e "$DATA_DIR/vaults.sqlite" ]; then
    die "Sparagne is already installed here: update it with sh update.sh"
fi

info "Installing the packages"
packages="curl ca-certificates jq sqlite3"
[ "$mode" = https ] && packages="$packages caddy"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
# shellcheck disable=SC2086 # one word per package
apt-get install -y -qq --no-install-recommends $packages >/dev/null

[ -n "$version" ] || version=$(latest_version "$pre")
info "Fetching Sparagne $version"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
release=$(fetch_release "$version" "$(target_triple)" "$work")

info "Installing the server"
id sparagne >/dev/null 2>&1 ||
    useradd --system --home-dir "$DATA_DIR" --shell /usr/sbin/nologin sparagne
install -o sparagne -g sparagne -m 700 -d "$DATA_DIR" "$BACKUP_DIR"
mkdir -p "$STATE_DIR" "$(dirname "$UPDATE_BIN")"
replace "$release/sparagne-server" "$BIN" 755
replace "$release/backup.sh" "$BACKUP_BIN" 755
replace "$release/sparagne-server.service" "$UNIT" 644
replace "$release/update.sh" "$UPDATE_BIN" 755
ln -sf "$UPDATE_BIN" /root/update.sh
cp "$release/.env.example" "$STATE_DIR/env.example"

# The settings: the release's example, with registration and the proxy as
# chosen. A file left by an earlier attempt is kept as it is.
set_env() {
    if grep -q "^#* *$1=" "$ENV_FILE"; then
        sed -i "s|^#* *$1=.*|$1=$2|" "$ENV_FILE"
    else
        echo "$1=$2" >> "$ENV_FILE"
    fi
}
if [ ! -e "$ENV_FILE" ]; then
    install -o root -g sparagne -m 640 "$release/.env.example" "$ENV_FILE"
    set_env SPARAGNE_ALLOW_REGISTRATION "$registration"
    # Behind Caddy on the same host the last X-Forwarded-For entry is the
    # client's; without a proxy nobody may choose their own address.
    if [ "$mode" = https ]; then
        set_env SPARAGNE_TRUST_PROXY true
    else
        set_env SPARAGNE_TRUST_PROXY false
    fi
else
    warn "$ENV_FILE exists: kept as it is"
fi

dropin="/etc/systemd/system/$SERVICE.service.d"
if [ "$mode" = http ]; then
    mkdir -p "$dropin"
    printf '[Service]\nEnvironment=SPARAGNE_BIND=0.0.0.0:3000\n' > "$dropin/listen.conf"
fi

cat > /etc/systemd/system/sparagne-backup.service <<UNIT
[Unit]
Description=Sparagne database backup

[Service]
Type=oneshot
User=sparagne
Environment=DATA_DIR=$DATA_DIR BACKUP_DIR=$BACKUP_DIR RETENTION_DAYS=30
ExecStart=$BACKUP_BIN
UNIT
cat > /etc/systemd/system/sparagne-backup.timer <<UNIT
[Unit]
Description=Daily Sparagne backup

[Timer]
OnCalendar=*-*-* 03:30
Persistent=true

[Install]
WantedBy=timers.target
UNIT

systemctl daemon-reload
systemctl enable --now "$SERVICE" >/dev/null 2>&1
systemctl enable --now sparagne-backup.timer >/dev/null 2>&1
if ! wait_healthy; then
    journalctl -u "$SERVICE" -n 20 --no-pager >&2 || true
    die "the server did not come up (if the log says 226/NAMESPACE, turn on the container's nesting feature)"
fi
echo "$version" > "$STATE_DIR/version"

if [ "$mode" = https ]; then
    [ -n "$address" ] || address=$(hostname -I | awk '{print $1}')
    [ -n "$address" ] || die "cannot tell this host's address: pass --address"
    info "Setting up Caddy for https://$address"
    if [ -f /etc/caddy/Caddyfile ] && ! grep -q "Sparagne's install.sh" /etc/caddy/Caddyfile; then
        cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.orig
    fi
    cat > /etc/caddy/Caddyfile <<CADDY
# Written by Sparagne's install.sh: HTTPS for the server on this address,
# with a certificate from Caddy's internal CA (docs/DEPLOY.md §3.3).
https://$address {
	reverse_proxy 127.0.0.1:3000
}
CADDY
    systemctl enable caddy >/dev/null 2>&1
    systemctl restart caddy
    i=0
    until [ -f "$CADDY_ROOT" ] && curl -fsS --max-time 2 --cacert "$CADDY_ROOT" "https://$address/health" >/dev/null 2>&1; do
        i=$((i + 1))
        [ "$i" -lt 30 ] || die "Caddy did not serve https://$address/health: see journalctl -u caddy"
        sleep 1
    done
    url="https://$address"
else
    [ -n "$address" ] || address=$(hostname -I | awk '{print $1}')
    url="http://$address:3000"
fi

cat <<SUMMARY

Sparagne $version is running.

  Server address for the app:  $url
SUMMARY
if [ "$mode" = https ]; then
    cat <<SUMMARY
  On every Mac that syncs, trust Caddy's root once:
    scp root@$address:$CADDY_ROOT sparagne-root.crt
    sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain sparagne-root.crt
SUMMARY
fi
if [ "$registration" = true ]; then
    cat <<SUMMARY
  Registration is open: sign up from the app, then close it with
    sed -i 's/^SPARAGNE_ALLOW_REGISTRATION=.*/SPARAGNE_ALLOW_REGISTRATION=false/' $ENV_FILE
    systemctl restart $SERVICE
SUMMARY
else
    cat <<SUMMARY
  Accounts (registration is closed); lowercase names:
    read -rs PW; printf '%s\\n' "\$PW" | runuser -u sparagne -- env SPARAGNE_DATA_DIR=$DATA_DIR $BIN user add <name>
SUMMARY
fi
cat <<SUMMARY
  Backups: daily at 03:30 in $BACKUP_DIR (kept 30 days).
  Updates: sh /root/update.sh
SUMMARY
