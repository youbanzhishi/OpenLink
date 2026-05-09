//! # ext-conditions — 条件路由扩展
//!
//! 提供 header-match 和 geo-match 条件处理器，
//! 作为 Extension 注册到 Registry，核心不改。
//!
//! 设计验证：新功能 = 注册扩展，架构本身永远不需要改。

use std::sync::Arc;
use async_trait::async_trait;
use openlink_core::{ConditionHandler, ExtensionRegistry, Context, CoreError};

// ─── HeaderMatch Condition ─────────────────────────────────

/// HTTP Header 匹配条件处理器
///
/// 检查请求的 HTTP Header 是否匹配指定模式。
/// 支持包含匹配和精确匹配两种模式。
///
/// 参数：
/// - `header`: Header 名称（如 "user-agent"）
/// - `pattern`: 匹配模式
/// - `mode`: 匹配模式，"contains"（默认）或 "exact"
struct HeaderMatchCondition;

#[async_trait]
impl ConditionHandler for HeaderMatchCondition {
    async fn evaluate(
        &self,
        ctx: &Context,
        params: &serde_json::Value,
    ) -> Result<bool, CoreError> {
        let header_name = params
            .get("header")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();

        let pattern = params
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let mode = params
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("contains")
            .to_lowercase();

        if header_name.is_empty() || pattern.is_empty() {
            return Ok(false);
        }

        // 从 Context headers 中查找
        let header_value = if ctx.headers.is_object() {
            ctx.headers
                .as_object()
                .unwrap()
                .iter()
                .find(|(k, _)| k.to_lowercase() == header_name)
                .and_then(|(_, v)| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };

        // 回退到 user_agent_raw
        let header_value = header_value.or_else(|| {
            if header_name == "user-agent" {
                ctx.device.user_agent_raw.clone()
            } else {
                None
            }
        });

        match header_value {
            Some(val) => {
                match mode.as_str() {
                    "exact" => Ok(val == pattern),
                    _ => Ok(val.to_lowercase().contains(&pattern.to_lowercase())),
                }
            }
            None => Ok(false),
        }
    }

    fn name(&self) -> &str {
        "header-match"
    }
}

// ─── GeoMatch Condition ────────────────────────────────────

/// 地理位置匹配条件处理器
///
/// 按国家/地区/城市匹配，预留接口可配置。
///
/// 参数：
/// - `country`: 国家代码（如 "CN", "US"）
/// - `region`: 地区（可选）
/// - `city`: 城市（可选）
struct GeoMatchCondition;

#[async_trait]
impl ConditionHandler for GeoMatchCondition {
    async fn evaluate(
        &self,
        ctx: &Context,
        params: &serde_json::Value,
    ) -> Result<bool, CoreError> {
        // 国家匹配
        if let Some(country) = params.get("country").and_then(|v| v.as_str()) {
            match &ctx.location.country {
                Some(ctx_country) if ctx_country.to_lowercase() == country.to_lowercase() => {
                    // 进一步匹配 region
                    if let Some(region) = params.get("region").and_then(|v| v.as_str()) {
                        return Ok(
                            ctx.location.region
                                .as_ref()
                                .map(|r| r.to_lowercase() == region.to_lowercase())
                                .unwrap_or(false)
                        );
                    }
                    return Ok(true);
                }
                _ => return Ok(false),
            }
        }

        // 城市匹配
        if let Some(city) = params.get("city").and_then(|v| v.as_str()) {
            return Ok(
                ctx.location.city
                    .as_ref()
                    .map(|c| c.to_lowercase() == city.to_lowercase())
                    .unwrap_or(false)
            );
        }

        Ok(false)
    }

    fn name(&self) -> &str {
        "geo-match"
    }
}

/// 注册所有条件扩展到 Extension Registry
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    registry.register_condition(Arc::new(HeaderMatchCondition))?;
    registry.register_condition(Arc::new(GeoMatchCondition))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlink_core::{Identity, IdentityType, DeviceInfo, GeoInfo};
    use std::collections::HashMap;

    fn make_ctx_with_headers(headers: HashMap<String, String>) -> Context {
        let mut ctx = Context::from_request_with_headers(None, None, &headers);
        ctx
    }

    #[tokio::test]
    async fn test_header_match_contains() {
        let handler = HeaderMatchCondition;
        let mut headers = HashMap::new();
        headers.insert("user-agent".to_string(), "curl/7.88.1".to_string());
        let ctx = make_ctx_with_headers(headers);

        let params = serde_json::json!({
            "header": "user-agent",
            "pattern": "curl"
        });
        assert!(handler.evaluate(&ctx, &params).await.unwrap());
    }

    #[tokio::test]
    async fn test_header_match_exact() {
        let handler = HeaderMatchCondition;
        let mut headers = HashMap::new();
        headers.insert("x-api-key".to_string(), "secret123".to_string());
        let ctx = make_ctx_with_headers(headers);

        let params = serde_json::json!({
            "header": "x-api-key",
            "pattern": "secret123",
            "mode": "exact"
        });
        assert!(handler.evaluate(&ctx, &params).await.unwrap());
    }

    #[tokio::test]
    async fn test_header_match_no_match() {
        let handler = HeaderMatchCondition;
        let mut headers = HashMap::new();
        headers.insert("user-agent".to_string(), "Mozilla/5.0".to_string());
        let ctx = make_ctx_with_headers(headers);

        let params = serde_json::json!({
            "header": "user-agent",
            "pattern": "curl"
        });
        assert!(!handler.evaluate(&ctx, &params).await.unwrap());
    }

    #[tokio::test]
    async fn test_header_match_missing_header() {
        let handler = HeaderMatchCondition;
        let ctx = Context::from_request(None, None);

        let params = serde_json::json!({
            "header": "x-custom",
            "pattern": "value"
        });
        assert!(!handler.evaluate(&ctx, &params).await.unwrap());
    }

    #[tokio::test]
    async fn test_geo_match_country() {
        let handler = GeoMatchCondition;
        let mut ctx = Context::from_request(None, None);
        ctx.location = GeoInfo {
            country: Some("CN".to_string()),
            region: Some("Beijing".to_string()),
            city: Some("Beijing".to_string()),
            lat: None,
            lon: None,
        };

        let params = serde_json::json!({"country": "CN"});
        assert!(handler.evaluate(&ctx, &params).await.unwrap());
    }

    #[tokio::test]
    async fn test_geo_match_country_region() {
        let handler = GeoMatchCondition;
        let mut ctx = Context::from_request(None, None);
        ctx.location = GeoInfo {
            country: Some("US".to_string()),
            region: Some("California".to_string()),
            city: None,
            lat: None,
            lon: None,
        };

        let params = serde_json::json!({"country": "US", "region": "California"});
        assert!(handler.evaluate(&ctx, &params).await.unwrap());
    }

    #[tokio::test]
    async fn test_geo_match_no_location() {
        let handler = GeoMatchCondition;
        let ctx = Context::from_request(None, None);

        let params = serde_json::json!({"country": "CN"});
        assert!(!handler.evaluate(&ctx, &params).await.unwrap());
    }

    #[tokio::test]
    async fn test_register_conditions() {
        let mut registry = ExtensionRegistry::new();
        assert!(register(&mut registry).is_ok());
        assert!(registry.get_condition_handler("header-match").is_some());
        assert!(registry.get_condition_handler("geo-match").is_some());
    }
}
