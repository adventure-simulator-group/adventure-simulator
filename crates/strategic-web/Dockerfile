# Build stage
# Using nightly for resolver = "3" (Cargo 2024 edition feature)
FROM rustlang/rust:nightly-slim as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build the web and tactical server binaries
RUN cargo build --release -p strategic-web -p adventuresim-tactical-server

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary
COPY --from=builder /app/target/release/strategic-web /usr/local/bin/
COPY --from=builder /app/target/release/adventuresim-tactical-server /usr/local/bin/

# Copy static files
COPY --from=builder /app/crates/strategic-web/static /static

# Set environment defaults
ENV BIND_ADDRESS=127.0.0.1:8080
ENV DATABASE_URL=sqlite:///data/adventuresim.db
ENV STATIC_DIR=/static
ENV TACTICAL_STATIC_DIR=/static/tactical
ENV TACTICAL_SERVER_BIN=/usr/local/bin/adventuresim-tactical-server
ENV STRATEGIC_INTERNAL_URL=http://127.0.0.1:8080

EXPOSE 8080
VOLUME ["/data"]

CMD ["strategic-web"]
