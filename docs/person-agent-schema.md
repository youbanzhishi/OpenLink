# OpenLink Person Agent Schema

> 版本：v1.0.0
> 更新日期：2026-06-16
> 关联：WO-061 权限校验Hook与Person Agent Schema集成

---

## 一、概述

Person Agent Schema 定义了 OpenLink 中「人」与「Agent」的描述规范，用于：
1. **身份发现**：Agent 如何找到并理解一个人/组织
2. **能力暴露**：人/组织愿意暴露给 Agent 的能力
3. **权限管理**：Agent 访问受保护资源时的权限模型

---

## 二、Agent Card 结构

### 2.1 基础结构

```json
{
  "schema_version": "2024-11-28",
  "name": "小龙的个人Agent",
  "description": "小龙的个人AI助手，提供知识管理、任务协作等服务",
  "url": "https://example.com/agent",
  "portrait": {
    "url": "https://example.com/avatar.png",
    "mime_type": "image/png"
  },
  "category": "personal-assistant",
  "tags": ["productivity", "knowledge-management", "automation"],
  "version": "1.0.0"
}
```

### 2.2 能力声明 (capabilities)

```json
{
  "capabilities": {
    "a2a": {
      "enabled": true,
      "supported_protocols": ["a2a/0.1", "a2a/0.2"],
      "agent_transfer": true,
      "streaming": true
    },
    "mcp": {
      "enabled": true,
      "servers": [
        {
          "name": "openlink-core",
          "command": "npx",
          "args": ["-y", "@openlink/mcp-server"]
        }
      ]
    },
    "memory": {
      "enabled": true,
      "types": ["episodic", "semantic", "procedural"]
    }
  }
}
```

### 2.3 认证与权限 (auth)

```json
{
  "auth": {
    "type": "agent-delegation",
    "delegation": {
      "agent_name": "小龙",
      "protocol": "a2a",
      "description": "通过主代理小龙间接访问受保护服务，私密数据永远不直接暴露"
    },
    "permission_endpoint": "/api/v1/permissions",
    "session_endpoint": "/api/v1/sessions",
    "policy_endpoint": "/api/v1/policies"
  }
}
```

#### auth 字段扩展说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `type` | string | 认证类型：`agent-delegation` / `api-key` / `oauth` |
| `delegation` | object | 代理配置（type=agent-delegation时必填） |
| `delegation.agent_name` | string | 主代理名称 |
| `delegation.protocol` | string | 代理协议：`a2a` |
| `delegation.description` | string | 代理说明 |
| `permission_endpoint` | string | 权限管理API端点 |
| `session_endpoint` | string | 会话管理API端点 |
| `policy_endpoint` | string | 策略模板API端点 |

### 2.4 暴露级别 (exposure)

```json
{
  "exposure": {
    "level": "分层暴露",
    "layers": [
      {
        "name": "public",
        "description": "公开信息，无需认证",
        "access": "anyone",
        "fields": ["name", "portrait", "category"]
      },
      {
        "name": "authorized",
        "description": "授权访问，需要Agent权限",
        "access": "authenticated_agent",
        "fields": ["capabilities", "skills", "tools"]
      },
      {
        "name": "private",
        "description": "私密信息，仅主代理访问",
        "access": "primary_agent_only",
        "fields": ["personal_data", "credentials", "secrets"]
      }
    ]
  }
}
```

### 2.5 技能列表 (skills)

```json
{
  "skills": [
    {
      "id": "knowledge-management",
      "name": "知识管理",
      "description": "管理个人知识库，支持搜索、分类、标签",
      "version": "1.0.0"
    },
    {
      "id": "task-automation",
      "name": "任务自动化",
      "description": "自动化重复性任务，支持定时触发",
      "version": "1.0.0"
    },
    {
      "id": "communication",
      "name": "沟通协作",
      "description": "跨平台消息聚合与回复",
      "version": "1.0.0"
    }
  ]
}
```

### 2.6 工具注册 (tools)

```json
{
  "tools": {
    "total_count": 3,
    "items": [
      {
        "id": "knowledge-search",
        "name": "知识搜索",
        "description": "搜索个人知识库",
        "input_schema": {
          "type": "object",
          "properties": {
            "query": {"type": "string"},
            "limit": {"type": "integer", "default": 10}
          }
        }
      },
      {
        "id": "file-upload",
        "name": "文件上传",
        "description": "上传文件到存储",
        "input_schema": {
          "type": "object",
          "properties": {
            "file": {"type": "string", "format": "binary"},
            "path": {"type": "string"}
          }
        }
      },
      {
        "id": "web-search",
        "name": "网络搜索",
        "description": "执行网络搜索",
        "input_schema": {
          "type": "object",
          "properties": {
            "query": {"type": "string"},
            "engine": {"type": "string", "enum": ["google", "bing", "duckduckgo"]}
          }
        }
      }
    ]
  }
}
```

### 2.7 默认 Agent Card 示例

```json
{
  "schema_version": "2024-11-28",
  "name": "小龙的个人Agent",
  "description": "小龙的个人AI助手，提供知识管理、任务协作等服务",
  "url": "https://example.com/agent",
  "portrait": {
    "url": "https://example.com/avatar.png",
    "mime_type": "image/png"
  },
  "category": "personal-assistant",
  "tags": ["productivity", "knowledge-management", "automation"],
  "version": "1.0.0",
  "capabilities": {
    "a2a": {
      "enabled": true,
      "supported_protocols": ["a2a/0.1", "a2a/0.2"],
      "agent_transfer": true,
      "streaming": true
    },
    "mcp": {
      "enabled": true,
      "servers": []
    },
    "memory": {
      "enabled": true,
      "types": ["episodic", "semantic"]
    }
  },
  "auth": {
    "type": "agent-delegation",
    "delegation": {
      "agent_name": "小龙",
      "protocol": "a2a",
      "description": "通过主代理小龙间接访问受保护服务"
    },
    "permission_endpoint": "/api/v1/permissions",
    "session_endpoint": "/api/v1/sessions",
    "policy_endpoint": "/api/v1/policies"
  },
  "exposure": {
    "level": "分层暴露",
    "layers": [
      {
        "name": "public",
        "description": "公开信息",
        "access": "anyone"
      },
      {
        "name": "authorized",
        "description": "授权访问",
        "access": "authenticated_agent"
      },
      {
        "name": "private",
        "description": "私密信息",
        "access": "primary_agent_only"
      }
    ]
  },
  "skills": [
    {
      "id": "knowledge-management",
      "name": "知识管理"
    },
    {
      "id": "task-automation",
      "name": "任务自动化"
    }
  ],
  "tools": {
    "total_count": 0,
    "items": []
  }
}
```

---

## 三、权限 API 端点

### 3.1 权限管理

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/api/v1/permissions` | 创建Agent权限 |
| GET | `/api/v1/permissions` | 列出当前用户的权限配置 |
| GET | `/api/v1/permissions/{id}` | 获取单个权限详情 |
| PUT | `/api/v1/permissions/{id}` | 更新权限配置 |
| DELETE | `/api/v1/permissions/{id}` | 撤销权限 |

### 3.2 会话管理

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/api/v1/sessions` | 创建会话（获取临时凭证） |
| POST | `/api/v1/sessions/refresh` | 刷新会话令牌 |
| DELETE | `/api/v1/sessions/{id}` | 终止会话 |

### 3.3 策略模板

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/api/v1/policies` | 列出可用策略模板 |
| POST | `/api/v1/policies/apply` | 应用策略创建权限 |

---

## 四、错误类型

| 错误码 | 描述 |
|--------|------|
| `SessionExpired` | 会话已过期 |
| `SessionRevoked` | 会话已被撤销 |
| `ExtensionNotAllowed` | Extension不在白名单中 |
| `OperationNotAllowed` | 操作不被允许 |
| `FileSizeExceeded` | 文件大小超过限制 |
| `RateLimitExceeded` | 请求频率超限 |
| `NoPermissionContext` | 缺少权限上下文 |

---

## 五、版本历史

| 版本 | 日期 | 变更 |
|------|------|------|
| v1.0.0 | 2026-06-16 | 新增auth字段扩展，支持AgentPermission权限模型 |
| v0.2.0 | 2026-06-12 | 新增分层暴露(exposure)机制 |
| v0.1.0 | 2026-06-01 | 初始版本 |

---

*文档版本：v1.0.0*
