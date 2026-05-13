//! # SDK Client 实现

use reqwest::Client;
use serde_json::json;
use std::sync::Arc;

use crate::config::{CircuitBreaker, Config};
use crate::error::SdkError;
use crate::models::*;

// ─── LinkClient ─────────────────────────────────────────────

/// 链接客户端
///
/// 提供创建、查询、解析短链等功能。
/// 支持自动重试（指数退避）和熔断器。
pub struct LinkClient {
    client: Client,
    config: Arc<Config>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl LinkClient {
    /// 创建新的 LinkClient
    pub fn new(config: Config) -> Self {
        let cb_config = config.circuit_breaker.clone();
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            config: Arc::new(config),
            circuit_breaker: Arc::new(CircuitBreaker::new(cb_config)),
        }
    }

    /// 创建新的 LinkClient（使用默认配置）
    pub fn default() -> Self {
        Self::new(Config::default())
    }

    /// 获取 HTTP 客户端
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// 获取配置
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// 添加认证头
    fn auth_headers(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = &self.config.api_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        // 添加 Agent 标识头
        if let Some(agent_id) = &self.config.agent_id {
            req = req.header("X-Agent-ID", agent_id.as_str());
        }
        if let Some(agent_type) = &self.config.agent_type {
            req = req.header("X-Agent-Type", agent_type.as_str());
        }
        if let Some(device_id) = &self.config.device_id {
            req = req.header("X-Device-ID", device_id.as_str());
        }
        req
    }

    /// 带重试和熔断器的请求执行
    async fn execute_with_resilience<F, Fut, T>(&self, request_fn: F) -> Result<T, SdkError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, SdkError>>,
    {
        let max_retries = self.config.retry.max_retries;

        let mut last_error: Option<SdkError> = None;

        for attempt in 0..=max_retries {
            // Check circuit breaker
            if !self.circuit_breaker.allow_request() {
                return Err(SdkError::Other("Circuit breaker is open".to_string()));
            }

            match request_fn().await {
                Ok(result) => {
                    self.circuit_breaker.record_success();
                    return Ok(result);
                }
                Err(e) => {
                    // Don't retry on auth errors
                    if e.is_auth_error() {
                        self.circuit_breaker.record_failure();
                        return Err(e);
                    }

                    last_error = Some(e);
                    self.circuit_breaker.record_failure();

                    if attempt < max_retries {
                        let backoff = self.config.retry.backoff_duration(attempt);
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| SdkError::Other("Unknown error".to_string())))
    }

    // ─── 链接操作 ───────────────────────────────────────────

    /// 创建短链
    pub async fn create(&self, target: impl Into<String>) -> Result<LinkResponse, SdkError> {
        let body = json!({
            "target": target.into(),
            "metadata": {},
            "is_active": true
        });

        self.execute_with_resilience(|| {
            let body = body.clone();
            async move {
                let req = self.client.post(self.config.api_url("/api/v1/links")).json(&body);

                let resp = self.auth_headers(req).send().await?;
                Self::handle_response(resp).await
            }
        })
        .await
    }

    /// 创建短链（带完整参数）
    pub async fn create_full(&self, request: CreateLinkRequest) -> Result<LinkResponse, SdkError> {
        let request_json = serde_json::to_value(&request)?;

        self.execute_with_resilience(|| {
            let request_json = request_json.clone();
            async move {
                let req = self
                    .client
                    .post(self.config.api_url("/api/v1/links"))
                    .json(&request_json);

                let resp = self.auth_headers(req).send().await?;
                Self::handle_response(resp).await
            }
        })
        .await
    }

    /// 获取链接
    pub async fn get(&self, code: impl Into<String>) -> Result<LinkResponse, SdkError> {
        let code = code.into();
        self.execute_with_resilience(|| {
            let code = code.clone();
            async move {
                let req = self.client.get(self.config.api_url(&format!("/api/v1/links/{}", code)));

                let resp = self.auth_headers(req).send().await?;
                Self::handle_response(resp).await
            }
        })
        .await
    }

    /// 查询链接列表
    pub async fn list(&self, query: Option<LinkQuery>) -> Result<Vec<LinkResponse>, SdkError> {
        let mut url = self.config.api_url("/api/v1/links");

        if let Some(q) = query {
            let mut params = vec![];
            if let Some(owner) = q.owner {
                params.push(format!("owner={}", owner));
            }
            if let Some(is_active) = q.is_active {
                params.push(format!("is_active={}", is_active));
            }
            if let Some(limit) = q.limit {
                params.push(format!("limit={}", limit));
            }
            if let Some(offset) = q.offset {
                params.push(format!("offset={}", offset));
            }
            if !params.is_empty() {
                url = format!("{}?{}", url, params.join("&"));
            }
        }

        let url_clone = url.clone();
        self.execute_with_resilience(|| {
            let url = url_clone.clone();
            async move {
                let req = self.client.get(&url);
                let resp = self.auth_headers(req).send().await?;
                Self::handle_response(resp).await
            }
        })
        .await
    }

    /// 删除链接
    pub async fn delete(&self, code: impl Into<String>) -> Result<(), SdkError> {
        let code = code.into();
        self.execute_with_resilience(|| {
            let code = code.clone();
            async move {
                let req = self
                    .client
                    .delete(self.config.api_url(&format!("/api/v1/links/{}", code)));

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
        })
        .await
    }

    /// 解析短链（获取目标 URL）
    pub async fn resolve(&self, code: impl Into<String>) -> Result<ResolveResult, SdkError> {
        let code = code.into();
        self.execute_with_resilience(|| {
            let code = code.clone();
            async move {
                let req = self
                    .client
                    .get(self.config.api_url(&format!("/api/v1/resolve/{}", code)));

                let resp = self.auth_headers(req).send().await?;
                Self::handle_response(resp).await
            }
        })
        .await
    }

    /// 批量解析短链
    pub async fn batch_resolve(&self, codes: Vec<String>) -> Result<BatchResolveResponse, SdkError> {
        let body = BatchResolveRequest { codes };
        let req = self
            .client
            .post(self.config.api_url("/api/v1/agent/resolve"))
            .json(&body);

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 发现可用链接
    pub async fn discover(
        &self,
        discover_type: impl Into<String>,
        filters: Option<serde_json::Value>,
        limit: Option<usize>,
    ) -> Result<DiscoverResponse, SdkError> {
        let request = DiscoverRequest {
            discover_type: discover_type.into(),
            filters: filters.unwrap_or(serde_json::Value::Null),
            limit: limit.unwrap_or(20),
        };

        let req = self
            .client
            .post(self.config.api_url("/api/v1/agent/discover"))
            .json(&request);

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    // ─── 路由操作 ───────────────────────────────────────────

    /// 创建路由规则
    pub async fn create_route(&self, request: CreateRouteRequest) -> Result<RouteResponse, SdkError> {
        let req = self.client.post(self.config.api_url("/api/v1/routes")).json(&request);

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 获取路由规则
    pub async fn get_route(&self, link_id: impl Into<String>) -> Result<RouteResponse, SdkError> {
        let link_id = link_id.into();
        let req = self
            .client
            .get(self.config.api_url(&format!("/api/v1/routes/{}", link_id)));

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    // ─── 响应处理 ───────────────────────────────────────────

    async fn handle_response<T: for<'de> serde::Deserialize<'de>>(resp: reqwest::Response) -> Result<T, SdkError> {
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

// ─── FileClient ─────────────────────────────────────────────

/// 文件传输客户端
///
/// 提供上传、下载、分享文件等功能。
pub struct FileClient {
    client: Client,
    config: Arc<Config>,
}

impl FileClient {
    /// 创建新的 FileClient
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

    /// 创建新的 FileClient（使用默认配置）
    pub fn default() -> Self {
        Self::new(Config::default())
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

    /// 请求上传 URL
    pub async fn request_upload(&self, request: FileUploadRequest) -> Result<FileUploadResponse, SdkError> {
        let req = self
            .client
            .post(self.config.api_url("/api/v1/files/upload"))
            .json(&request);

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 上传文件（完整流程）
    pub async fn upload(
        &self,
        filename: impl Into<String>,
        content: Vec<u8>,
        content_type: impl Into<String>,
    ) -> Result<FileUploadResponse, SdkError> {
        let filename = filename.into();
        let content_type = content_type.into();
        let size = content.len() as u64;

        // 1. 请求上传 URL
        let upload_req = FileUploadRequest {
            filename: filename.clone(),
            size,
            content_type: content_type.clone(),
            storage: None,
            generate_share_link: true,
            share_link_ttl_secs: Some(3600 * 24 * 7), // 7 天
        };

        let upload_resp = self.request_upload(upload_req).await?;

        // 2. 使用预签名 URL 上传
        let put_resp = self
            .client
            .put(&upload_resp.upload_url)
            .header("Content-Type", &content_type)
            .header("Content-Length", size)
            .body(content)
            .send()
            .await?;

        if !put_resp.status().is_success() {
            return Err(SdkError::Other(format!("Upload failed: {}", put_resp.status())));
        }

        Ok(upload_resp)
    }

    /// 下载文件
    pub async fn download(&self, file_id: impl Into<String>) -> Result<FileDownloadResponse, SdkError> {
        let file_id = file_id.into();
        let req = self
            .client
            .get(self.config.api_url(&format!("/api/v1/files/{}/download", file_id)));

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 获取文件（通过分享码）
    pub async fn get_by_share_code(&self, share_code: impl Into<String>) -> Result<Vec<u8>, SdkError> {
        let share_code = share_code.into();

        // 先获取下载 URL
        let dl_resp: FileDownloadResponse = self.get_by_share_code_url(&share_code).await?;

        // 下载内容
        let resp = self.client.get(&dl_resp.download_url).send().await?;

        if !resp.status().is_success() {
            return Err(SdkError::Other(format!("Download failed: {}", resp.status())));
        }

        let bytes = resp.bytes().await?.to_vec();
        Ok(bytes)
    }

    async fn get_by_share_code_url(&self, share_code: &str) -> Result<FileDownloadResponse, SdkError> {
        let req = self
            .client
            .get(self.config.api_url(&format!("/api/v1/files/share/{}", share_code)));

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 生成分享链接
    pub async fn share(
        &self,
        file_id: impl Into<String>,
        ttl_secs: Option<u64>,
    ) -> Result<FileShareResponse, SdkError> {
        let file_id = file_id.into();
        let body = json!({
            "file_id": file_id,
            "ttl_secs": ttl_secs.unwrap_or(3600 * 24 * 7)
        });

        let req = self.client.post(self.config.api_url("/api/v1/files/share")).json(&body);

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 获取文件信息
    pub async fn info(&self, file_id: impl Into<String>) -> Result<FileInfo, SdkError> {
        let file_id = file_id.into();
        let req = self
            .client
            .get(self.config.api_url(&format!("/api/v1/files/{}", file_id)));

        let resp = self.auth_headers(req).send().await?;
        Self::handle_response(resp).await
    }

    /// 删除文件
    pub async fn delete(&self, file_id: impl Into<String>) -> Result<(), SdkError> {
        let file_id = file_id.into();
        let req = self
            .client
            .delete(self.config.api_url(&format!("/api/v1/files/{}", file_id)));

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

    async fn handle_response<T: for<'de> serde::Deserialize<'de>>(resp: reqwest::Response) -> Result<T, SdkError> {
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

// ─── Builder ───────────────────────────────────────────────

/// Client 构建器
///
/// 支持 `.retry()`, `.timeout()`, `.circuit_breaker()` 等链式调用。
#[derive(Debug, Default)]
pub struct ClientBuilder {
    config: Config,
}

impl ClientBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置基础 URL
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.config.base_url = url.into();
        self
    }

    /// 设置 API Token
    pub fn api_token(mut self, token: impl Into<String>) -> Self {
        self.config.api_token = Some(token.into());
        self
    }

    /// 设置 Agent ID
    pub fn agent_id(mut self, id: impl Into<String>) -> Self {
        self.config.agent_id = Some(id.into());
        self
    }

    /// 设置 Agent 类型
    pub fn agent_type(mut self, type_: impl Into<String>) -> Self {
        self.config.agent_type = Some(type_.into());
        self
    }

    /// 设置 Device ID
    pub fn device_id(mut self, id: impl Into<String>) -> Self {
        self.config.device_id = Some(id.into());
        self
    }

    /// 设置请求超时（秒）
    pub fn timeout(mut self, secs: u64) -> Self {
        self.config.timeout_secs = secs;
        self
    }

    /// 设置自动重试（指数退避）
    pub fn retry(mut self, max_retries: u32) -> Self {
        self.config.retry.max_retries = max_retries;
        self
    }

    /// 设置熔断器
    pub fn circuit_breaker(mut self, failure_threshold: u32, reset_timeout_secs: u64) -> Self {
        self.config.circuit_breaker.failure_threshold = failure_threshold;
        self.config.circuit_breaker.reset_timeout_secs = reset_timeout_secs;
        self.config.circuit_breaker.enabled = true;
        self
    }

    /// 构建 LinkClient
    pub fn build_link_client(self) -> LinkClient {
        LinkClient::new(self.config)
    }

    /// 构建 FileClient
    pub fn build_file_client(self) -> FileClient {
        FileClient::new(self.config)
    }

    /// 构建 LinkClient 和 FileClient
    pub fn build(self) -> (LinkClient, FileClient) {
        let config = self.config.clone();
        (LinkClient::new(config.clone()), FileClient::new(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_builder() {
        let (link, _file) = ClientBuilder::new()
            .base_url("https://api.example.com")
            .api_token("test-token")
            .agent_id("my-agent")
            .agent_type("assistant")
            .device_id("device-001")
            .build();

        assert_eq!(link.config.base_url, "https://api.example.com");
        assert_eq!(link.config.api_token.as_deref(), Some("test-token"));
        assert_eq!(link.config.agent_id.as_deref(), Some("my-agent"));
    }

    #[test]
    fn test_client_builder_with_retry() {
        let (link, _file) = ClientBuilder::new()
            .base_url("https://api.example.com")
            .retry(3)
            .build();

        assert_eq!(link.config.retry.max_retries, 3);
    }

    #[test]
    fn test_client_builder_with_circuit_breaker() {
        let (link, _file) = ClientBuilder::new()
            .base_url("https://api.example.com")
            .circuit_breaker(5, 60)
            .build();

        assert!(link.config.circuit_breaker.enabled);
        assert_eq!(link.config.circuit_breaker.failure_threshold, 5);
        assert_eq!(link.config.circuit_breaker.reset_timeout_secs, 60);
    }

    #[test]
    fn test_client_builder_with_timeout() {
        let (link, _file) = ClientBuilder::new().timeout(120).build();

        assert_eq!(link.config.timeout_secs, 120);
    }

    #[test]
    fn test_config_api_url() {
        let config = Config::new("https://api.openlink.dev");
        assert_eq!(config.api_url("/api/v1/links"), "https://api.openlink.dev/api/v1/links");
        assert_eq!(config.api_url("api/v1/links"), "https://api.openlink.dev/api/v1/links");
    }

    #[test]
    fn test_file_upload_request_serialization() {
        let req = FileUploadRequest {
            filename: "test.txt".to_string(),
            size: 1024,
            content_type: "text/plain".to_string(),
            storage: Some("r2".to_string()),
            generate_share_link: true,
            share_link_ttl_secs: Some(3600),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"filename\":\"test.txt\""));
        assert!(json.contains("\"storage\":\"r2\""));
    }
}
