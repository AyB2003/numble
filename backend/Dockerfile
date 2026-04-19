FROM rust:latest AS builder
WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY src src

RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/backend /usr/local/bin/backend

EXPOSE 3001
ENV JWT_SECRET=dev-secret-change-me

CMD ["backend"]
