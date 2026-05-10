FROM rust:1.86-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p openlink-api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/openlink-api /usr/local/bin/openlink-api
EXPOSE 3000
CMD ["openlink-api"]
