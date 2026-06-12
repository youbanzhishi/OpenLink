# OpenLink Docker 部署指南

## 快速启动（3步）

```bash
# 1. Clone 知识仓库
git clone --depth 1 https://github.com/youbanzhishi/open-knowledge-system.git knowledge/open-knowledge-system

# 2. 修改配置
cp config/production.toml config/production.toml.local
# 编辑 config/production.toml.local，替换 sync_token 和 base_url

# 3. 启动
docker compose up -d
```

## 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `OPENLINK_CONFIG` | `/opt/openlink/config/default.toml` | 配置文件路径 |
| `RUST_LOG` | `openlink=info` | 日志级别 |

## 挂载卷

| 宿主路径 | 容器路径 | 说明 |
|---------|---------|------|
| `./config/production.toml` | `/opt/openlink/config/default.toml` | 配置文件(只读) |
| `./knowledge/` | `/opt/openlink/knowledge/` | 知识仓库(只读) |
| `openlink-data` | `/opt/openlink/data/` | 数据持久化 |

## 同步知识仓库

推送后通知Docker实例拉取最新：

```bash
curl -X POST \
  -H "Authorization: Bearer YOUR_SYNC_TOKEN" \
  https://your-domain/api/v1/knowledge/sync
```

> 注意：Docker容器内需要能访问GitHub才能git pull。如果网络受限，可在宿主机手动 `git pull` 后重启容器。

## 镜像信息

- **镜像**: `ghcr.io/youbanzhishi/openlink:latest`
- **基础镜像**: Alpine 3.20（~5MB）
- **编译方式**: musl静态链接（零glibc依赖，兼容所有Linux发行版）
- **架构**: x86_64
- **暴露端口**: 3000

## 标签

- `latest` — 最新版
- `1.1` — 主版本号
- `1.1.1` — 完整版本号
- `sha-xxxxxx` — 对应commit
