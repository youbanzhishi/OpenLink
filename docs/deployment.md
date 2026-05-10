# OpenLink 部署指南

本指南涵盖 Docker、Kubernetes 部署，以及配置、监控和故障排查。

## Docker 部署

### 开发环境

```bash
cd docker
docker-compose up -d
```

单容器部署，使用 SQLite 数据库。

### 生产环境

```bash
cd deploy
docker-compose -f docker-compose.prod.yml up -d
```

生产部署包含：

| 服务 | 说明 |
|------|------|
| openlink-api | API 服务（2副本） |
| postgres | PostgreSQL 16 数据库 |
| redis | Redis 7 缓存（AOF 持久化） |
| nginx | 反向代理（SSL + 限流 + WebSocket） |
| prometheus | 指标采集 |
| grafana | 可视化仪表盘 |

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| DATABASE_URL | sqlite:openlink.db | 数据库连接串 |
| REDIS_URL | redis://127.0.0.1:6379 | Redis 连接串 |
| OPENLINK_HOST | 0.0.0.0 | 监听地址 |
| OPENLINK_PORT | 3000 | 监听端口 |
| RUST_LOG | openlink=info | 日志级别 |
| DB_PASSWORD | openlink_secure_2024 | PostgreSQL 密码 |
| GRAFANA_PASSWORD | admin | Grafana 管理员密码 |

### SSL 配置

1. 将证书文件放在 `deploy/nginx/ssl/` 目录：
   - `fullchain.pem` — 完整证书链
   - `privkey.pem` — 私钥

2. 使用 Let's Encrypt 自动获取：

```bash
# 安装 certbot
apt-get install certbot

# 获取证书
certbot certonly --standalone -d your-domain.com

# 复制到部署目录
cp /etc/letsencrypt/live/your-domain.com/fullchain.pem deploy/nginx/ssl/
cp /etc/letsencrypt/live/your-domain.com/privkey.pem deploy/nginx/ssl/
```

## Kubernetes 部署

### Deployment YAML 示例

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: openlink-api
  labels:
    app: openlink
spec:
  replicas: 3
  selector:
    matchLabels:
      app: openlink
  template:
    metadata:
      labels:
        app: openlink
    spec:
      containers:
        - name: openlink-api
          image: ghcr.io/your-org/openlink:1.0.0
          ports:
            - containerPort: 3000
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: openlink-secrets
                  key: database-url
            - name: REDIS_URL
              valueFrom:
                secretKeyRef:
                  name: openlink-secrets
                  key: redis-url
          resources:
            requests:
              memory: "128Mi"
              cpu: "100m"
            limits:
              memory: "512Mi"
              cpu: "500m"
          livenessProbe:
            httpGet:
              path: /health
              port: 3000
            initialDelaySeconds: 30
            periodSeconds: 15
          readinessProbe:
            httpGet:
              path: /health
              port: 3000
            initialDelaySeconds: 10
            periodSeconds: 5
---
apiVersion: v1
kind: Service
metadata:
  name: openlink-service
spec:
  selector:
    app: openlink
  ports:
    - port: 80
      targetPort: 3000
  type: ClusterIP
```

### PostgreSQL StatefulSet

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres
spec:
  serviceName: postgres
  replicas: 1
  selector:
    matchLabels:
      app: postgres
  template:
    metadata:
      labels:
        app: postgres
    spec:
      containers:
        - name: postgres
          image: postgres:16-alpine
          env:
            - name: POSTGRES_DB
              value: openlink
            - name: POSTGRES_USER
              value: openlink
            - name: POSTGRES_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: openlink-secrets
                  key: db-password
          volumeMounts:
            - name: postgres-data
              mountPath: /var/lib/postgresql/data
  volumeClaimTemplates:
    - metadata:
        name: postgres-data
      spec:
        accessModes: ["ReadWriteOnce"]
        resources:
          requests:
            storage: 10Gi
```

## 监控配置

### Prometheus

Prometheus 采集配置位于 `deploy/monitoring/prometheus.yml`，默认采集：

- OpenLink API 指标（`/metrics`）
- Redis 指标（通过 redis_exporter）
- PostgreSQL 指标（通过 postgres_exporter）
- Nginx 指标（通过 nginx-prometheus-exporter）

### Grafana

1. 访问 `http://your-server:3001`
2. 添加 Prometheus 数据源：`http://prometheus:9090`
3. 导入仪表盘：使用 `deploy/monitoring/grafana/openlink-dashboard.json`

仪表盘包含：
- API 请求速率 (QPS)
- API 延迟 (P50/P95/P99)
- 错误率
- 缓存命中率
- 活跃连接数
- Redis 内存使用

### Nginx 限流

生产环境 Nginx 配置了三级限流：

| 区域 | 速率 | 适用 |
|------|------|------|
| general | 10 req/s | 一般请求 |
| api | 30 req/s | API 请求 |
| auth | 5 req/min | 认证请求 |

## 故障排查

### 常见问题

#### API 服务启动失败

```bash
# 检查日志
docker logs openlink-api-1

# 常见原因：
# 1. 数据库连接失败 — 检查 DATABASE_URL
# 2. Redis 连接失败 — 检查 REDIS_URL
# 3. 端口冲突 — 检查 OPENLINK_PORT
```

#### 数据库连接超时

```bash
# 检查 PostgreSQL 状态
docker exec openlink-postgres pg_isready -U openlink

# 检查连接数
docker exec openlink-postgres psql -U openlink -c "SELECT count(*) FROM pg_stat_activity;"
```

#### Redis 内存不足

```bash
# 查看 Redis 信息
docker exec openlink-redis redis-cli info memory

# 清理策略已在配置中设置为 allkeys-lru
# 最大内存: 256MB (可在 docker-compose.prod.yml 中调整)
```

#### Nginx 502 错误

```bash
# 检查后端 API 是否健康
docker exec openlink-nginx curl -sf http://openlink-api:3000/health

# 检查 Nginx 配置
docker exec openlink-nginx nginx -t
```

### 性能调优

#### API 层

- 增加副本数（`deploy.replicas`）
- 调整 tokio 工作线程数（`TOKIO_WORKER_THREADS`）
- 启用连接池复用

#### 数据库

- 添加索引（target, owner, created_at）
- 启用连接池（`SQLX_MAX_CONNECTIONS`）
- 配置 pg_stat_statements 监控慢查询

#### 缓存

- 调整 Redis 最大内存
- 优化缓存 TTL
- 监控缓存命中率
