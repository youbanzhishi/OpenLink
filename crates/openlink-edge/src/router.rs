//! # 边缘路由器（Phase 5 增强）
//!
//! 精简版路由器，处理短链重定向。
//! Phase 5 增强：集成 WASM 重定向引擎和地理路由。

use crate::config::EdgeConfig;
use crate::cache::{EdgeCache, CacheEntry};
use crate::geo::GeoRouter;
use crate::wasm_redirect::{EdgeRedirectEngine, EdgeRequest, EdgeRedirectRule};
use std::sync::Arc;
use std::io::Cursor;
use tiny_http::{Response, Header, StatusCode};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// 路由目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteTarget {
    pub code: String,
    pub url: String,
    pub status_code: u16,
}

/// 边缘路由器（Phase 5 增强版）
pub struct EdgeRouter {
    config: EdgeConfig,
    cache: EdgeCache,
    /// 内存中的路由表（简化版，无数据库）
    routes: Arc<RwLock<std::collections::HashMap<String, RouteTarget>>>,
    /// WASM 重定向引擎
    redirect_engine: Arc<RwLock<EdgeRedirectEngine>>,
    /// 地理路由
    geo_router: Arc<RwLock<GeoRouter>>,
}

impl EdgeRouter {
    /// 创建新路由器
    pub fn new(config: EdgeConfig) -> Self {
        let cache = EdgeCache::new(config.cache.max_entries, config.cache.ttl_secs);
        let geo_router = GeoRouter::new(config.geo_route.clone().unwrap_or_default());
        Self {
            config,
            cache,
            routes: Arc::new(RwLock::new(std::collections::HashMap::new())),
            redirect_engine: Arc::new(RwLock::new(EdgeRedirectEngine::new())),
            geo_router: Arc::new(RwLock::new(geo_router)),
        }
    }

    /// 注册路由
    pub async fn register_route(&self, code: String, url: String, status_code: u16) {
        let mut routes = self.routes.write().await;
        routes.insert(code.clone(), RouteTarget {
            code: code.clone(),
            url: url.clone(),
            status_code,
        });

        // 预热缓存
        self.cache.put(code, url, status_code).await;
    }

    /// 注册重定向规则（到 WASM 引擎）
    pub async fn register_redirect_rule(&self, rule: EdgeRedirectRule) {
        let mut engine = self.redirect_engine.write().await;
        engine.add_rule(rule);
    }

    /// 注册热链
    pub async fn register_hot_link(&self, code: String, url: String) {
        let mut engine = self.redirect_engine.write().await;
        engine.register_hot_link(code, url);
    }

    /// 解析短链（Phase 5：三层查找）
    ///
    /// 查找顺序：
    /// 1. WASM 重定向引擎（含热链快速路径）
    /// 2. 缓存
    /// 3. 路由表
    pub async fn resolve(&self, code: &str, client_ip: Option<&str>, user_agent: Option<&str>) -> Option<RedirectResult> {
        // 1. WASM 重定向引擎
        let request = EdgeRequest {
            code: code.to_string(),
            client_ip: client_ip.map(|s| s.to_string()),
            user_agent: user_agent.map(|s| s.to_string()),
            device_type: user_agent.and_then(|ua| detect_device_type(ua)),
            identity_type: user_agent.and_then(|ua| detect_identity_type(ua)),
            geo_region: {
                if let Some(ip) = client_ip {
                    let router = self.geo_router.read().await;
                    Some(router.resolve(ip).region.clone())
                } else {
                    None
                }
            },
            headers: std::collections::HashMap::new(),
        };

        {
            let engine = self.redirect_engine.read().await;
            if let Some(decision) = engine.resolve(&request) {
                return Some(RedirectResult {
                    target_url: decision.target_url,
                    status_code: decision.status_code,
                    source: if decision.cache_hit { "hot_link" } else { "wasm_rule" },
                });
            }
        }

        // 2. 缓存
        if let Some(entry) = self.cache.get(code).await {
            return Some(RedirectResult {
                target_url: entry.target_url,
                status_code: entry.status_code,
                source: "cache",
            });
        }

        // 3. 路由表
        let url = {
            let routes = self.routes.read().await;
            routes.get(code).map(|t| (t.url.clone(), t.status_code))
        };

        if let Some((target_url, status_code)) = url {
            // 回填缓存
            self.cache.put(code.to_string(), target_url.clone(), status_code).await;

            return Some(RedirectResult {
                target_url,
                status_code,
                source: "route_table",
            });
        }

        None
    }

    /// 获取地理路由推荐的节点信息
    pub async fn geo_resolve(&self, client_ip: &str) -> crate::geo::NodeEndpoint {
        let router = self.geo_router.read().await;
        router.resolve(client_ip).clone()
    }

    /// 获取缓存统计
    pub async fn cache_stats(&self) -> crate::cache::CacheStats {
        self.cache.stats().await
    }

    /// 主动失效缓存
    pub async fn invalidate_cache(&self, code: &str) -> bool {
        self.cache.invalidate(code).await
    }

    /// 缓存预热
    pub async fn warmup_cache(&self, entries: Vec<(String, String, u16)>) {
        self.cache.warmup(entries).await;
    }

    /// 构建重定向响应（保留兼容）
    pub fn build_redirect_response(&self, target_url: &str, status_code: u16) -> Response<Cursor<Vec<u8>>> {
        let status = if status_code == 301 {
            StatusCode(301)
        } else {
            StatusCode(302)
        };

        let header = Header::from_bytes(&b"Location"[..], target_url.as_bytes()).unwrap();

        Response::from_data(target_url.as_bytes().to_vec())
            .with_status_code(status)
            .with_header(header)
    }
}

/// 重定向结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedirectResult {
    pub target_url: String,
    pub status_code: u16,
    /// 结果来源：hot_link / wasm_rule / cache / route_table
    pub source: &'static str,
}

/// 从 User-Agent 检测身份类型（边缘简化版）
fn detect_identity_type(ua: &str) -> Option<String> {
    let ua_lower = ua.to_lowercase();
    if ua_lower.contains("curl/") || ua_lower.contains("wget/") || ua_lower.contains("python-requests/") {
        Some("service".to_string())
    } else if ua_lower.contains("openai") || ua_lower.contains("anthropic") || ua_lower.contains("claude") || ua_lower.contains("agent") {
        Some("agent".to_string())
    } else {
        Some("human".to_string())
    }
}

/// 从 User-Agent 检测设备类型（边缘简化版）
fn detect_device_type(ua: &str) -> Option<String> {
    let ua_lower = ua.to_lowercase();
    if ua_lower.contains("mobile") || ua_lower.contains("android") || ua_lower.contains("iphone") {
        Some("mobile".to_string())
    } else if ua_lower.contains("curl/") || ua_lower.contains("wget/") {
        Some("server".to_string())
    } else {
        Some("desktop".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_resolve() {
        let config = EdgeConfig::default_config();
        let router = EdgeRouter::new(config);

        router.register_route("test".to_string(), "https://example.com".to_string(), 302).await;

        let result = router.resolve("test", None, None).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().target_url, "https://example.com");
    }

    #[tokio::test]
    async fn test_resolve_not_found() {
        let config = EdgeConfig::default_config();
        let router = EdgeRouter::new(config);

        let result = router.resolve("nonexistent", None, None).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_wasm_redirect_rule() {
        let config = EdgeConfig::default_config();
        let router = EdgeRouter::new(config);

        let rule = EdgeRedirectRule {
            id: "r1".to_string(),
            code: "wasm-test".to_string(),
            condition: None,
            target_url: "https://wasm.example.com".to_string(),
            status_code: 301,
            priority: 10,
        };
        router.register_redirect_rule(rule).await;

        let result = router.resolve("wasm-test", None, None).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.target_url, "https://wasm.example.com");
        assert_eq!(r.status_code, 301);
        assert_eq!(r.source, "wasm_rule");
    }

    #[tokio::test]
    async fn test_hot_link() {
        let config = EdgeConfig::default_config();
        let router = EdgeRouter::new(config);

        router.register_hot_link("hot1".to_string(), "https://hot.example.com".to_string()).await;

        let result = router.resolve("hot1", None, None).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().source, "hot_link");
    }

    #[tokio::test]
    async fn test_geo_resolve() {
        let config = EdgeConfig::default_config();
        let router = EdgeRouter::new(config);

        let node = router.geo_resolve("10.0.1.100").await;
        assert_eq!(node.region, "cn-east");
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let config = EdgeConfig::default_config();
        let router = EdgeRouter::new(config);

        router.register_route("del1".to_string(), "https://example.com".to_string(), 302).await;

        // 先查一次让它进缓存
        let _ = router.resolve("del1", None, None).await;

        // 失效缓存
        assert!(router.invalidate_cache("del1").await);

        // 路由表仍在，所以还能查到
        let result = router.resolve("del1", None, None).await;
        assert!(result.is_some());
    }
}
