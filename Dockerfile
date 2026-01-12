# Build container
FROM rust:1.92-bookworm AS builder

WORKDIR /sparagne
COPY ./ .
RUN apt-get update \
    && apt-get install -y --no-install-recommends libsqlite3-dev libssl-dev pkg-config ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && multiarch="$(dpkg-architecture -qDEB_HOST_MULTIARCH)" \
    && mkdir -p /sparagne/runtime-libs /sparagne/data /sparagne/artifacts \
    && cp -a /usr/lib/${multiarch}/libsqlite3.so.0* /sparagne/runtime-libs/ \
    && cp -a /usr/lib/${multiarch}/libssl.so.* /usr/lib/${multiarch}/libcrypto.so.* /sparagne/runtime-libs/ \
    && cp -a /usr/lib/${multiarch}/libz.so.1 /sparagne/runtime-libs/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/sparagne/target \
    cargo build -p sparagne --release --locked \
    && cp /sparagne/target/release/sparagne /sparagne/artifacts/

# Final image
FROM gcr.io/distroless/cc-debian12

WORKDIR /sparagne

# Copy our build
COPY --from=builder /sparagne/runtime-libs/ /usr/lib/
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder --chown=10001:10001 /sparagne/artifacts/sparagne /sparagne/sparagne
COPY --from=builder --chown=10001:10001 /sparagne/data /sparagne/data
COPY --chown=10001:10001 config/ /sparagne/config/

EXPOSE 3000

USER 10001:10001

CMD ["/sparagne/sparagne"]

VOLUME /sparagne/data
