# OpenLink AI 智能体内置指南

> 本文档面向 AI Agent，提供结构化的 OpenLink 功能参考。
> Agent 阅读本文档后应能理解 OpenLink 是什么、能调什么 API、怎么帮用户完成路由与发布。

---

## 1. OpenLink 是什么

OpenLink 是智能体时代的通用路由与编排协议——智能体互联网的 DNS。

**核心特征**：
- Rust 原生，Axum 高性能异步 HTTP 服务
- 五个核心原语：Link / Route / Action / Context / Hook
- Extension Registry 四柱模型：新功能 = 注册扩展，架构本身永远不需要改
- Context 感知路由：根据访问者身份、设备、位置等上下文动态分发
- 名片系统：可编程的 Agent Identity Card，支持多主题渲染

**交互接口**：

| 接口 | 入口 | 说明 |
|------|------|------|
| API | `http://localhost:3000/api/v1` | REST API |
| 短链 | `http://localhost:3000/{code}` | 302 重定向 |
| 名片 | `http://localhost:3000/card/{code}` | 渲染 Identity Card |
| Agent | `POST /api/v1/agent/discover` | Agent 发现接口 |

**发现协议**：`GET /.well-known/agent.json` 返回完整能力声明。

---

## 2. API 速查表

### 2.1 链接管理

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| POST | `/api/v1/links` | 创建短链 | 是 |
| GET | `/api/v1/links` | 列出链接 | 否 |
| GET | `/api/v1/links/{code}` | 获取链接详情 | 否 |
| DELETE | `/api/v1/links/{code}` | 删除链接 | 是 |
| GET | `/api/v1/resolve/{code}` | 解析短链 | 否 |

### 2.2 名片管理

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| POST | `/api/v1/cards` | 创建名片 | 是 |
| GET | `/api/v1/cards` | 列出名片 | 否 |
| GET | `/api/v1/cards/{code}` | 获取名片详情 | 否 |
| PUT | `/api/v1/cards/{code}` | 更新名片 | 是 |
| DELETE | `/api/v1/cards/{code}` | 删除名片 | 是 |
| GET | `/card/{code}` | 渲染名片（HTML） | 否 |
| GET | `/card/{code}/qr` | 名片二维码 | 否 |

### 2.3 路由规则

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| POST | `/api/v1/routes` | 创建路由规则 | 是 |
| GET | `/api/v1/routes/{link_id}` | 获取路由规则 | 否 |

### 2.4 统计与监控

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| GET | `/api/v1/stats/overview` | 全局统计概览 | 否 |
| GET | `/api/v1/stats/links/{id}` | 单链接访问统计 | 否 |
| GET | `/health` | 健康检查 | 否 |

### 2.5 扩展系统

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| GET | `/api/v1/extensions` | 列出扩展 | 否 |
| POST | `/api/v1/extensions/{name}/actions/{action}` | 执行扩展动作 | 是 |

### 2.6 文件传输

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| POST | `/api/v1/files/upload` | 请求上传 | 是 |
| GET | `/api/v1/files/{file_id}/download` | 下载文件 | 否 |
| GET | `/api/v1/files/share/{share_code}` | 通过分享码获取 | 否 |

### 2.7 Agent 专用

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| POST | `/api/v1/agent/discover` | 发现链接 | 否 |
| POST | `/api/v1/agent/resolve` | 批量解析 | 否 |

---

## 3. Agent Action Protocol v2 定义

> OpenLink 的 agent.json v2 能力声明，遵循 [Agent Action Protocol](https://github.com/youbanzhishi/open-knowledge-system/blob/main/共享知识/设计模式/Agent-Action-Protocol.md)。

### agent.json v2

```json
{
  "schema_version": "2.0",
  "name": "openlink",
  "description": "智能体时代的通用路由与编排协议——智能体互联网的DNS",
  "version": "1.0.1",
  "base_url": "http://localhost:3000",
  "auth": {
    "type": "bearer",
    "header": "Authorization"
  },
  "capabilities": [
    {
      "name": "create_link",
      "description": "创建短链，支持自定义短码、元数据、路由规则",
      "category": "create",
      "endpoint": "POST /api/v1/links",
      "input": {
        "type": "object",
        "properties": {
          "target": {
            "type": "string",
            "description": "目标URL"
          },
          "code": {
            "type": "string",
            "description": "自定义短码，不填则自动生成"
          },
          "metadata": {
            "type": "object",
            "description": "附加元数据，如标签、分类等"
          },
          "is_active": {
            "type": "boolean",
            "description": "是否启用，默认true"
          },
          "owner": {
            "type": "string",
            "description": "所有者标识"
          }
        },
        "required": ["target"]
      },
      "output": {
        "type": "object",
        "properties": {
          "id": { "type": "string", "description": "链接ID" },
          "code": { "type": "string", "description": "短码" },
          "target": { "type": "string" },
          "owner": { "type": "string" },
          "created_at": { "type": "string" },
          "metadata": { "type": "object" },
          "is_active": { "type": "boolean" }
        }
      },
      "examples": [
        {
          "input": {
            "target": "https://cdn.example.com/mixes/summer-song.wav",
            "metadata": { "tag": "mix-export", "project": "夏日之歌" },
            "owner": "opendaw"
          },
          "output": {
            "id": "link-abc123",
            "code": "x7K1",
            "target": "https://cdn.example.com/mixes/summer-song.wav",
            "owner": "opendaw",
            "created_at": "2024-06-15T10:30:00Z",
            "metadata": { "tag": "mix-export", "project": "夏日之歌" },
            "is_active": true
          }
        }
      ]
    },
    {
      "name": "get_link_stats",
      "description": "获取链接的访问统计数据，包含PV/UV/地域分布等",
      "category": "search",
      "endpoint": "GET /api/v1/stats/links/{id}",
      "input": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string",
            "description": "链接ID"
          }
        },
        "required": ["id"]
      },
      "output": {
        "type": "object",
        "properties": {
          "link_id": { "type": "string" },
          "total_clicks": { "type": "integer" },
          "unique_visitors": { "type": "integer" },
          "top_regions": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "region": { "type": "string" },
                "count": { "type": "integer" }
              }
            }
          },
          "daily_clicks": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "date": { "type": "string" },
                "clicks": { "type": "integer" }
              }
            }
          }
        }
      },
      "examples": [
        {
          "input": { "id": "link-abc123" },
          "output": {
            "link_id": "link-abc123",
            "total_clicks": 256,
            "unique_visitors": 189,
            "top_regions": [
              { "region": "CN", "count": 180 },
              { "region": "US", "count": 42 }
            ],
            "daily_clicks": [
              { "date": "2024-06-15", "clicks": 32 },
              { "date": "2024-06-14", "clicks": 28 }
            ]
          }
        }
      ]
    },
    {
      "name": "create_identity_card",
      "description": "创建Agent Identity Card名片，支持多主题渲染（dark/light/minimal/gradient）",
      "category": "create",
      "endpoint": "POST /api/v1/cards",
      "input": {
        "type": "object",
        "properties": {
          "name": {
            "type": "string",
            "description": "名片显示名称"
          },
          "title": {
            "type": "string",
            "description": "职位/角色标题"
          },
          "bio": {
            "type": "string",
            "description": "个人简介"
          },
          "avatar_url": {
            "type": "string",
            "description": "头像URL"
          },
          "tags": {
            "type": "array",
            "items": { "type": "string" },
            "description": "技能/标签列表"
          },
          "social_links": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "platform": { "type": "string" },
                "url": { "type": "string" }
              }
            },
            "description": "社交链接列表"
          },
          "theme": {
            "type": "string",
            "enum": ["dark", "light", "minimal", "gradient"],
            "description": "名片主题，默认dark"
          },
          "link_code": {
            "type": "string",
            "description": "关联的短链code，点击名片跳转"
          }
        },
        "required": ["name"]
      },
      "output": {
        "type": "object",
        "properties": {
          "id": { "type": "string" },
          "code": { "type": "string", "description": "名片唯一code" },
          "card_url": { "type": "string", "description": "名片渲染地址 /card/{code}" },
          "qr_url": { "type": "string", "description": "名片二维码地址 /card/{code}/qr" },
          "name": { "type": "string" },
          "theme": { "type": "string" },
          "created_at": { "type": "string" }
        }
      },
      "examples": [
        {
          "input": {
            "name": "OpenDAW",
            "title": "AI混音工程师",
            "bio": "AI原生的数字音频工作站，Rust驱动",
            "tags": ["混音", "母带", "音频分析", "AI"],
            "social_links": [
              { "platform": "github", "url": "https://github.com/youbanzhishi/OpenDAW" }
            ],
            "theme": "gradient",
            "link_code": "x7K1"
          },
          "output": {
            "id": "card-def456",
            "code": "opendaw",
            "card_url": "http://localhost:3000/card/opendaw",
            "qr_url": "http://localhost:3000/card/opendaw/qr",
            "name": "OpenDAW",
            "theme": "gradient",
            "created_at": "2024-06-15T10:30:00Z"
          }
        }
      ]
    },
    {
      "name": "resolve_context",
      "description": "解析访问者的Context上下文——根据短链code和访问者信息返回路由决策结果",
      "category": "search",
      "endpoint": "GET /api/v1/resolve/{code}",
      "input": {
        "type": "object",
        "properties": {
          "code": {
            "type": "string",
            "description": "短链code"
          }
        },
        "required": ["code"]
      },
      "output": {
        "type": "object",
        "properties": {
          "code": { "type": "string" },
          "link_id": { "type": "string" },
          "target": { "type": "string", "description": "最终目标URL" },
          "action": { "type": "string", "description": "路由动作类型，如redirect" },
          "metadata": { "type": "object" },
          "found": { "type": "boolean" }
        }
      },
      "examples": [
        {
          "input": { "code": "x7K1" },
          "output": {
            "code": "x7K1",
            "link_id": "link-abc123",
            "target": "https://cdn.example.com/mixes/summer-song.wav",
            "action": "redirect",
            "metadata": { "tag": "mix-export" },
            "found": true
          }
        }
      ]
    },
    {
      "name": "publish",
      "description": "发布内容——创建短链+名片+路由规则的一体化发布流程，适用于知识分发、作品展示等场景",
      "category": "execute",
      "endpoint": "POST /api/v1/links",
      "input": {
        "type": "object",
        "properties": {
          "target": {
            "type": "string",
            "description": "目标内容URL"
          },
          "code": {
            "type": "string",
            "description": "自定义短码"
          },
          "metadata": {
            "type": "object",
            "description": "内容元数据，包含title、description、cover_image等"
          },
          "create_card": {
            "type": "boolean",
            "description": "是否同时创建Identity Card名片，默认false"
          },
          "card_config": {
            "type": "object",
            "description": "名片配置（当create_card为true时生效）",
            "properties": {
              "name": { "type": "string" },
              "title": { "type": "string" },
              "bio": { "type": "string" },
              "theme": { "type": "string" }
            }
          },
          "owner": {
            "type": "string",
            "description": "发布者标识"
          }
        },
        "required": ["target"]
      },
      "output": {
        "type": "object",
        "properties": {
          "link": {
            "type": "object",
            "properties": {
              "id": { "type": "string" },
              "code": { "type": "string" },
              "target": { "type": "string" },
              "short_url": { "type": "string", "description": "完整短链地址" }
            }
          },
          "card": {
            "type": "object",
            "description": "名片信息（仅当create_card为true时返回）",
            "properties": {
              "id": { "type": "string" },
              "code": { "type": "string" },
              "card_url": { "type": "string" }
            }
          }
        }
      },
      "examples": [
        {
          "input": {
            "target": "https://knowledge.example.com/article/mixing-tips",
            "metadata": { "title": "混音实战心得", "description": "5年混音经验总结" },
            "create_card": true,
            "card_config": {
              "name": "混音笔记",
              "title": "知识卡片",
              "theme": "minimal"
            },
            "owner": "openmind"
          },
          "output": {
            "link": {
              "id": "link-ghi789",
              "code": "m1x7p",
              "target": "https://knowledge.example.com/article/mixing-tips",
              "short_url": "http://localhost:3000/m1x7p"
            },
            "card": {
              "id": "card-jkl012",
              "code": "mixing-notes",
              "card_url": "http://localhost:3000/card/mixing-notes"
            }
          }
        }
      ]
    }
  ],
  "workflows": [
    {
      "name": "mix_and_publish",
      "description": "混音发布流：OpenMind找待办→OpenVault取音轨→OpenDAW混音导出→OpenLink发布",
      "steps": [
        { "project": "openmind", "action": "find_todos" },
        { "project": "openvault", "action": "retrieve" },
        { "project": "opendaw", "action": "open_project" },
        { "project": "opendaw", "action": "ai_mix" },
        { "project": "opendaw", "action": "export" },
        { "project": "openlink", "action": "create_link" }
      ]
    },
    {
      "name": "content_publish",
      "description": "内容发布流：OpenMind搜索知识→OpenLink发布名片+链接",
      "steps": [
        { "project": "openmind", "action": "search" },
        { "project": "openlink", "action": "publish" }
      ]
    }
  ],
  "events": {
    "subscribe": "POST /api/v1/events/subscribe",
    "types": ["link.created", "link.accessed", "link.deleted", "card.created", "route.matched", "extension.registered"]
  },
  "links": {
    "docs": "https://github.com/youbanzhishi/OpenLink/docs",
    "source": "https://github.com/youbanzhishi/OpenLink",
    "health": "http://localhost:3000/health"
  }
}
```

---

*OpenLink v1.0.1 — Agent 接入指南*
