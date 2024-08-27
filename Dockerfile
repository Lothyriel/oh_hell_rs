# Build stage
FROM rust:1.76 as builder

COPY ./src ./src
COPY Cargo.toml ./


RUN cargo build --release

# Prod stage
FROM debian:stable-slim


EXPOSE 3000

COPY --from=builder /target/release/oh_hell /

ENTRYPOINT ["./oh_hell"]
