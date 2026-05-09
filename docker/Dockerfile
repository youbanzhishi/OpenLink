# OpenLink Dockerfile
# 多阶段构建：Rust 编译 + 最小运行镜像

# ─── 构建阶段 ───────────────────────────────────────────────
FROM rust:1.77-slim AS builder

# 安装编译依赖
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 先复制依赖文件，利用 Docker 缓存
COPY Cargo.toml Cargo.lock* ./
COPY crates/openlink-core/Cargo.toml crates/openlink-core/Cargo.toml
COPY crates/openlink-store/Cargo.toml crates/openlink-store/Cargo.toml
COPY crates/openlink-api/Cargo.toml crates/openlink-api/Cargo.toml
COPY crates/openlink-edge/Cargo.toml crates/openlink-edge/Cargo.toml
COPY crates/openlink-sdk/Cargo.toml crates/openlink-sdk/Cargo.toml
COPY extensions/ext-redirect/Cargo.toml extensions/ext-redirect/Cargo.toml

# 创建空源文件以缓存依赖
RUN mkdir -p crates/openlink-core/src && echo "" > crates/openlink-core/src/lib.rs \
    && mkdir -p crates/openlink-store/src && echo "" > crates/openlink-store/src/lib.rs \
    && mkdir -p crates/openlink-api/src && echo "" > crates/openlink-api/src/lib.rs \
    && mkdir -p crates/openlink-edge/src && echo "" > crates/openlink-edge/src/lib.rs \
    && mkdir -p crates/openlink-sdk/src && echo "" > crates/openlink-sdk/src/lib.rs \
    && mkdir -p extensions/ext-redirect/src && echo "" > extensions/ext-redirect/src/lib.rs

# 构建依赖（缓存层）
RUN cargo build --release -p openlink-api 2>/dev/null || true

# 复制实际源代码
COPY . .

# 触发重新构建（源代码变更）
RUN touch crates/openlink-core/src/lib.rs \
    && touch crates/openlink-store/src/lib.rs \
    && touch crates/openlink-api/src/lib.rs \
    && touch extensions/ext-redirect/src/lib.rs

# 构建
RUN cargo build --release -p openlink-api

# ─── 运行阶段 ───────────────────────────────────────────────
FROM debian:bookworm-slim

# 安装运行时依赖（SQLite 需要）
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# 创建数据目录
RUN mkdir -p /app/data /app/config

# 从构建阶段复制二进制
COPY --from=builder /app/target/release/openlink /app/openlink

# 复制配置
COPY config/default.toml /app/config/default.toml

WORKDIR /app

# 暴露端口
EXPOSE 3000

# 数据目录挂载点
VOLUME ["/app/data"]

# 启动
CMD ["/app/openlink"]
