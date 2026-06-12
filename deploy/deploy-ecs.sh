#!/bin/bash
# OpenLink ECS 一键部署脚本（多源版 v1.3.0+）
# 前置条件：GitHub Release 已有 Linux musl 二进制，DNS 已配好
# 用法：sudo bash deploy-ecs.sh

set -euo pipefail

INSTALL_DIR="/opt/openlink"
BIN_DIR="${INSTALL_DIR}/bin"
CONFIG_DIR="${INSTALL_DIR}/config"
DATA_DIR="${INSTALL_DIR}/data"
KNOWLEDGE_DIR="${INSTALL_DIR}/knowledge"
LOG_DIR="${INSTALL_DIR}/log"

# 仓库地址
PRIVATE_REPO="https://github.com/youbanzhishi/open-knowledge-system.git"
PUBLIC_REPO="https://github.com/youbanzhishi/open-knowledge-framework.git"

# Release 二进制（musl 静态编译，零 glibc 依赖）
RELEASE_VERSION="v1.3.0"
MUSL_BINARY="openlink-linux-amd64-musl.tar.gz"
RELEASE_URL="https://github.com/youbanzhishi/OpenLink/releases/download/${RELEASE_VERSION}/${MUSL_BINARY}"

echo "=== OpenLink ECS 部署开始 (v1.3.0 多源版) ==="

# 1. 创建目录和用户
echo "[1/8] 创建目录和用户..."
id -u openlink &>/dev/null || useradd -r -s /bin/false openlink
mkdir -p "${BIN_DIR}" "${CONFIG_DIR}" "${DATA_DIR}" "${KNOWLEDGE_DIR}" "${LOG_DIR}"
chown -R openlink:openlink "${INSTALL_DIR}"

# 2. 下载二进制（从 GitHub Release）
echo "[2/8] 下载二进制 (musl 静态编译)..."
if [ ! -f "${BIN_DIR}/openlink-api" ]; then
    echo "  从 GitHub Release 下载 ${MUSL_BINARY}..."
    curl -L -o /tmp/${MUSL_BINARY} "${RELEASE_URL}"
    tar xzf /tmp/${MUSL_BINARY} -C "${BIN_DIR}/"
    rm -f /tmp/${MUSL_BINARY}
    chmod +x "${BIN_DIR}/openlink-api"
    echo "  二进制已安装: ${BIN_DIR}/openlink-api"
else
    echo "  二进制已存在，跳过下载"
fi

# 3. Clone 私有知识体系仓库
echo "[3/8] Clone 私有知识体系仓库..."
if [ ! -d "${KNOWLEDGE_DIR}/open-knowledge-system/.git" ]; then
    git clone --depth 1 "${PRIVATE_REPO}" "${KNOWLEDGE_DIR}/open-knowledge-system"
    echo "  私有仓库已 clone"
else
    echo "  私有仓库已存在，跳过"
fi

# 4. Clone 公开知识框架仓库
echo "[4/8] Clone 公开知识框架仓库..."
if [ ! -d "${KNOWLEDGE_DIR}/open-knowledge-framework/.git" ]; then
    git clone --depth 1 "${PUBLIC_REPO}" "${KNOWLEDGE_DIR}/open-knowledge-framework"
    echo "  公开仓库已 clone"
else
    echo "  公开仓库已存在，跳过"
fi
chown -R openlink:openlink "${KNOWLEDGE_DIR}"

# 5. 配置
echo "[5/8] 写入配置..."
if [ ! -f "${CONFIG_DIR}/production.toml" ]; then
    cp production.toml "${CONFIG_DIR}/production.toml"
    echo "⚠️  请编辑 ${CONFIG_DIR}/production.toml 替换 sync_token！"
else
    echo "  配置已存在，跳过（如需更新请手动替换）"
fi

# 6. systemd 服务
echo "[6/8] 安装 systemd 服务..."
cp openlink.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable openlink

# 7. Nginx
echo "[7/8] 配置 Nginx..."
if [ ! -f /etc/nginx/sites-available/openlink ]; then
    cp openlink.conf /etc/nginx/sites-available/openlink
    ln -sf /etc/nginx/sites-available/openlink /etc/nginx/sites-enabled/
    nginx -t && systemctl reload nginx
    echo "  Nginx 配置已安装"
else
    echo "  Nginx 配置已存在，跳过"
fi

# 8. 启动
echo "[8/8] 启动 OpenLink..."
systemctl start openlink
sleep 2
systemctl status openlink --no-pager

echo ""
echo "=== 部署完成 ==="
echo ""
echo "短链入口："
echo "  私有源: https://link.opendev.dev/join?code=openclaw-2026"
echo "  公开源: https://link.opendev.dev/join?code=openclaw-framework"
echo ""
echo "同步端点（push.sh 推送后自动触发）："
echo "  私有源: curl -X POST -H 'Authorization: Bearer YOUR_TOKEN' https://link.opendev.dev/api/v1/knowledge/private/sync"
echo ""
echo "后续步骤："
echo "  1. 编辑 ${CONFIG_DIR}/production.toml 替换 sync_token"
echo "  2. certbot --nginx -d link.opendev.dev  配置HTTPS"
echo "  3. systemctl restart openlink"
echo "  4. 配 DNS: link.opendev.dev → 本机 IP"
