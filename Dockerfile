# S'Shop — one image: Rust API + built web app.
# Railway builds this automatically (see railway.json).

# ── 1. Web app ────────────────────────────────────────────────
FROM node:22-alpine AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

# ── 2. API server ─────────────────────────────────────────────
FROM rust:1-bookworm AS server
WORKDIR /src
# Cache dependencies separately from application code.
COPY Cargo.toml Cargo.lock ./
COPY server/Cargo.toml server/Cargo.toml
RUN mkdir -p server/src && echo "fn main() {}" > server/src/main.rs \
    && cargo build --release -p sshop \
    && rm -rf server/src target/release/deps/sshop* target/release/sshop*
COPY server/ server/
RUN touch server/src/main.rs && cargo build --release -p sshop

# ── 3. Runtime ────────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 sshop
WORKDIR /app
COPY --from=server /src/target/release/sshop /usr/local/bin/sshop
COPY --from=web /web/dist /app/web
ENV WEB_DIR=/app/web \
    RUST_LOG=info,sqlx=warn,tower_http=info \
    PORT=8080
USER sshop
EXPOSE 8080
CMD ["sshop"]
