FROM oven/bun:1 AS build

WORKDIR /build

COPY ./client/package.json ./client/bun.lockb ./
RUN bun install --frozen-lockfile

COPY ./client .
RUN bun run build

##############################
FROM lukemathwalker/cargo-chef:latest-rust-alpine AS chef
RUN apk add --no-cache openssl-dev openssl openssl-libs-static bash curl
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall sqlx-cli

WORKDIR /app

##############################
FROM chef AS planner
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
RUN cargo chef prepare --recipe-path recipe.json

##############################
FROM chef AS builder
COPY --from=planner /app/recipe.json .
RUN cargo chef cook --release
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
COPY ./migrations ./migrations
COPY --from=build /build/dist ./client/dist

ENV DATABASE_URL=sqlite:/tmp/ci.db
RUN sqlx database create
RUN sqlx migrate run
RUN cargo build --release

##############################
FROM scratch AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/partage /usr/local/bin/partage
ENTRYPOINT ["/usr/local/bin/partage"]
