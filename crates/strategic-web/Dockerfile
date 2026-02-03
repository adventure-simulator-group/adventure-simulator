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

# Build the strategic-web binary
RUN cargo build --release -p strategic-web

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary
COPY --from=builder /app/target/release/strategic-web /usr/local/bin/

# Copy static files
COPY --from=builder /app/crates/strategic-web/static /static

# Set environment defaults
ENV BIND_ADDRESS=0.0.0.0:8080
ENV STATIC_DIR=/static
ENV SPACETIMEDB_HOST=http://localhost:3000
ENV SPACETIMEDB_DATABASE=strategic-db

EXPOSE 8080

CMD ["strategic-web"]
