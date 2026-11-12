FROM rust:1.88-bookworm AS build
# sorter2 pins Rust 1.88; match it
RUN rustup install 1.88.0 || true
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
RUN cargo build --release --bin archive

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /app/target/release/archive /app/archive
COPY data /app/data
ENV PORT=8080
EXPOSE 8080
CMD ["/app/archive"]
