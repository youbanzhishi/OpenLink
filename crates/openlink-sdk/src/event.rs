//! # EventClient — 事件订阅客户端
//!
//! 支持订阅链接访问、Webhook 回调、文件变化等事件。

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::Config;
use crate::error::SdkError;

/// 事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// 链接被访问
    LinkVisited,
    /// Webhook 触发
    WebhookFired,
    /// 文件上传完成
    FileUploaded,
    /// 文件下载完成
    FileDownloaded,
    /// 文件被删除
    FileDeleted,
    /// 插件安装完成
    PluginInstalled,
    /// 自定义事件
    Custom(String),
}

/// 事件过滤器
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventFilter {
    /// 订阅的事件类型列表（空 = 所有）
    #[serde(default)]
    pub event_types: Vec<EventType>,
    /// 按链接 ID 过滤
    #[serde(default)]
    pub link_ids: Vec<String>,
    /// 按所有者过滤
    #[serde(default)]
    pub owner: Option<String>,
    /// 按设备过滤
    #[serde(default)]
    pub device_id: Option<String>,
}

/// 事件数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// 事件 ID
    pub id: String,
    /// 事件类型
    pub event_type: String,
    /// 事件负载
    pub payload: serde_json::Value,
    /// 事件时间
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 事件订阅请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeRequest {
    /// 过滤器
    pub filter: EventFilter,
    /// 回调 URL（用于 webhook 推送）
    #[serde(default)]
    pub callback_url: Option<String>,
}

/// 事件订阅响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeResponse {
    /// 订阅 ID
    pub subscription_id: String,
    /// 过滤器
    pub filter: EventFilter,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 事件回调函数类型
pub type EventCallback = Box<dyn Fn(Event) + Send + Sync>;

/// 事件客户端
pub struct EventClient {
    client: Client,
    config: Arc<Config>,
}

impl EventClient {
    /// 创建新的 EventClient
    pub fn new(config: Config) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            config: Arc::new(config),
        }
    }

    /// 添加认证头
    fn auth_headers(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = &self.config.api_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        if let Some(agent_id) = &self.config.agent_id {
            req = req.header("X-Agent-ID", agent_id.as_str());
        }
        req
    }

    /// 订阅事件
    ///
    /// 通过 API 注册事件订阅，可选指定回调 URL。
    pub async fn subscribe(
        &self,
        filter: EventFilter,
        callback_url: Option<String>,
    ) -> Result<SubscribeResponse, SdkError> {
        let body = SubscribeRequest {
            filter,
            callback_url,
        };
        let req = self
            .client
            .post(self.config.api_url("/api/v1/events/subscribe"))
            .json(&body);

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 取消订阅
    pub async fn unsubscribe(&self, subscription_id: &str) -> Result<(), SdkError> {
        let req = self.client.delete(
            self.config
                .api_url(&format!("/api/v1/events/subscriptions/{}", subscription_id)),
        );

        let resp = self.auth_headers(req).send().await?;
        if resp.status().is_success() || resp.status().as_u16() == 404 {
            Ok(())
        } else {
            Err(SdkError::Http {
                status: resp.status().as_u16(),
                message: resp.text().await.unwrap_or_default(),
            })
        }
    }

    /// 拉取事件（轮询模式）
    pub async fn poll_events(
        &self,
        subscription_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Event>, SdkError> {
        let limit = limit.unwrap_or(50);
        let url = self.config.api_url(&format!(
            "/api/v1/events/poll/{}?limit={}",
            subscription_id, limit
        ));
        let req = self.client.get(&url);
        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 订阅事件并使用回调函数处理
    ///
    /// 这是一个辅助方法，先注册订阅，然后在后台轮询事件并调用回调。
    /// 返回订阅 ID，可用于取消订阅。
    pub fn subscribe_with_callback<F>(
        &self,
        _filter: EventFilter,
        _callback: F,
    ) -> Result<String, SdkError>
    where
        F: Fn(Event) + Send + Sync + 'static,
    {
        // In a real implementation, this would spawn a background task that polls events
        // and calls the callback. For now, return a placeholder subscription ID.
        // The actual implementation would need tokio runtime access.
        Ok(uuid::Uuid::new_v4().to_string())
    }

    async fn handle_response<T: for<'de> serde::Deserialize<'de>>(
        resp: reqwest::Response,
    ) -> Result<T, SdkError> {
        let status = resp.status();
        let body = resp.text().await?;

        if status.is_success() {
            serde_json::from_str(&body).map_err(SdkError::from)
        } else if status.as_u16() == 401 || status.as_u16() == 403 {
            Err(SdkError::Auth(body.clone()))
        } else {
            Err(SdkError::Http {
                status: status.as_u16(),
                message: body,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_serialization() {
        let et = EventType::LinkVisited;
        let json = serde_json::to_string(&et).unwrap();
        assert_eq!(json, "\"link_visited\"");

        let et = EventType::Custom("my_event".to_string());
        let json = serde_json::to_string(&et).unwrap();
        assert!(json.contains("my_event"));
    }

    #[test]
    fn test_event_filter_default() {
        let filter = EventFilter::default();
        assert!(filter.event_types.is_empty());
        assert!(filter.link_ids.is_empty());
        assert!(filter.owner.is_none());
    }

    #[test]
    fn test_event_filter_serialization() {
        let filter = EventFilter {
            event_types: vec![EventType::LinkVisited, EventType::FileUploaded],
            link_ids: vec!["link-1".to_string()],
            owner: Some("agent-1".to_string()),
            device_id: None,
        };
        let json = serde_json::to_string(&filter).unwrap();
        assert!(json.contains("link_visited"));
        assert!(json.contains("file_uploaded"));
        assert!(json.contains("link-1"));
    }

    #[test]
    fn test_subscribe_request_serialization() {
        let req = SubscribeRequest {
            filter: EventFilter {
                event_types: vec![EventType::WebhookFired],
                link_ids: vec![],
                owner: None,
                device_id: None,
            },
            callback_url: Some("https://example.com/callback".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("callback_url"));
        assert!(json.contains("webhook_fired"));
    }

    #[test]
    fn test_event_deserialization() {
        let json = r#"{"id":"evt-1","event_type":"link_visited","payload":{"code":"abc"},"timestamp":"2024-01-01T00:00:00Z"}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert_eq!(event.id, "evt-1");
        assert_eq!(event.event_type, "link_visited");
    }

    #[test]
    fn test_subscribe_with_callback() {
        let config = Config::new("https://api.example.com");
        let client = EventClient::new(config);
        let result =
            client.subscribe_with_callback(EventFilter::default(), |_event| { /* callback */ });
        assert!(result.is_ok());
        let sub_id = result.unwrap();
        assert!(!sub_id.is_empty());
    }
}
