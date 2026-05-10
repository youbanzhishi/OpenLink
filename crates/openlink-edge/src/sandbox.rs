//! # WASM 沙箱执行环境（Phase 5）
//!
//! 提供 WASM 沙箱的 trait 接口和 Mock 实现。
//! 设计目标：
//! - 安全隔离：重定向逻辑在沙箱中执行，不影响主进程
//! - 可插拔：支持不同的 WASM 运行时（wasmtime、wasmer 等）
//! - 轻量：边缘节点友好，最小依赖
//!
//! 当前为 trait + Mock 实现，未来可接入 wasmtime/wasmer。

use crate::wasm_redirect::{EdgeRequest, RedirectDecision};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 沙箱执行错误
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// WASM 模块编译失败
    #[error("Compilation error: {0}")]
    CompilationError(String),

    /// WASM 模块实例化失败
    #[error("Instantiation error: {0}")]
    InstantiationError(String),

    /// 执行超时
    #[error("Execution timeout: exceeded {0}ms")]
    Timeout(u64),

    /// 内存超限
    #[error("Memory limit exceeded: used {0} bytes, limit {1} bytes")]
    MemoryLimitExceeded(usize, usize),

    /// 执行被中止（如陷阱）
    #[error("Execution aborted: {0}")]
    Aborted(String),

    /// 导入函数未找到
    #[error("Import not found: {0}")]
    ImportNotFound(String),

    /// I/O 错误
    #[error("IO error: {0}")]
    Io(String),
}

/// 沙箱资源配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// 最大内存（字节），默认 16MB
    #[serde(default = "default_max_memory")]
    pub max_memory_bytes: usize,

    /// 执行超时（毫秒），默认 100ms
    #[serde(default = "default_execution_timeout")]
    pub execution_timeout_ms: u64,

    /// 最大栈深度，默认 1024
    #[serde(default = "default_max_stack_depth")]
    pub max_stack_depth: u32,

    /// 允许的导入函数列表
    #[serde(default)]
    pub allowed_imports: Vec<String>,
}

fn default_max_memory() -> usize {
    16 * 1024 * 1024
}
fn default_execution_timeout() -> u64 {
    100
}
fn default_max_stack_depth() -> u32 {
    1024
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: default_max_memory(),
            execution_timeout_ms: default_execution_timeout(),
            max_stack_depth: default_max_stack_depth(),
            allowed_imports: Vec::new(),
        }
    }
}

/// WASM 模块元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmModuleInfo {
    /// 模块 ID
    pub id: String,
    /// 模块名称
    pub name: String,
    /// 模块版本
    pub version: String,
    /// 导出的函数列表
    pub exports: Vec<String>,
    /// 模块大小（字节）
    pub size_bytes: usize,
    /// 编译时间戳
    pub compiled_at: i64,
}

/// WASM 沙箱 trait — 所有 WASM 运行时实现此接口
#[async_trait]
pub trait WasmSandbox: Send + Sync {
    /// 编译 WASM 模块
    async fn compile_module(
        &self,
        module_id: &str,
        wasm_bytes: &[u8],
    ) -> Result<WasmModuleInfo, SandboxError>;

    /// 执行重定向决策函数
    ///
    /// 调用 WASM 模块中的 `redirect` 导出函数，
    /// 传入请求信息，返回重定向决策。
    async fn execute_redirect(
        &self,
        module_id: &str,
        request: &EdgeRequest,
    ) -> Result<Option<RedirectDecision>, SandboxError>;

    /// 列出已加载的模块
    async fn list_modules(&self) -> Vec<WasmModuleInfo>;

    /// 卸载模块
    async fn unload_module(&self, module_id: &str) -> Result<(), SandboxError>;

    /// 获取沙箱配置
    fn config(&self) -> &SandboxConfig;

    /// 获取运行时名称
    fn runtime_name(&self) -> &str;
}

/// Mock WASM 沙箱（开发/测试用）
///
/// 不执行真实 WASM，而是用内置规则模拟。
/// 用于边缘节点无法运行 WASM 的场景。
pub struct MockSandbox {
    config: SandboxConfig,
    /// 模拟的模块列表
    modules: tokio::sync::RwLock<HashMap<String, WasmModuleInfo>>,
    /// 模拟的重定向规则（module_id → (code → target_url)）
    rules: tokio::sync::RwLock<HashMap<String, HashMap<String, String>>>,
}

impl MockSandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            modules: tokio::sync::RwLock::new(HashMap::new()),
            rules: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 添加模拟规则
    pub async fn add_mock_rule(&self, module_id: &str, code: &str, target_url: &str) {
        let mut rules = self.rules.write().await;
        rules
            .entry(module_id.to_string())
            .or_default()
            .insert(code.to_string(), target_url.to_string());
    }
}

impl Default for MockSandbox {
    fn default() -> Self {
        Self::new(SandboxConfig::default())
    }
}

#[async_trait]
impl WasmSandbox for MockSandbox {
    async fn compile_module(
        &self,
        module_id: &str,
        wasm_bytes: &[u8],
    ) -> Result<WasmModuleInfo, SandboxError> {
        // 检查大小限制
        if wasm_bytes.len() > self.config.max_memory_bytes {
            return Err(SandboxError::MemoryLimitExceeded(
                wasm_bytes.len(),
                self.config.max_memory_bytes,
            ));
        }

        let info = WasmModuleInfo {
            id: module_id.to_string(),
            name: format!("mock-{}", module_id),
            version: "0.1.0".to_string(),
            exports: vec!["redirect".to_string()],
            size_bytes: wasm_bytes.len(),
            compiled_at: chrono::Utc::now().timestamp(),
        };

        let mut modules = self.modules.write().await;
        modules.insert(module_id.to_string(), info.clone());

        tracing::info!(
            module_id = %module_id,
            size = wasm_bytes.len(),
            "Mock module compiled"
        );

        Ok(info)
    }

    async fn execute_redirect(
        &self,
        module_id: &str,
        request: &EdgeRequest,
    ) -> Result<Option<RedirectDecision>, SandboxError> {
        // 检查模块是否存在
        {
            let modules = self.modules.read().await;
            if !modules.contains_key(module_id) {
                return Err(SandboxError::ImportNotFound(module_id.to_string()));
            }
        }

        // 查找模拟规则
        let rules = self.rules.read().await;
        if let Some(module_rules) = rules.get(module_id) {
            if let Some(target_url) = module_rules.get(&request.code) {
                return Ok(Some(RedirectDecision {
                    target_url: target_url.clone(),
                    status_code: 302,
                    matched_rule_id: Some(format!("mock-{}-{}", module_id, request.code)),
                    cache_hit: false,
                }));
            }
        }

        Ok(None)
    }

    async fn list_modules(&self) -> Vec<WasmModuleInfo> {
        let modules = self.modules.read().await;
        modules.values().cloned().collect()
    }

    async fn unload_module(&self, module_id: &str) -> Result<(), SandboxError> {
        let mut modules = self.modules.write().await;
        let mut rules = self.rules.write().await;

        modules
            .remove(module_id)
            .ok_or_else(|| SandboxError::ImportNotFound(module_id.to_string()))?;
        rules.remove(module_id);

        tracing::info!(module_id = %module_id, "Mock module unloaded");
        Ok(())
    }

    fn config(&self) -> &SandboxConfig {
        &self.config
    }

    fn runtime_name(&self) -> &str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_sandbox_compile() {
        let sandbox = MockSandbox::default();
        let wasm_bytes = b"WASM_BINARY_PLACEHOLDER";

        let info = sandbox
            .compile_module("test-module", wasm_bytes)
            .await
            .unwrap();
        assert_eq!(info.id, "test-module");
        assert_eq!(info.size_bytes, wasm_bytes.len());
        assert!(info.exports.contains(&"redirect".to_string()));
    }

    #[tokio::test]
    async fn test_mock_sandbox_execute() {
        let sandbox = MockSandbox::default();
        sandbox
            .compile_module("test-module", b"fake")
            .await
            .unwrap();
        sandbox
            .add_mock_rule("test-module", "abc", "https://example.com")
            .await;

        let request = EdgeRequest {
            code: "abc".to_string(),
            client_ip: None,
            user_agent: None,
            device_type: None,
            identity_type: None,
            geo_region: None,
            headers: HashMap::new(),
        };

        let decision = sandbox
            .execute_redirect("test-module", &request)
            .await
            .unwrap();
        assert!(decision.is_some());
        assert_eq!(decision.unwrap().target_url, "https://example.com");
    }

    #[tokio::test]
    async fn test_mock_sandbox_unload() {
        let sandbox = MockSandbox::default();
        sandbox
            .compile_module("test-module", b"fake")
            .await
            .unwrap();

        assert!(sandbox.unload_module("test-module").await.is_ok());
        assert!(sandbox.unload_module("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn test_mock_sandbox_memory_limit() {
        let config = SandboxConfig {
            max_memory_bytes: 10,
            ..Default::default()
        };
        let sandbox = MockSandbox::new(config);

        let result = sandbox.compile_module("big-module", b"01234567890").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SandboxError::MemoryLimitExceeded(used, limit) => {
                assert!(used > limit);
            }
            _ => panic!("Expected MemoryLimitExceeded"),
        }
    }

    #[tokio::test]
    async fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.max_memory_bytes, 16 * 1024 * 1024);
        assert_eq!(config.execution_timeout_ms, 100);
        assert_eq!(config.max_stack_depth, 1024);
    }
}
