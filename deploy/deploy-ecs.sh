#!/bin/bash
# OpenLink ECS 一键部署脚本
# 前置条件：GitHub Release 已有 Linux 二进制，DNS 已配好
# 用法：sudo bash deploy-ecs.sh

set -euo pipefail

INSTALL_DIR="/opt/openlink"
BIN_DIR="${INSTALL_DIR}/bin"
CONFIG_DIR="${INSTALL_DIR}/config"
DATA_DIR="${INSTALL_DIR}/data"
KNOWLEDGE_DIR="${INSTALL_DIR}/knowledge"
REPO_URL="https://github.com/youbanzhishi/open-knowledge-system.git"
SERVICE_FILE="openlink.service"
NGINX_CONF="openlink.conf"

echo "=== OpenLink ECS 部署开始 ==="

# 1. 创建目录和用户
echo "[1/7] 创建目录和用户..."
id -u openlink &>/dev/null || useradd -r -s /bin/false openlink
mkdir -p "${BIN_DIR}" "${CONFIG_DIR}" "${DATA_DIR}" "${KNOWLEDGE_DIR}"
chown -R openlink:openlink "${INSTALL_DIR}"

# 2. 下载二进制（从 GitHub Release）
echo "[2/7] 下载二进制..."
echo "请从 https://github.com/youbanzhishi/OpenLink/releases 下载最新 Linux 二进制"
echo "放入 ${BIN_DIR}/openlink-api 后按回车继续"
read -r
chmod +x "${BIN_DIR}/openlink-api"

# 3. Clone 知识体系仓库
echo "[3/7] Clone 知识体系仓库..."
if [ ! -d "${KNOWLEDGE_DIR}/open-knowledge-system/.git" ]; then
    git clone --depth 1 "${REPO_URL}" "${KNOWLEDGE_DIR}/open-knowledge-system"
fi
chown -R openlink:openlink "${KNOWLEDGE_DIR}"

# 4. 配置
echo "[4/7] 写入配置..."
if [ ! -f "${CONFIG_DIR}/production.toml" ]; then
    cat > "${CONFIG_DIR}/production.toml" << 'TOML'
[server]
host = "0.0.0.0"
port = 3000

[database]
url = "sqlite:///opt/openlink/data/openlink.db"

[knowledge]
enabled = true
repo_path = "/opt/openlink/knowledge/open-knowledge-system"
base_url = "https://link.opendev.dev"
invite_codes = ["openclaw-2026", "welcome-agent", "knowledge-join"]
sync_token = "CHANGE_ME_REPLACE_WITH_STRONG_TOKEN"
TOML
    echo "⚠️  请编辑 ${CONFIG_DIR}/production.toml 替换 sync_token！"
fi

# 5. systemd 服务
echo "[5/7] 安装 systemd 服务..."
cp "${SERVICE_FILE}" /etc/systemd/system/
systemctl daemon-reload
systemctl enable openlink

# 6. Nginx
echo "[6/7] 配置 Nginx..."
cp "${NGINX_CONF}" /etc/nginx/sites-available/openlink
ln -sf /etc/nginx/sites-available/openlink /etc/nginx/sites-enabled/
nginx -t && systemctl reload nginx

# 7. 启动
echo "[7/7] 启动 OpenLink..."
systemctl start openlink
sleep 2
systemctl status openlink --no-pager

echo ""
echo "=== 部署完成 ==="
echo "短链入口：https://link.opendev.dev/join?code=openclaw-2026"
echo "同步端点：curl -X POST -H 'Authorization: Bearer YOUR_TOKEN' https://link.opendev.dev/api/v1/knowledge/sync"
echo ""
echo "后续步骤："
echo "  1. 编辑 ${CONFIG_DIR}/production.toml 替换 sync_token"
echo "  2. certbot --nginx -d link.opendev.dev  配置HTTPS"
echo "  3. systemctl restart openlink"
