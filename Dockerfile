# Build container
FROM rust AS builder
RUN update-ca-certificates

WORKDIR /sparagne
COPY ./ .

RUN cargo build --release

# Final image
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y openssl ca-certificates  && apt clean && rm -rf /var/lib/apt/lists/*

WORKDIR /sparagne

# Copy our build
COPY --from=builder /sparagne/target/release/sparagne ./

CMD [ "/sparagne/sparagne" ]

VOLUME /sparagne/config
