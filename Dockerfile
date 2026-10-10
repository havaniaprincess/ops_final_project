# ---------- build ----------
FROM rust:1-slim-bookworm AS builder
ARG APP_NAME
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock build.rs ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release && rm -rf src target/release/deps/${APP_NAME}*

COPY src ./src
RUN cargo build --release

# ---------- runtime ----------
FROM debian:bookworm-slim
ARG APP_NAME
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 appuser \
    && mkdir -p /app/logs && chown appuser /app/logs

COPY --from=builder /app/target/release/${APP_NAME} /usr/local/bin/app
USER appuser
CMD ["app"]
