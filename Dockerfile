# ── Stage 1: 编译（musl静态链接，零glibc依赖）──
FROM rust:1.88-bookworm AS builder

RUN apt-get update && apt-get install -y musl-tools && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /app
COPY . .
# reqwest用rustls，无需openssl
RUN cargo build --release -p openlink-api --target x86_64-unknown-linux-musl

# ── Stage 2: 运行（极简镜像，~10MB）──
FROM alpine:3.20

RUN apk add --no-cache ca-certificates git

# 非root用户运行
RUN adduser -D -s /bin/sh openlink
USER openlink
WORKDIR /opt/openlink

# 二进制
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/openlink-api /usr/local/bin/openlink-api

# 默认配置（可被volume覆盖）
COPY --chown=openlink:openlink config/default.toml /opt/openlink/config/default.toml

# 数据目录
RUN mkdir -p /opt/openlink/data /opt/openlink/knowledge

EXPOSE 3000

ENV RUST_LOG=openlink=info
ENV OPENLINK_CONFIG=/opt/openlink/config/default.toml

CMD ["openlink-api"]
