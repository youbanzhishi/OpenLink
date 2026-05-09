# OpenLink — 智能体时代的通用路由与编排协议

> URL 是人类互联网的入口协议，OpenLink 是智能体互联网的入口协议。

## 项目概述

OpenLink 不是一个短链服务，而是智能体互联网的基础协议层：

- **当下**：短链重定向（必须保留，这是入口）
- **近未来**：Agent 间的发现、握手、协作
- **远未来**：智能体互联网的 DNS + 路由 + 编排

核心哲学：**新功能 = 注册扩展，架构本身永远不需要改**。

## 构建要求

- **Rust** 1.82+ （推荐使用 rustup 安装最新稳定版）
- **C/C++ 编译器**（gcc 或 clang，用于 SQLite 编译）
- **pkg-config**（用于 OpenSSL 链接，可选）

```bash
# 推荐使用 rustup 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## 快速开始

### 构建

```bash
# 构建
cargo build --release

# 运行测试
cargo test

# 运行服务
./target/release/openlink
```

### Docker

```bash
# 构建并启动
cd docker && docker-compose up -d

# 查看日志
docker-compose logs -f openlink
```

## 架构

```
┌─────────────────────────────────────────────┐
│            Protocol Layer（协议层）            │
│  HTTP/WebSocket/MCP/A2A/自定义协议适配         │
├─────────────────────────────────────────────┤
│            Routing Engine（路由引擎）           │
│  Context解析 → Rule匹配 → Action调度          │
├─────────────────────────────────────────────┤
│            Action Layer（动作层）              │
│  Redirect | Webhook | Workflow | Extension... │
├─────────────────────────────────────────────┤
│            Core Store（核心存储）              │
│  Link | Route | Context | Hook | Stats       │
├─────────────────────────────────────────────┤
│            Extension Registry（扩展注册表）     │
│  自定义Action | 自定义Condition | 自定义Hook   │
└─────────────────────────────────────────────┘
```

## 项目结构

```
openlink/
├── Cargo.toml                    # workspace 根
├── crates/
│   ├── openlink-core/            # 核心原语 + 路由引擎 + Extension Registry
│   │   ├── src/
│   │   │   ├── lib.rs            # 模块导出
│   │   │   ├── primitives.rs     # Link/Route/Action/Context/Hook 五个核心原语
│   │   │   ├── engine.rs         # 路由引擎：Context → Rule匹配 → Action调度
│   │   │   ├── registry.rs       # Extension Registry 四柱模型
│   │   │   ├── error.rs          # 统一错误类型
│   │   │   └── shortcode.rs      # Base62 短码生成器
│   │   └── Cargo.toml
│   │
│   ├── openlink-store/           # 存储抽象层
│   │   ├── src/
│   │   │   ├── lib.rs            # 模块导出
│   │   │   ├── traits.rs         # Store trait 定义（不绑定具体数据库）
│   │   │   ├── sqlite.rs         # SQLite 实现
│   │   │   └── error.rs          # 存储层错误类型
│   │   └── Cargo.toml
│   │
│   ├── openlink-api/             # HTTP API（Axum）
│   │   ├── src/
│   │   │   ├── main.rs           # 启动入口
│   │   │   ├── lib.rs            # 模块导出
│   │   │   ├── router.rs         # 路由定义
│   │   │   ├── state.rs          # AppState 共享状态
│   │   │   ├── config.rs         # 应用配置
│   │   │   ├── handlers/         # 请求处理器
│   │   │   │   ├── link.rs       # 短链 CRUD
│   │   │   │   ├── route.rs      # 路由规则管理
│   │   │   │   ├── extension.rs  # 扩展管理
│   │   │   │   └── redirect.rs   # 核心重定向路径
│   │   │   └── middleware/       # 中间件
│   │   │       └── logging.rs    # 请求日志
│   │   └── Cargo.toml
│   │
│   ├── openlink-edge/            # 边缘重定向（WASM，Phase 5 空壳）
│   │   └── src/lib.rs
│   │
│   └── openlink-sdk/             # Agent SDK（Phase 3 空壳）
│       └── src/lib.rs
│
├── extensions/
│   └── ext-redirect/             # 重定向 Action 扩展
│       └── src/lib.rs            # 第一个注册的扩展，验证 Registry
│
├── config/
│   └── default.toml              # 默认配置
│
├── docker/
│   ├── Dockerfile                # 多阶段构建
│   └── docker-compose.yml        # 单容器部署
│
└── README.md
```

## API 概览

### 短链管理

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | /v1/links | 创建短链 |
| GET | /v1/links/:code | 查询短链信息 |
| GET | /v1/links | 列出短链 |
| PUT | /v1/links/:code | 更新短链 |
| DELETE | /v1/links/:code | 删除短链 |
| GET | /v1/links/:code/stats | 访问统计 |

### 路由规则

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | /v1/links/:code/routes | 创建路由规则 |
| PUT | /v1/links/:code/routes/:id | 更新路由规则 |
| DELETE | /v1/links/:code/routes/:id | 删除路由规则 |

### 扩展管理

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | /v1/extensions | 注册扩展 |
| GET | /v1/extensions | 列出扩展 |
| DELETE | /v1/extensions/:name | 卸载扩展 |

### 重定向（核心路径，最快）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | /:code | 短链重定向 → 302 |

## 使用示例

```bash
# 创建短链
curl -X POST http://localhost:3000/v1/links \
  -H "Content-Type: application/json" \
  -d '{"target_url": "https://example.com/very-long-url"}'

# 响应
# {"id":"...","code":"abc123","payload":{"target_url":"https://example.com/very-long-url"},...}

# 访问短链（302 重定向）
curl -v http://localhost:3000/abc123
# → 302 Redirect to https://example.com/very-long-url

# 查询短链信息
curl http://localhost:3000/v1/links/abc123

# 创建带条件路由的短链
curl -X POST http://localhost:3000/v1/links/mylink/routes \
  -H "Content-Type: application/json" \
  -d '{
    "rules": [
      {
        "condition": {"type": "identity-type", "params": {"type": "agent"}},
        "target": {"action": "redirect", "params": {"url": "https://api.example.com/data", "status_code": 302}},
        "priority": 10
      }
    ],
    "default_target": {"action": "redirect", "params": {"url": "https://example.com/page", "status_code": 302}}
  }'

# 查看访问统计
curl http://localhost:3000/v1/links/abc123/stats
```

## 核心设计

### 五个核心原语

| 原语 | 说明 | 关键字段 |
|------|------|----------|
| **Link** | 可寻址实体 | code, payload, owner |
| **Route** | Link→Action 映射 | rules[], default_target |
| **Action** | 执行动作 | Redirect/FileTransfer/Webhook/Workflow/Transform/Delegate/Custom |
| **Context** | 请求上下文 | identity, device, location, time, intent |
| **Hook** | 路由拦截器 | BeforeRoute/AfterRoute/OnError |

### 路由引擎执行流程

```
请求 → 构建 Context
     → BeforeRoute Hooks（改写 Context）
     → Rule 匹配（按优先级，命中即停）
     → 确定 Target（匹配规则 or default）
     → 执行 Action（通过 Extension Registry 查找 Handler）
     → AfterRoute Hooks（日志/通知）
     → 返回响应
```

### Extension Registry 四柱模型

```
┌──────────────────┐  ┌──────────────────┐
│   Action API     │  │ Condition API    │
│   注册新动作      │  │ 注册新路由条件    │
└──────────────────┘  └──────────────────┘
┌──────────────────┐  ┌──────────────────┐
│   Hook API       │  │ Protocol API     │
│   注册拦截器      │  │ 注册协议适配器    │
└──────────────────┘  └──────────────────┘
```

## 设计铁律

1. **核心层零业务逻辑** — 路由引擎不知道"短链"是什么，只知道 Context→Action
2. **新功能=注册扩展** — 任何新场景都不改核心代码
3. **存储层可替换** — 核心逻辑通过 trait 抽象，不绑定具体数据库
4. **传统短链兼容** — `GET /:code → 302` 零配置开箱即用
5. **可观测内置** — 每次路由决策都有完整上下文记录
6. **代码注释** — 每个模块有用途注释，关键设计决策有注释
7. **测试覆盖** — 核心路由引擎和 Extension Registry 有单元测试

## 技术栈

| 组件 | 技术 | 说明 |
|------|------|------|
| 语言 | Rust (edition 2021+) | 要求 rustc 1.82+ |
| Web 框架 | Axum 0.7 + Tokio | 异步高性能 |
| 数据库 | SQLx 0.7 (SQLite) | 后期可切 PostgreSQL |
| 序列化 | serde + serde_json | JSON 原生支持 |
| 短码 | Base62, 6位 | 568 亿组合空间 |
| 错误处理 | thiserror | 类型安全 |
| 日志 | tracing + tracing-subscriber | 结构化日志 |
| 配置 | toml | 简洁可读 |

## Phase 1 完成状态

- [x] 5 个核心原语的数据结构（Link/Route/Action/Context/Hook）
- [x] 路由引擎核心：Context → Rule匹配 → Action调度
- [x] Extension Registry 框架（注册/查询/调用 Action/Condition/Hook）
- [x] Redirect Action 扩展（302/301 重定向）
- [x] HTTP API：短链 CRUD + 重定向
- [x] SQLite 存储（Store trait + SQLite 实现）
- [x] Docker 单容器部署
- [x] 访问日志 + 基础统计
- [x] 单元测试（核心引擎 + Registry + 短码 + SQLite）

## License

MIT
