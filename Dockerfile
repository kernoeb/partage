FROM oven/bun:1 AS build

WORKDIR /build

COPY ./client/package.json ./client/bun.lockb ./
RUN bun install --frozen-lockfile

COPY ./client .
RUN bun run build

RUN pwd

##############################
FROM lukemathwalker/cargo-chef:latest-rust-alpine AS chef
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
COPY --from=build /build/dist ./client/dist
RUN cargo build --release

##############################
FROM scratch AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/partage /usr/local/bin/partage
ENTRYPOINT ["/usr/local/bin/partage"]
