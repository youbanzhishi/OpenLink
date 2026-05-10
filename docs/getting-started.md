# OpenLink 快速入门

本指南帮助你快速搭建 OpenLink 开发环境并创建你的第一个短链。

## 安装

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/your-org/openlink.git
cd openlink

# 构建项目
cargo build --release

# 运行 API 服务
./target/release/openlink-api
```

### 使用 Docker

```bash
# 开发环境
docker-compose -f docker/docker-compose.yml up -d

# 生产环境
docker-compose -f deploy/docker-compose.prod.yml up -d
```

### 通过 SDK 集成

在你的 `Cargo.toml` 中添加：

```toml
[dependencies]
openlink-sdk = "1.0.0"
```

## 配置

OpenLink 使用环境变量或配置文件进行配置：

```bash
# 基础配置
export DATABASE_URL="sqlite:openlink.db"
export REDIS_URL="redis://127.0.0.1:6379"
export OPENLINK_HOST="0.0.0.0"
export OPENLINK_PORT="3000"
export RUST_LOG="openlink=info"
```

对于生产部署，推荐使用 PostgreSQL：

```bash
export DATABASE_URL="postgres://openlink:password@localhost:5432/openlink"
```

## 创建第一个短链

### 使用 cURL

```bash
# 创建短链
curl -X POST http://localhost:3000/api/v1/links \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{
    "target": "https://example.com/my-long-url",
    "is_active": true
  }'

# 响应示例
{
  "id": "link-abc123",
  "code": "x7Kq9m",
  "target": "https://example.com/my-long-url",
  "owner": "default",
  "created_at": "2024-01-15T10:30:00Z",
  "metadata": {},
  "is_active": true
}
```

### 使用 Rust SDK

```rust
use openlink_sdk::LinkClientBuilder;
use openlink_sdk::retry::RetryPolicy;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = LinkClientBuilder::new()
        .url("http://localhost:3000")
        .api_key("your-api-key")
        .timeout(30)
        .retry_policy(RetryPolicy::exponential_backoff(3, 100, 10_000))
        .build()?;

    // 创建短链
    let link = client.create("https://example.com/my-long-url").await?;
    println!("Short code: {}", link.code);
    println!("Target: {}", link.target);

    // 解析短链
    let resolved = client.resolve(&link.code).await?;
    println!("Resolved target: {:?}", resolved.target);

    Ok(())
}
```

## 创建第一个路由规则

路由规则允许你根据条件将短链指向不同的目标：

```bash
# 创建路由规则
curl -X POST http://localhost:3000/api/v1/routes \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{
    "link_id": "link-abc123",
    "rules": [
      {
        "condition": {
          "type": "geo",
          "value": "CN"
        },
        "target": {
          "type": "url",
          "value": "https://cn.example.com"
        },
        "priority": 10
      },
      {
        "condition": {
          "type": "device",
          "value": "mobile"
        },
        "target": {
          "type": "url",
          "value": "https://m.example.com"
        },
        "priority": 20
      }
    ],
    "default_action": {
      "type": "url",
      "value": "https://example.com"
    }
  }'
```

## 使用中间件链

SDK 支持中间件链，可以组合认证、日志和指标收集：

```rust
use openlink_sdk::middleware::{
    MiddlewareChain, AuthMiddleware, LoggingMiddleware, MetricsMiddleware,
};

let mut chain = MiddlewareChain::new();
chain.add(AuthMiddleware::new("your-api-key".to_string()));
chain.add(LoggingMiddleware::new());
chain.add(MetricsMiddleware::new());
```

## 批量操作

```rust
use openlink_sdk::BatchClient;
use openlink_sdk::models::CreateLinkRequest;

let batch_client = BatchClient::with_max_concurrency(config, 8);

// 批量创建
let links = vec![
    CreateLinkRequest {
        target: "https://example.com/1".to_string(),
        code: None,
        metadata: serde_json::Value::Null,
        is_active: true,
        owner: None,
    },
    CreateLinkRequest {
        target: "https://example.com/2".to_string(),
        code: None,
        metadata: serde_json::Value::Null,
        is_active: true,
        owner: None,
    },
];

let result = batch_client.batch_create(links).await?;
println!("Succeeded: {}, Failed: {}", result.succeeded, result.failed);
```

## 下一步

- [API 完整参考](./api-reference.md)
- [架构设计](./architecture.md)
- [部署指南](./deployment.md)
- [SDK 开发指南](./sdk-guide.md)
