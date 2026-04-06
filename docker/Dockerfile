# API-only image: build from repository context (unlike Dockerfile.all-in-one, which clones from GitHub).
# Toolchain matches all-in-one: rust:1.90-bookworm + same build deps.
#
# Config: mount `config/default.toml` or rely on baked-in defaults; `ELIDUNE_*` env vars override TOML (see src/config.rs).

FROM rust:1.90-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml ./

# Cache dependency build
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Application source
COPY src ./src
COPY migrations ./migrations
COPY config ./config
COPY data ./data

RUN touch src/main.rs && \
    cargo build --release

# Runtime
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/elidune-server /app/elidune-server
COPY --from=builder /app/migrations /app/migrations
COPY --from=builder /app/config /app/config
COPY --from=builder /app/data /app/data

RUN useradd -m -u 1000 elidune && \
    chown -R elidune:elidune /app

USER elidune

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/api/v1/health || exit 1

CMD ["/app/elidune-server"]
