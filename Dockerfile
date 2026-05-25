# Multi-stage build: frontend + gateway
FROM node:20-slim AS frontend-builder

WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# Gateway build
FROM rust:1.82-slim AS gateway-builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY shared/ shared/
COPY ringbuf/ ringbuf/
COPY rules/ rules/
COPY tools/ tools/
COPY targets/ targets/
COPY sessions/ sessions/
COPY mcp/ mcp/
COPY cdp-engine/ cdp-engine/
COPY gateway/ gateway/

RUN cargo build --release --bin gateway

# Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy gateway binary
COPY --from=gateway-builder /app/target/release/gateway ./

# Copy frontend build
COPY --from=frontend-builder /app/frontend/dist ./frontend/dist

# Default config
RUN mkdir -p .chatapi
COPY config.example.toml .chatapi/config.toml

EXPOSE 8090

ENV CHATAPI_PORT=8090
ENV CHATAPI_FRONTEND_DIR=frontend/dist
ENV RUST_LOG=gateway=info,tower_http=info

CMD ["./gateway"]
