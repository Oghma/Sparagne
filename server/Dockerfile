# syntax=docker/dockerfile:1
#
# Build context is the repository root (core/ is a path dependency of
# server/), so this image is built as:
#
#   docker build -f server/Dockerfile .
#
# Two stages: a Rust builder that compiles the release binary, and a slim
# Debian runtime that only carries the binary, certificates and a non-root
# user. See docs/DEPLOY.md for how to run the resulting image.

########################################
# Builder
########################################
# The same release as rust-toolchain.toml.
FROM rust:1.99.0-trixie AS builder
WORKDIR /build

# Cache the dependency graph separately from the application code: copy only
# the manifests first, build a throwaway main so `cargo build` resolves and
# compiles every dependency, then overwrite it with the real sources. As long
# as the manifests and lockfile do not change, Docker reuses this layer.
COPY Cargo.toml Cargo.lock ./
COPY core/Cargo.toml core/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
RUN mkdir -p core/src server/src \
    && echo "fn main() {}" > core/src/lib.rs \
    && echo "fn main() {}" > server/src/main.rs \
    && cargo build --release -p sparagne_server \
    && rm -rf core/src server/src

# Now bring in the real sources and build for real. Touch the sources so
# cargo does not skip them because of the dummy build's timestamps.
COPY core/ core/
COPY server/ server/
RUN touch core/src/lib.rs server/src/main.rs \
    && cargo build --release -p sparagne_server \
    && cp target/release/sparagne-server /build/sparagne-server

########################################
# Runtime
########################################
FROM debian:trixie-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl sqlite3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /home/sparagne --shell /usr/sbin/nologin sparagne \
    && mkdir -p /data /backups \
    && chown -R sparagne:sparagne /data /backups

COPY --from=builder /build/sparagne-server /usr/local/bin/sparagne-server
# sqlite3 is also installed above so this can run as `docker compose exec
# sparagne backup.sh` (docs/DEPLOY.md, server/deploy/backup.sh).
COPY server/deploy/backup.sh /usr/local/bin/backup.sh
RUN chmod 755 /usr/local/bin/backup.sh

USER sparagne
WORKDIR /home/sparagne

ENV SPARAGNE_BIND=0.0.0.0:3000
ENV SPARAGNE_DATA_DIR=/data

VOLUME ["/data"]
EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl --fail --silent "http://127.0.0.1:3000/health" || exit 1

ENTRYPOINT ["/usr/local/bin/sparagne-server"]
