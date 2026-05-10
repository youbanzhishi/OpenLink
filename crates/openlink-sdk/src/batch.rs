//! # BatchClient — Batch Operations with Concurrency Control
//!
//! Supports batch create, resolve, and delete of short links
//! with configurable concurrency limits via semaphore.
//!
//! ## Example
//!
//! ```rust,ignore
//! use openlink_sdk::batch::{BatchClient, BatchRequest};
//!
//! let client = BatchClient::new(config)
//!     .with_max_concurrency(5);
//!
//! let results = client.batch_create(requests).await?;
//! ```

use std::sync::Arc;

use reqwest::Client;
use tokio::sync::Semaphore;

use crate::config::Config;
use crate::error::SdkError;
use crate::models::*;

// ─── Batch Request/Response Types ─────────────────────────────────────────

/// A single item in a batch request.
#[derive(Debug, Clone)]
pub struct BatchRequestItem {
    /// Unique ID for tracking this item within the batch.
    pub id: String,
    /// The operation type.
    pub operation: BatchOperation,
}

/// Batch operation type.
#[derive(Debug, Clone)]
pub enum BatchOperation {
    /// Create a short link.
    Create(CreateLinkRequest),
    /// Resolve a short code.
    Resolve(String),
    /// Delete a short link.
    Delete(String),
}

/// Result of a batch operation for a single item.
#[derive(Debug, Clone)]
pub struct BatchResult<T> {
    /// The item ID.
    pub id: String,
    /// The result (Ok or Err).
    pub result: Result<T, SdkError>,
}

/// Response for a batch operation.
#[derive(Debug, Clone)]
pub struct BatchResponse<T> {
    /// Results for each item.
    pub results: Vec<BatchResult<T>>,
    /// Number of succeeded items.
    pub succeeded: usize,
    /// Number of failed items.
    pub failed: usize,
}

impl<T> BatchResponse<T> {
    /// Create a new batch response from results.
    pub fn from_results(results: Vec<BatchResult<T>>) -> Self {
        let succeeded = results.iter().filter(|r| r.result.is_ok()).count();
        let failed = results.len() - succeeded;
        Self {
            results,
            succeeded,
            failed,
        }
    }

    /// Check if all operations succeeded.
    pub fn all_succeeded(&self) -> bool {
        self.failed == 0
    }
}

// ─── BatchClient ──────────────────────────────────────────────────────────

/// Batch operation client with concurrency control.
pub struct BatchClient {
    client: Client,
    config: Arc<Config>,
    semaphore: Arc<Semaphore>,
}

impl BatchClient {
    /// Create a new BatchClient with default concurrency (4).
    pub fn new(config: Config) -> Self {
        Self::with_max_concurrency(config, 4)
    }

    /// Create a new BatchClient with a specific concurrency limit.
    pub fn with_max_concurrency(config: Config, max_concurrency: usize) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            config: Arc::new(config),
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
        }
    }

    /// Get the maximum concurrency.
    pub fn max_concurrency(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Add authentication headers.
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

    /// Batch create short links.
    pub async fn batch_create(
        &self,
        links: Vec<CreateLinkRequest>,
    ) -> Result<BatchCreateResponse, SdkError> {
        let body = BatchCreateRequest { links };
        let req = self
            .client
            .post(self.config.api_url("/api/v1/links/batch"))
            .json(&body);

        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| SdkError::Other(format!("Semaphore error: {}", e)))?;

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// Batch resolve short codes.
    pub async fn batch_resolve(
        &self,
        codes: Vec<String>,
    ) -> Result<BatchResolveResponse, SdkError> {
        let body = BatchResolveRequest { codes };
        let req = self
            .client
            .post(self.config.api_url("/api/v1/agent/resolve"))
            .json(&body);

        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| SdkError::Other(format!("Semaphore error: {}", e)))?;

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// Batch delete short links.
    pub async fn batch_delete(&self, codes: Vec<String>) -> Result<BatchDeleteResponse, SdkError> {
        let body = BatchDeleteRequest { codes };
        let req = self
            .client
            .post(self.config.api_url("/api/v1/links/batch-delete"))
            .json(&body);

        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| SdkError::Other(format!("Semaphore error: {}", e)))?;

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// Batch resolve with concurrency control — resolves each code individually
    /// with semaphore-limited concurrency.
    pub async fn batch_resolve_concurrent(
        &self,
        codes: Vec<String>,
    ) -> BatchResponse<ResolveResult> {
        let mut handles = Vec::with_capacity(codes.len());

        for code in codes {
            let client = self.client.clone();
            let config = self.config.clone();
            let semaphore = self.semaphore.clone();
            let code_clone = code.clone();

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await;
                let url = config.api_url(&format!("/api/v1/resolve/{}", code_clone));

                let mut req = client.get(&url);
                if let Some(token) = &config.api_token {
                    req = req.header("Authorization", format!("Bearer {}", token));
                }

                match req.send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        match resp.text().await {
                            Ok(body) => {
                                if status.is_success() {
                                    match serde_json::from_str::<ResolveResult>(&body) {
                                        Ok(result) => Ok(result),
                                        Err(e) => Err(SdkError::Serialization(e.to_string())),
                                    }
                                } else {
                                    Err(SdkError::Http {
                                        status: status.as_u16(),
                                        message: body,
                                    })
                                }
                            }
                            Err(e) => Err(SdkError::Network(e.to_string())),
                        }
                    }
                    Err(e) => Err(SdkError::Network(e.to_string())),
                }
            });

            handles.push((code, handle));
        }

        let mut results = Vec::with_capacity(handles.len());
        for (code, handle) in handles {
            let result = match handle.await {
                Ok(Ok(val)) => Ok(val),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(SdkError::Other("Task panicked".to_string())),
            };
            results.push(BatchResult { id: code, result });
        }

        BatchResponse::from_results(results)
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
        let config = Config::new("https://api.example.com").api_token("test-token");
        let _client = BatchClient::new(config);
    }

    #[test]
    fn test_batch_client_with_concurrency() {
        let config = Config::new("https://api.example.com").api_token("test-token");
        let client = BatchClient::with_max_concurrency(config, 8);
        assert!(client.max_concurrency() > 0);
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

    #[test]
    fn test_batch_response_all_succeeded() {
        let results: Vec<BatchResult<String>> = vec![
            BatchResult {
                id: "1".to_string(),
                result: Ok("ok1".to_string()),
            },
            BatchResult {
                id: "2".to_string(),
                result: Ok("ok2".to_string()),
            },
        ];
        let resp = BatchResponse::from_results(results);
        assert!(resp.all_succeeded());
        assert_eq!(resp.succeeded, 2);
        assert_eq!(resp.failed, 0);
    }

    #[test]
    fn test_batch_response_partial_failure() {
        let results: Vec<BatchResult<String>> = vec![
            BatchResult {
                id: "1".to_string(),
                result: Ok("ok".to_string()),
            },
            BatchResult {
                id: "2".to_string(),
                result: Err(SdkError::Other("fail".to_string())),
            },
        ];
        let resp = BatchResponse::from_results(results);
        assert!(!resp.all_succeeded());
        assert_eq!(resp.succeeded, 1);
        assert_eq!(resp.failed, 1);
    }

    #[test]
    fn test_batch_operation_variants() {
        let create = BatchOperation::Create(CreateLinkRequest {
            target: "https://example.com".to_string(),
            code: None,
            metadata: serde_json::Value::Null,
            is_active: true,
            owner: None,
        });
        let resolve = BatchOperation::Resolve("abc".to_string());
        let delete = BatchOperation::Delete("abc".to_string());

        // Verify variants compile and match
        match create {
            BatchOperation::Create(req) => assert_eq!(req.target, "https://example.com"),
            _ => panic!("Expected Create"),
        }
        match resolve {
            BatchOperation::Resolve(code) => assert_eq!(code, "abc"),
            _ => panic!("Expected Resolve"),
        }
        match delete {
            BatchOperation::Delete(code) => assert_eq!(code, "abc"),
            _ => panic!("Expected Delete"),
        }
    }

    #[test]
    fn test_batch_request_item() {
        let item = BatchRequestItem {
            id: "item-1".to_string(),
            operation: BatchOperation::Resolve("abc".to_string()),
        };
        assert_eq!(item.id, "item-1");
    }
}
