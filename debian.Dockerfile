# Stage 1: Build
FROM rust:1.75-bookworm as builder

RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release --bin cdd-rust

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/cdd-rust /usr/local/bin/cdd-rust

EXPOSE 8082

ENTRYPOINT ["cdd-rust", "serve_json_rpc", "--port", "8082", "--listen", "0.0.0.0"]