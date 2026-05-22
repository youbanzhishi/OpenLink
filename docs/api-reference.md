# OpenLink API 参考文档

完整 REST API 参考，包含所有端点、请求/响应示例和错误码。

## 基础信息

- **Base URL**: `http://localhost:3000`
- **认证方式**: Bearer Token (Header: `Authorization: Bearer <api_key>`)
- **内容类型**: `application/json`

---

## 链接管理

### 创建短链

```
POST /api/v1/links
```

**请求体**:

```json
{
  "target": "https://example.com/long-url",
  "code": "custom-code",
  "metadata": { "tag": "marketing" },
  "is_active": true,
  "owner": "agent-1"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| target | string | ✓ | 目标 URL |
| code | string | | 自定义短码（不填自动生成） |
| metadata | object | | 附加元数据 |
| is_active | boolean | | 是否启用（默认 true） |
| owner | string | | 所有者标识 |

**响应** (`201 Created`):

```json
{
  "id": "link-abc123",
  "code": "custom-code",
  "target": "https://example.com/long-url",
  "owner": "agent-1",
  "created_at": "2024-01-15T10:30:00Z",
  "metadata": { "tag": "marketing" },
  "is_active": true
}
```

### 获取链接

```
GET /api/v1/links/{code}
```

**响应** (`200 OK`):

```json
{
  "id": "link-abc123",
  "code": "custom-code",
  "target": "https://example.com/long-url",
  "owner": "agent-1",
  "created_at": "2024-01-15T10:30:00Z",
  "metadata": {},
  "is_active": true
}
```

### 查询链接列表

```
GET /api/v1/links?owner={owner}&is_active={bool}&limit={n}&offset={n}
```

| 参数 | 类型 | 说明 |
|------|------|------|
| owner | string | 按所有者过滤 |
| is_active | boolean | 按状态过滤 |
| limit | integer | 每页数量（默认 20） |
| offset | integer | 偏移量 |

**响应** (`200 OK`):

```json
[
  {
    "id": "link-abc123",
    "code": "abc",
    "target": "https://example.com",
    "owner": "agent-1",
    "created_at": "2024-01-15T10:30:00Z",
    "metadata": {},
    "is_active": true
  }
]
```

### 删除链接

```
DELETE /api/v1/links/{code}
```

**响应**: `204 No Content`

### 解析短链

```
GET /api/v1/resolve/{code}
```

**响应** (`200 OK`):

```json
{
  "code": "abc",
  "link_id": "link-abc123",
  "target": "https://example.com",
  "action": "redirect",
  "metadata": {},
  "found": true
}
```

---

## 批量操作

### 批量创建

```
POST /api/v1/links/batch
```

**请求体**:

```json
{
  "links": [
    { "target": "https://example.com/1", "metadata": {}, "is_active": true },
    { "target": "https://example.com/2", "metadata": {}, "is_active": true }
  ]
}
```

**响应** (`200 OK`):

```json
{
  "results": [
    { "id": "1", "code": "x7K1", "target": "https://example.com/1", "owner": "default", "created_at": "2024-01-15T10:30:00Z", "metadata": {}, "is_active": true }
  ],
  "succeeded": 2,
  "failed": 0
}
```

### 批量解析

```
POST /api/v1/agent/resolve
```

**请求体**:

```json
{
  "codes": ["abc", "def", "ghi"]
}
```

### 批量删除

```
POST /api/v1/links/batch-delete
```

**请求体**:

```json
{
  "codes": ["abc", "def"]
}
```

---

## 路由规则

### 创建路由规则

```
POST /api/v1/routes
```

**请求体**:

```json
{
  "rules": [
    {
      "condition": { "type": "geo", "params": { "value": "CN" } },
      "target": { "action": "url", "params": { "url": "https://cn.example.com" } },
      "priority": 10
    }
  ],
  "default_target": {
    "action": "url",
    "params": { "link_id": "link-abc123", "url": "https://example.com" }
  }
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| rules | array | | 路由规则列表 |
| rules[].condition | object | | 单条件（Phase 1 向后兼容） |
| rules[].condition.type | string | ✓ | 条件类型：geo/device/time/language/always |
| rules[].condition.params | object | | 条件参数 |
| rules[].conditions | array | | 多条件列表（Phase 2: AND/OR 组合） |
| rules[].condition_logic | string | | 条件组合逻辑："and"（默认）/"or" |
| rules[].target | object | ✓ | 匹配后的跳转目标 |
| rules[].target.action | string | ✓ | 动作类型：url/redirect/agent/card |
| rules[].target.params | object | | 动作参数 |
| rules[].priority | integer | | 优先级（数值越大越优先） |
| default_target | object | ✓ | 兜底目标（无规则匹配时执行） |
| default_target.action | string | ✓ | 动作类型 |
| default_target.params | object | ✓ | 必须包含 link_id（关联的短链ID） |

**注意**：`link_id` 通过 `default_target.params.link_id` 传入，而非顶层字段。

**响应** (`201 Created`):

```json
{
  "id": "route-uuid",
  "link_id": "link-abc123",
  "rules": [...],
  "default": { "action": "url", "params": { "url": "https://example.com" } },
  "version": 1
}
```

### 更新路由规则

```
PUT /api/v1/routes/{route_id}
```

**请求体**:

```json
{
  "rules": [...],
  "default_target": { "action": "url", "params": { "link_id": "link-abc123", "url": "..." } }
}
```

### 获取路由规则

```
GET /api/v1/routes/{route_id}
```

**响应** (`200 OK`):

```json
{
  "id": "route-uuid",
  "link_id": "link-abc123",
  "rules": [...],
  "default": { "action": "url", "params": {} },
  "version": 1
}
```

### 删除路由规则

```
DELETE /api/v1/routes/{route_id}
```

**响应**: `204 No Content`

---

## 文件传输

### 请求上传

```
POST /api/v1/files/upload
```

**请求体**:

```json
{
  "filename": "document.pdf",
  "size": 1048576,
  "content_type": "application/pdf",
  "storage": "r2",
  "generate_share_link": true,
  "share_link_ttl_secs": 604800
}
```

**响应** (`200 OK`):

```json
{
  "file_id": "file-xyz789",
  "upload_url": "https://storage.example.com/presigned-upload-url",
  "access_url": "https://cdn.example.com/files/xyz789",
  "share_code": "sh4r3",
  "expires_at": "2024-01-22T10:30:00Z"
}
```

### 下载文件

```
GET /api/v1/files/{file_id}/download
```

### 通过分享码获取

```
GET /api/v1/files/share/{share_code}
```

### 生成分享链接

```
POST /api/v1/files/share
```

---

## Agent API

### 发现链接

```
POST /api/v1/agent/discover
```

**请求体**:

```json
{
  "discover_type": "by_owner",
  "filters": { "owner": "agent-1" },
  "limit": 20
}
```

---

## 扩展系统

### 列出扩展

```
GET /api/v1/extensions
```

### 执行扩展动作

```
POST /api/v1/extensions/{name}/actions/{action}
```

---

## 插件管理

### 安装插件

```
POST /api/v1/plugins/install
```

### 列出插件

```
GET /api/v1/plugins
```

---

## 监控

### 健康检查

```
GET /health
```

**响应** (`200 OK`):

```json
{ "status": "ok" }
```

### Prometheus 指标

```
GET /metrics
```

返回 Prometheus 格式的指标数据。

---

## 错误码

| HTTP 状态码 | 错误类型 | 说明 |
|-------------|----------|------|
| 400 | Bad Request | 请求参数无效 |
| 401 | Unauthorized | 未提供认证信息或 Token 无效 |
| 403 | Forbidden | 无权访问该资源 |
| 404 | Not Found | 资源不存在 |
| 409 | Conflict | 资源冲突（如短码已存在） |
| 429 | Too Many Requests | 请求频率超限 |
| 500 | Internal Server Error | 服务器内部错误 |
| 502 | Bad Gateway | 上游服务错误 |
| 503 | Service Unavailable | 服务暂时不可用 |

### 错误响应格式

```json
{
  "error": {
    "code": "LINK_NOT_FOUND",
    "message": "Link with code 'abc' not found"
  }
}
```
