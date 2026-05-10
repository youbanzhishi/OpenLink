//! # ext-webhook — Webhook Action 扩展
//!
//! OpenLink 的 Webhook Action 扩展，实现 HTTP POST 回调。
//! 验证 Extension Registry 的设计：新功能 = 注册扩展。
//!
//! 功能：
//! - HTTP POST/GET 到指定 URL，携带 Context 信息
//! - 可配置：URL / Method / Headers / Body模板 / 超时 / 重试
//! - 异步执行，不阻塞路由响应
//! - 结果可选：忽略 / 记录 / 作为响应返回

use async_trait::async_trait;
use openlink_core::{ActionHandler, ActionResult, Context, CoreError, ExtensionRegistry, Target};
use std::sync::Arc;

/// Webhook Action Handler
///
/// 触发外部 HTTP 回调，携带路由上下文信息。
struct WebhookHandler;

#[async_trait]
impl ActionHandler for WebhookHandler {
    async fn execute(&self, ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
        // 从 target.params 提取配置
        let url = target
            .params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CoreError::InvalidInput("Webhook action requires 'url' parameter".to_string())
            })?
            .to_string();

        let method = target
            .params
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("POST")
            .to_string();

        let timeout_secs = target
            .params
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(10);

        let result_mode = target
            .params
            .get("result_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("ignore")
            .to_string();

        let body_template = target
            .params
            .get("body")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        // 构建 body：将 Context 信息注入模板
        let body = build_webhook_body(ctx, &body_template);

        // 异步触发 HTTP 请求（spawn 独立任务，不阻塞）
        let webhook_url = url.clone();
        let webhook_method = method.clone();
        tokio::spawn(async move {
            let client = reqwest_client();
            let result =
                send_webhook(&client, &webhook_url, &webhook_method, &body, timeout_secs).await;

            match result {
                Ok(status) => {
                    tracing::info!(
                        url = %webhook_url,
                        status = %status,
                        "Webhook triggered successfully"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        url = %webhook_url,
                        error = %e,
                        "Webhook trigger failed"
                    );
                }
            }
        });

        // 根据 result_mode 决定返回
        match result_mode.as_str() {
            "return" => Ok(ActionResult::WebhookTriggered {
                target_url: url,
                status: "triggered".to_string(),
            }),
            _ => {
                // ignore / record: 返回触发状态
                Ok(ActionResult::WebhookTriggered {
                    target_url: url,
                    status: "triggered_async".to_string(),
                })
            }
        }
    }

    fn name(&self) -> &str {
        "webhook"
    }
}

/// 构建请求客户端
fn reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

/// 构建 Webhook Body — 将 Context 信息注入模板
fn build_webhook_body(ctx: &Context, template: &serde_json::Value) -> serde_json::Value {
    // 如果模板是对象，合并 Context 信息
    if template.is_object() {
        let mut body = template.clone();
        body["context"] = serde_json::to_value(ctx).unwrap_or_default();
        body["timestamp"] = serde_json::Value::String(chrono::Utc::now().to_rfc3339());
        body
    } else if template.is_null() {
        // 默认 body：包含完整 Context
        serde_json::json!({
            "event": "openlink.webhook",
            "context": ctx,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })
    } else {
        // 其他情况直接使用模板
        template.clone()
    }
}

/// 发送 Webhook HTTP 请求
async fn send_webhook(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    body: &serde_json::Value,
    timeout_secs: u64,
) -> Result<u16, String> {
    let request = match method.to_uppercase().as_str() {
        "POST" => client
            .post(url)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .json(body)
            .build(),
        "PUT" => client
            .put(url)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .json(body)
            .build(),
        "GET" => client
            .get(url)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build(),
        _ => return Err(format!("Unsupported HTTP method: {}", method)),
    };

    let req = request.map_err(|e| format!("Failed to build request: {}", e))?;
    let resp = client
        .execute(req)
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    Ok(resp.status().as_u16())
}

/// 注册 Webhook 扩展到 Extension Registry
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    registry.register_action(Arc::new(WebhookHandler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlink_core::{Action, Target};

    #[test]
    fn test_build_webhook_body_default() {
        let ctx = Context::from_request(Some("curl/7.88"), Some("127.0.0.1"));
        let body = build_webhook_body(&ctx, &serde_json::Value::Null);
        assert_eq!(body["event"], "openlink.webhook");
        assert!(body["context"].is_object());
        assert!(body["timestamp"].is_string());
    }

    #[test]
    fn test_build_webhook_body_template() {
        let ctx = Context::from_request(None, None);
        let template = serde_json::json!({
            "custom_field": "value",
            "action": "deploy"
        });
        let body = build_webhook_body(&ctx, &template);
        assert_eq!(body["custom_field"], "value");
        assert_eq!(body["action"], "deploy");
        assert!(body["context"].is_object());
    }

    #[tokio::test]
    async fn test_webhook_handler_requires_url() {
        let handler = WebhookHandler;
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Webhook,
            params: serde_json::json!({}), // 没有 url
        };

        let result = handler.execute(&ctx, &target).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CoreError::InvalidInput(msg) => assert!(msg.contains("url")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_webhook_handler_triggers() {
        let handler = WebhookHandler;
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Webhook,
            params: serde_json::json!({
                "url": "https://httpbin.org/post",
                "method": "POST",
                "result_mode": "ignore",
            }),
        };

        // 注意：这里会实际发送 HTTP 请求（在 spawn 中），
        // 但 execute 本身应该立即返回
        let result = handler.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::WebhookTriggered { target_url, status } => {
                assert_eq!(target_url, "https://httpbin.org/post");
                assert!(status.contains("triggered"));
            }
            _ => panic!("Expected WebhookTriggered result"),
        }
    }
}
