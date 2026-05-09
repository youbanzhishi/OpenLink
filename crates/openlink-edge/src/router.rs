//! # 边缘路由器
//!
//! 精简版路由器，处理短链重定向。

use crate::config::EdgeConfig;
use crate::cache::{EdgeCache, CacheEntry};
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

/// 边缘路由器
pub struct EdgeRouter {
    config: EdgeConfig,
    cache: EdgeCache,
    /// 内存中的路由表（简化版，无数据库）
    routes: Arc<RwLock<std::collections::HashMap<String, RouteTarget>>>,
}

impl EdgeRouter {
    /// 创建新路由器
    pub fn new(config: EdgeConfig) -> Self {
        let cache = EdgeCache::new(config.cache.max_entries, config.cache.ttl_secs);
        Self {
            config,
            cache,
            routes: Arc::new(RwLock::new(std::collections::HashMap::new())),
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
    
    /// 解析短链
    pub async fn resolve(&self, code: &str) -> Option<Response<Cursor<Vec<u8>>>> {
        // 先查缓存
        if let Some(entry) = self.cache.get(code).await {
            return Some(self.build_redirect_response(entry));
        }
        
        // 查路由表
        let url = {
            let routes = self.routes.read().await;
            routes.get(code).map(|t| (t.url.clone(), t.status_code))
        };
        
        if let Some((target_url, status_code)) = url {
            // 回填缓存
            self.cache.put(code.to_string(), target_url.clone(), status_code).await;
            
            return Some(self.build_redirect_response(CacheEntry {
                target_url,
                status_code,
                created_at: chrono::Utc::now().timestamp(),
                access_count: 0,
            }));
        }
        
        None
    }
    
    /// 构建重定向响应
    fn build_redirect_response(&self, entry: CacheEntry) -> Response<Cursor<Vec<u8>>> {
        let status = if entry.status_code == 301 {
            StatusCode(301)
        } else {
            StatusCode(302)
        };
        
        let header = Header::from_bytes(&b"Location"[..], &entry.target_url[..]).unwrap();
        
        Response::from_data(entry.target_url)
            .with_status_code(status)
            .with_header(header)
    }
    
    /// 获取缓存统计
    pub async fn cache_stats(&self) -> crate::cache::CacheStats {
        self.cache.stats().await
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
        
        let response = router.resolve("test").await;
        assert!(response.is_some());
    }
    
    #[tokio::test]
    async fn test_resolve_not_found() {
        let config = EdgeConfig::default_config();
        let router = EdgeRouter::new(config);
        
        let response = router.resolve("nonexistent").await;
        assert!(response.is_none());
    }
}
