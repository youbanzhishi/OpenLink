# OpenLink 架构设计

## 概览

OpenLink 采用三层架构设计，以扩展体系为核心，支持灵活的功能组合和协议桥接。

```
┌─────────────────────────────────────────────────────┐
│                    API 层 (axum)                     │
│   REST API / WebSocket / Agent API / Monitoring     │
├─────────────────────────────────────────────────────┤
│                   Core 核心层                        │
│   Link Engine / Router / Extension Host / A2A       │
├─────────────────────────────────────────────────────┤
│                   存储层                             │
│   Store Trait / SQLite / PostgreSQL / Redis Cache   │
└─────────────────────────────────────────────────────┘
```

## 三层架构

### 1. API 层 (`openlink-api`)

基于 `axum` 的高性能 HTTP 服务，提供：

- **REST API**: 短链 CRUD、路由规则、文件传输、扩展管理
- **Agent API**: 智能体专用端点（发现、批量解析、A2A 交互）
- **WebSocket**: 实时事件推送
- **Monitoring**: Prometheus 指标导出、健康检查

#### 请求处理流水线

```
Request → Tower Middleware → Handler → Core Engine → Store
              ↓                                  ↑
         Auth / CORS / Logging              Cache (Redis)
              ↓
         Rate Limiting
```

### 2. Core 核心层 (`openlink-core`)

系统的心脏，包含：

#### 链接引擎 (Link Engine)

- 短码生成：基于 `rand` 的安全随机短码
- 链接生命周期管理：创建、查询、更新、删除
- 元数据存储：JSON 格式，支持任意扩展字段

#### 路由引擎 (Router)

```
请求 → 条件匹配 → 优先级排序 → 动作执行
         ↓
    Geo / Device / Time / Custom
         ↓
    URL / Webhook / Extension Action
```

- **条件类型**: 地理位置、设备类型、时间窗口、自定义表达式
- **动作类型**: URL 重定向、Webhook 触发、扩展动作
- **优先级**: 数值越小优先级越高

#### 扩展宿主 (Extension Host)

- 动态加载扩展（基于 trait 对象）
- 统一的动作执行接口
- 扩展生命周期管理（注册、启用、禁用、卸载）

#### A2A 协议层 (`openlink-a2a`)

- **MCP 适配器**: 模型上下文协议，支持工具注册与调用
- **A2A 市场**: 智能体能力发布与发现
- **信任模型**: 基于声誉分数的信任评估
- **协商协议**: 多智能体间的任务协商
- **去中心化路由**: DHT 分布式路由
- **协议桥接**: MCP ↔ A2A ↔ OpenLink 双向桥接

### 3. 存储层 (`openlink-store`, `openlink-cache`)

#### Store Trait

```rust
#[async_trait]
pub trait LinkStore: Send + Sync {
    async fn create(&self, link: NewLink) -> Result<Link>;
    async fn get(&self, code: &str) -> Result<Option<Link>>;
    async fn list(&self, query: LinkQuery) -> Result<Vec<Link>>;
    async fn delete(&self, code: &str) -> Result<bool>;
    // ...
}
```

- **SQLite**: 开发/单机部署
- **PostgreSQL**: 生产部署（推荐）
- **Redis Cache**: 热点数据缓存，LRU 淘汰策略

## 扩展体系

### 扩展 Trait

```rust
#[async_trait]
pub trait Extension: Send + Sync {
    fn name(&self) -> &str;
    fn actions(&self) -> Vec<ActionDef>;
    async fn execute(&self, action: &str, input: Value) -> Result<Value>;
}
```

### 内置扩展 (12个)

| 扩展 | 功能 |
|------|------|
| ext-redirect | URL 重定向规则 |
| ext-webhook | HTTP Webhook 触发 |
| ext-conditions | 条件表达式求值 |
| ext-hooks | 生命周期钩子 |
| ext-json | JSON 数据转换 |
| ext-workflow | 工作流编排 |
| ext-filetransfer | 文件传输 (R2/S3/WebDAV/SFTP) |
| ext-direct-transfer | P2P 直连传输 |
| ext-p2p-transfer | NAT 穿透 + 分块传输 |
| ext-daw-distribute | DAW 项目分发 |
| ext-a2a-discovery | A2A 智能体发现 |
| ext-orchestrator | 多智能体编排 |

### 扩展加载

```
启动 → 扫描 extensions/ → 注册到 ExtensionHost → 暴露 API 端点
```

## 边缘计算 (`openlink-edge`)

- **WASM 沙箱**: 安全执行用户自定义重定向逻辑
- **地理路由**: 基于 GeoIP 的就近路由
- **边缘缓存**: LRU 缓存，减少回源
- **健康检查**: 后端探活

## 节点网络 (`openlink-node`)

- **LAN 发现**: mDNS 广播，自动发现局域网内节点
- **文件共享**: 跨节点文件传输
- **集群协调**: 节点注册与心跳

## 编排器 (`openlink-orchestrator`)

- **DAG 执行**: 有向无环图工作流
- **并行调度**: 无依赖节点并行执行
- **状态管理**: 工作流实例状态持久化

## 技术选型

| 组件 | 技术 | 原因 |
|------|------|------|
| HTTP 框架 | axum 0.7 | 高性能，Tower 生态 |
| 异步运行时 | tokio | Rust 标准异步方案 |
| 序列化 | serde + serde_json | 生态最广 |
| 错误处理 | thiserror | 零开销错误类型 |
| 数据库 | SQLx | 编译时 SQL 检查 |
| 缓存 | Redis + LRU | 热数据加速 |
| 监控 | Prometheus | 业界标准 |
| 日志 | tracing + tracing-subscriber | 结构化日志 |

## 数据流

### 短链访问流程

```
用户访问 /s/{code}
    → Cache 查询 (Redis)
        → 命中: 直接返回 302 重定向
        → 未命中: Store 查询 (PostgreSQL)
            → 路由规则匹配
                → Geo/Device/Custom 条件判断
                → 执行动作 (redirect/webhook/extension)
            → 写入 Cache
            → 返回 302 重定向
```

### A2A 交互流程

```
Agent A 发布能力 → A2A Market → Agent B 发现能力
                                          ↓
                               Agent B 发起协商
                                          ↓
                               信任评估 (声誉分数)
                                          ↓
                               建立会话 → 执行任务
```
