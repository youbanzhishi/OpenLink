# OpenLink SDK 开发指南

本指南详细说明 Rust SDK 的使用，包括客户端构建、中间件、批量操作和重试策略。

## 安装

```toml
[dependencies]
openlink-sdk = "1.0.0"
```

## 快速开始

### 使用 LinkClientBuilder

推荐使用 Builder 模式创建客户端：

```rust
use openlink_sdk::LinkClientBuilder;
use openlink_sdk::retry::RetryPolicy;

let client = LinkClientBuilder::new()
    .url("https://api.openlink.dev")
    .api_key("your-api-key")
    .timeout(30)
    .retry_policy(RetryPolicy::exponential_backoff(3, 100, 10_000))
    .circuit_breaker(5, 60)
    .edge_mode(false)
    .build()
    .expect("Failed to build client");
```

### 使用传统 ClientBuilder

```rust
use openlink_sdk::ClientBuilder;

let (link_client, file_client) = ClientBuilder::new()
    .base_url("https://api.openlink.dev")
    .api_token("your-api-key")
    .retry(3)
    .timeout(30)
    .circuit_breaker(5, 60)
    .build();
```

## 链接操作

### 创建短链

```rust
// 简单创建
let link = client.create("https://example.com/long-url").await?;

// 完整参数创建
let link = client.create_full(CreateLinkRequest {
    target: "https://example.com".to_string(),
    code: Some("my-code".to_string()),
    metadata: serde_json::json!({ "tag": "marketing" }),
    is_active: true,
    owner: Some("agent-1".to_string()),
}).await?;
```

### 查询和解析

```rust
// 获取链接信息
let link = client.get("my-code").await?;

// 解析短链（获取目标URL）
let resolved = client.resolve("my-code").await?;
if resolved.found {
    println!("Target: {:?}", resolved.target);
}

// 查询链接列表
let links = client.list(Some(LinkQuery {
    owner: Some("agent-1".to_string()),
    is_active: Some(true),
    limit: Some(20),
    offset: None,
})).await?;
```

### 删除

```rust
client.delete("my-code").await?;
```

## 重试策略

SDK 提供三种重试策略：

### 指数退避（推荐）

等待时间随重试次数指数增长：

```rust
use openlink_sdk::retry::RetryPolicy;

let policy = RetryPolicy::exponential_backoff(
    3,      // 最大重试次数
    100,    // 初始等待 100ms
    10_000, // 最大等待 10s
);

// 延迟序列: 100ms → 200ms → 400ms
```

### 固定间隔

每次重试等待固定时间：

```rust
let policy = RetryPolicy::fixed_interval(
    5,     // 最大重试次数
    1000,  // 每次等待 1s
);
```

### 自定义策略

指定每次重试的精确延迟：

```rust
let policy = RetryPolicy::custom(
    4,
    vec![50, 100, 500, 2000],  // 50ms, 100ms, 500ms, 2s
);
```

### 重试条件

控制哪些错误应该重试：

```rust
use openlink_sdk::retry::RetryCondition;

// 默认条件：重试 408/429/5xx
let condition = RetryCondition::default();

// 仅重试服务器错误
let condition = RetryCondition::server_errors_only();

// 自定义条件
let condition = RetryCondition::new()
    .with_status_code(409)                    // 重试 409 Conflict
    .retry_on_connection_error(true)          // 重试连接错误
    .max_duration(std::time::Duration::from_secs(120));  // 最大重试时长
```

## 中间件链

中间件允许在请求/响应前后执行自定义逻辑。

### 内置中间件

#### AuthMiddleware — 自动注入认证

```rust
use openlink_sdk::middleware::AuthMiddleware;

let auth = AuthMiddleware::new("your-api-key".to_string())
    .with_agent_id("agent-1".to_string())
    .with_device_id("device-001".to_string());
```

#### LoggingMiddleware — 请求日志

```rust
use openlink_sdk::middleware::LoggingMiddleware;

let logging = LoggingMiddleware::new();       // 不记录 Body
let logging = LoggingMiddleware::new().with_bodies();  // 记录 Body
```

#### MetricsMiddleware — 指标收集

```rust
use openlink_sdk::middleware::MetricsMiddleware;

let metrics = MetricsMiddleware::new();

// 在请求后获取指标
let m = metrics.metrics();
println!("Total requests: {}", m.total_requests);
println!("Avg duration: {:.1}ms", m.avg_duration_ms());
println!("Error rate: {:.1}%", m.error_rate() * 100.0);

// 重置指标
metrics.reset();
```

### 组合中间件

```rust
use openlink_sdk::middleware::MiddlewareChain;

let mut chain = MiddlewareChain::new();
chain.add(AuthMiddleware::new("key".to_string()));
chain.add(LoggingMiddleware::new());
chain.add(MetricsMiddleware::new());

// 在请求前执行
let mut req_ctx = RequestContext {
    url: "https://api.example.com/v1/links".to_string(),
    method: "POST".to_string(),
    headers: HashMap::new(),
    body: Some(r#"{"target":"https://example.com"}"#.to_string()),
};
chain.before_request(&mut req_ctx);

// 在响应后执行
let resp_ctx = ResponseContext {
    status: 200,
    headers: HashMap::new(),
    body: None,
    duration_ms: 42,
};
chain.after_response(&resp_ctx);
```

## 批量操作

### 基础批量操作

```rust
use openlink_sdk::BatchClient;

let batch = BatchClient::new(config);

// 批量创建
let result = batch.batch_create(vec![
    CreateLinkRequest { target: "https://a.com".into(), code: None, metadata: serde_json::Value::Null, is_active: true, owner: None },
    CreateLinkRequest { target: "https://b.com".into(), code: None, metadata: serde_json::Value::Null, is_active: true, owner: None },
]).await?;
println!("Created: {}, Failed: {}", result.succeeded, result.failed);

// 批量解析
let resolved = batch.batch_resolve(vec!["abc".into(), "def".into()]).await?;

// 批量删除
let deleted = batch.batch_delete(vec!["abc".into(), "def".into()]).await?;
```

### 并发控制

```rust
// 设置最大并发数（默认 4）
let batch = BatchClient::with_max_concurrency(config, 8);

// 并发解析（每个解析独立执行，受信号量控制）
let result = batch.batch_resolve_concurrent(vec![
    "code1".into(),
    "code2".into(),
    "code3".into(),
]).await;

println!("Succeeded: {}, Failed: {}", result.succeeded, result.failed);
```

## 熔断器

熔断器在连续失败达到阈值后停止请求，防止级联故障：

```rust
// 使用 LinkClientBuilder
let client = LinkClientBuilder::new()
    .url("https://api.openlink.dev")
    .circuit_breaker(5, 60)  // 5次失败后熔断，60秒后尝试恢复
    .build()?;

// 使用 ClientBuilder
let (link, file) = ClientBuilder::new()
    .circuit_breaker(5, 60)
    .build();
```

熔断器状态：
- **Closed**: 正常状态，请求通过
- **Open**: 熔断中，请求被拒绝
- **HalfOpen**: 试探恢复，允许少量请求通过

## 事件订阅

```rust
use openlink_sdk::{EventClient, EventFilter, EventType};

let event_client = EventClient::new(config);

// 订阅事件
let sub = event_client.subscribe(
    EventFilter {
        event_types: vec![EventType::LinkVisited, EventType::FileUploaded],
        link_ids: vec![],
        owner: Some("agent-1".to_string()),
        device_id: None,
    },
    Some("https://example.com/callback".to_string()),
).await?;

// 轮询事件
let events = event_client.poll_events(&sub.subscription_id, Some(50)).await?;

// 取消订阅
event_client.unsubscribe(&sub.subscription_id).await?;
```

## 错误处理

```rust
use openlink_sdk::SdkError;

match client.create("https://example.com").await {
    Ok(link) => println!("Created: {}", link.code),
    Err(SdkError::Auth(msg)) => eprintln!("Authentication failed: {}", msg),
    Err(SdkError::Http { status, message }) => {
        eprintln!("HTTP error {}: {}", status, message);
        if status == 429 {
            // 处理限流
        }
    }
    Err(SdkError::Network(msg)) => eprintln!("Network error: {}", msg),
    Err(e) => eprintln!("Other error: {}", e),
}
```

## 文件传输

```rust
use openlink_sdk::FileClient;

let file_client = FileClient::new(config);

// 上传文件
let result = file_client.upload(
    "document.pdf",
    std::fs::read("document.pdf")?,
    "application/pdf",
).await?;
println!("File ID: {}", result.file_id);
println!("Share code: {:?}", result.share_code);

// 下载文件
let download = file_client.download("file-id").await?;

// 生成分享链接
let share = file_client.share("file-id", Some(3600 * 24 * 7)).await?;
```
