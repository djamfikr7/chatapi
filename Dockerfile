# Multi-stage build: Rust gateway + SolidJS frontend
FROM rust:1.82 AS builder

WORKDIR /app
COPY . .

# Build frontend
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && apt-get install -y nodejs
RUN cd frontend && npm install && npm run build

# Build gateway
RUN cargo build --release --bin gateway

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy gateway binary
COPY --from=builder /app/target/release/gateway /usr/local/bin/gateway

# Copy frontend build
COPY --from=builder /app/frontend/dist /app/frontend/dist

# Copy default config
COPY --from=builder /app/.chatapi /app/.chatapi

ENV CHATAPI_PORT=8090
ENV CHATAPI_FRONTEND_DIR=/app/frontend/dist
ENV RUST_LOG=gateway=info

EXPOSE 8090

CMD ["gateway"]
