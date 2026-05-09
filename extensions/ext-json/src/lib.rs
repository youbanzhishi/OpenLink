//! # ext-json — JSON 数据响应 Action 扩展
//!
//! 当 Agent/curl 访问短链时，直接返回 JSON 数据而非重定向。
//! 这是 Phase 2 核心场景：同一短链，浏览器跳网页，curl 返回 JSON。
//!
//! 设计验证：新功能 = 注册扩展，架构本身永远不需要改。

use std::sync::Arc;
use async_trait::async_trait;
use openlink_core::{ActionHandler, ExtensionRegistry, Context, Target, ActionResult, CoreError};

/// JSON 数据响应 Action Handler
///
/// 返回 JSON 格式的数据，适用于 Agent/程序化访问场景。
struct JsonDataHandler;

#[async_trait]
impl ActionHandler for JsonDataHandler {
    async fn execute(&self, ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
        // 从 target.params 构建 JSON 响应
        let data = if target.params.is_null() || target.params == serde_json::Value::Null {
            // 默认响应：包含 Context 信息
            serde_json::json!({
                "message": "OpenLink API Response",
                "context": {
                    "identity_type": format!("{:?}", ctx.identity.identity_type).to_lowercase(),
                    "device_type": ctx.device.device_type,
                }
            })
        } else {
            // 自定义数据 + Context 元信息
            let mut response = target.params.clone();
            if let Some(obj) = response.as_object_mut() {
                obj.insert(
                    "_meta".to_string(),
                    serde_json::json!({
                        "identity_type": format!("{:?}", ctx.identity.identity_type).to_lowercase(),
                        "device_type": ctx.device.device_type,
                        "session": ctx.session,
                    }),
                );
            }
            response
        };

        tracing::debug!("JsonData action: returning JSON response");
        Ok(ActionResult::Json(data))
    }

    fn name(&self) -> &str {
        "json_data"
    }
}

/// 注册 JSON 数据响应扩展到 Extension Registry
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    registry.register_action(Arc::new(JsonDataHandler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlink_core::{Action, IdentityType};

    #[tokio::test]
    async fn test_json_data_handler_with_params() {
        let handler = JsonDataHandler;
        let ctx = Context::from_request(Some("curl/7.88"), Some("127.0.0.1"));
        let target = Target {
            action: Action::JsonData,
            params: serde_json::json!({
                "api_version": "v1",
                "resource": "link_data",
                "url": "https://example.com/long-url",
            }),
        };

        let result = handler.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(data) => {
                assert_eq!(data["api_version"], "v1");
                assert_eq!(data["resource"], "link_data");
                assert!(data["_meta"].is_object());
                assert_eq!(data["_meta"]["identity_type"], "service");
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_json_data_handler_default() {
        let handler = JsonDataHandler;
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::JsonData,
            params: serde_json::Value::Null,
        };

        let result = handler.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(data) => {
                assert_eq!(data["message"], "OpenLink API Response");
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_register_json_data() {
        let mut registry = ExtensionRegistry::new();
        assert!(register(&mut registry).is_ok());
        assert!(registry.get_action_handler("json_data").is_some());
    }
}
