# Stage 1: Build
FROM rust:1.75-alpine as builder

RUN apk add --no-cache musl-dev gcc make openssl-dev

WORKDIR /app
COPY . .

RUN cargo build --release --bin cdd-rust

# Stage 2: Runtime
FROM alpine:3.19

RUN apk add --no-cache libgcc

WORKDIR /app
COPY --from=builder /app/target/release/cdd-rust /usr/local/bin/cdd-rust

EXPOSE 8082

ENTRYPOINT ["cdd-rust", "serve_json_rpc", "--port", "8082", "--listen", "0.0.0.0"]