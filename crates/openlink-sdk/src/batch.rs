//! # BatchClient — 批量操作客户端
//!
//! 支持批量创建、解析、删除短链。

use std::sync::Arc;
use reqwest::Client;

use crate::config::Config;
use crate::error::SdkError;
use crate::models::*;

/// 批量操作客户端
pub struct BatchClient {
    client: Client,
    config: Arc<Config>,
}

impl BatchClient {
    /// 创建新的 BatchClient
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
        if let Some(device_id) = &self.config.device_id {
            req = req.header("X-Device-ID", device_id.as_str());
        }
        req
    }

    /// 批量创建短链
    pub async fn batch_create(
        &self,
        links: Vec<CreateLinkRequest>,
    ) -> Result<BatchCreateResponse, SdkError> {
        let body = BatchCreateRequest { links };
        let req = self.client
            .post(self.config.api_url("/api/v1/links/batch"))
            .json(&body);

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 批量解析短链
    pub async fn batch_resolve(
        &self,
        codes: Vec<String>,
    ) -> Result<BatchResolveResponse, SdkError> {
        let body = BatchResolveRequest { codes };
        let req = self.client
            .post(self.config.api_url("/api/v1/agent/resolve"))
            .json(&body);

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 批量删除短链
    pub async fn batch_delete(
        &self,
        codes: Vec<String>,
    ) -> Result<BatchDeleteResponse, SdkError> {
        let body = BatchDeleteRequest { codes };
        let req = self.client
            .post(self.config.api_url("/api/v1/links/batch-delete"))
            .json(&body);

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
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
    fn test_batch_client_new() {
        let config = Config::new("https://api.example.com")
            .api_token("test-token");
        let _client = BatchClient::new(config);
    }

    #[test]
    fn test_batch_create_request_serialization() {
        let req = BatchCreateRequest {
            links: vec![
                CreateLinkRequest {
                    target: "https://example.com/1".to_string(),
                    code: Some("abc".to_string()),
                    metadata: serde_json::Value::Null,
                    is_active: true,
                    owner: Some("agent-1".to_string()),
                },
                CreateLinkRequest {
                    target: "https://example.com/2".to_string(),
                    code: None,
                    metadata: serde_json::Value::Null,
                    is_active: true,
                    owner: None,
                },
            ],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"links\""));
        assert!(json.contains("https://example.com/1"));
        assert!(json.contains("https://example.com/2"));
    }

    #[test]
    fn test_batch_delete_request_serialization() {
        let req = BatchDeleteRequest {
            codes: vec!["abc".to_string(), "def".to_string()],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"codes\""));
        assert!(json.contains("abc"));
        assert!(json.contains("def"));
    }

    #[test]
    fn test_batch_create_response_deserialization() {
        let json = r#"{"results":[{"id":"1","code":"abc","target":"https://example.com","owner":"agent-1","created_at":"2024-01-01T00:00:00Z","metadata":{},"is_active":true}],"succeeded":1,"failed":0}"#;
        let resp: BatchCreateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.succeeded, 1);
        assert_eq!(resp.failed, 0);
        assert_eq!(resp.results.len(), 1);
    }

    #[test]
    fn test_batch_delete_response_deserialization() {
        let json = r#"{"results":[{"code":"abc","deleted":true}],"succeeded":1,"failed":0}"#;
        let resp: BatchDeleteResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.succeeded, 1);
        assert!(resp.results[0].deleted);
    }
}
