# syntax=docker/dockerfile:1

# ---- build ------------------------------------------------------------------
FROM rust:1-slim-bookworm AS build
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app

# Build dependencies first so they are cached until Cargo.toml/lock change.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
 && echo 'fn main() {}' > src/main.rs \
 && echo '' > src/lib.rs \
 && cargo build --release --locked \
 && rm -rf src

# Migrations are embedded into the binary at compile time.
COPY migrations ./migrations
COPY src ./src
RUN touch src/main.rs src/lib.rs \
 && cargo build --release --locked --bin querylab-api

# ---- runtime ----------------------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home querylab
COPY --from=build /app/target/release/querylab-api /usr/local/bin/querylab-api

USER querylab
ENV APP_ENV=production \
    HOST=0.0.0.0 \
    PORT=8080
EXPOSE 8080
# Platforms (Render, Railway) probe GET /health; it reports 503 while the
# metadata database is unreachable.
CMD ["querylab-api"]
