# Build container
FROM rust AS builder
RUN update-ca-certificates

WORKDIR /hodlTracker
COPY ./ .

RUN cargo build --release

# Final image
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y openssl ca-certificates  && apt clean && rm -rf /var/lib/apt/lists/*

WORKDIR /hodlTracker

# Copy our build
COPY --from=builder /hodlTracker/target/release/hodl_tracker ./

CMD [ "/hodlTracker/hodl_tracker" ]

VOLUME /hodlTracker/config
