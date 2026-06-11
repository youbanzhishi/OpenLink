# OpenLink Knowledge Join - P0 MVP 实现文档

## 概述

本实现提供了"知识体系一键接入"功能，允许任意智能体通过短链加入 OpenClaw 知识体系。

## 实现范围（P0 MVP）

### 新增 API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/.well-known/agent.json` | Agent 发现端点 |
| POST | `/api/v1/knowledge/join` | 加入知识体系 |
| GET | `/api/v1/knowledge/entry` | 获取入口文档 |
| GET | `/api/v1/knowledge/role/{name}` | 获取角色 RULES.md |
| GET | `/api/v1/knowledge/project/{name}` | 获取项目 INDEX.md |
| GET | `/api/v1/knowledge/script/{name}` | 获取脚本内容 |
| GET | `/api/v1/knowledge/hot-rules/{role}` | 获取角色热规则 |
| GET | `/api/v1/knowledge/markdown` | 获取知识 Markdown（全角色/项目拼接） |

### 新增 Action 扩展

| Action | 说明 |
|--------|------|
| `knowledge_join` | 返回 JSON 知识包给全能 Agent |
| `knowledge_serve` | 返回 Markdown 知识全文给只读 Agent |

## 配置

在 `config/default.toml` 中配置：

```toml
[knowledge]
enabled = true
repo_path = "/path/to/open-knowledge-system"
base_url = "http://localhost:3000"
invite_codes = ["openclaw-2026", "welcome-agent", "knowledge-join"]
```

## 文件结构

```
openlink/
├── crates/openlink-api/src/
│   ├── handlers/
│   │   └── knowledge.rs    # 知识 API handlers
│   ├── config.rs           # 新增 KnowledgeConfig
│   ├── state.rs            # 新增 knowledge_repo_path
│   ├── router.rs           # 新增知识路由
│   ├── handlers/mod.rs     # 注册 knowledge 模块
│   └── main.rs             # 注册 ext-knowledge-join
├── extensions/
│   └── ext-knowledge-join/  # 新增扩展
│       ├── Cargo.toml
│       └── src/lib.rs
└── config/
    └── default.toml         # 新增 knowledge 配置
```

## 测试方法

### 1. 启动服务

```bash
cd openlink
cargo build
cargo run
```

### 2. 测试 Agent 发现

```bash
curl http://localhost:3000/.well-known/agent.json
```

### 3. 测试加入知识体系

```bash
curl -X POST http://localhost:3000/api/v1/knowledge/join \
  -H "Content-Type: application/json" \
  -d '{
    "invite_code": "openclaw-2026",
    "agent_name": "test-agent",
    "agent_type": "llm",
    "capabilities": ["markdown", "code_execution"]
  }'
```

### 4. 测试获取入口文档

```bash
curl http://localhost:3000/api/v1/knowledge/entry
```

### 5. 测试获取角色 RULES

```bash
curl http://localhost:3000/api/v1/knowledge/role/系统开发者
```

### 6. 测试获取项目 INDEX

```bash
curl http://localhost:3000/api/v1/knowledge/project/OpenLink
```

### 7. 测试获取脚本

```bash
curl http://localhost:3000/api/v1/knowledge/script/act.sh
```

### 8. 测试只读 Agent 知识 Markdown

```bash
curl http://localhost:3000/api/v1/knowledge/markdown
```

## 验收标准

| 编号 | 验收项 | 标准 |
|------|--------|------|
| AC-001 | Agent发现 | `GET /.well-known/agent.json` 返回合法JSON |
| AC-005 | 知识加入 | `POST /api/v1/knowledge/join` 邀请码正确→返回知识全景+Token |
| AC-006 | 知识资源 | `GET /api/v1/knowledge/role/产品经理` 返回RULES.md全文 |
| AC-007 | 无授权拒绝 | 无邀请码或邀请码过期→返回403 |

## 后续工作（P1/P2）

- P1: Agent 注册+身份系统
- P1: 邀请码管理（创建/查询/撤销）
- P1: 权限控制（按 Agent 身份返回不同知识范围）
- P2: Webhook 知识变更推送
- P2: Rust SDK
