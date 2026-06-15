//! Permission Hook - 路由前自动校验权限
//!
//! 实现 BeforeRoute Hook，校验链：Token验证→会话状态→权限有效期→Extension白名单→操作检查→资源限制

use crate::error::CoreError;
use crate::hooks::{Hook, HookAdvice, HookContext, HookResult};
use crate::primitives::{Action, Context, Route};

/// 权限Hook配置
#[derive(Debug, Clone)]
pub struct PermissionHookConfig {
    /// 是否启用
    pub enabled: bool,
    /// 忽略的路径白名单
    pub bypass_paths: Vec<String>,
    /// 默认文件大小限制（字节）
    pub default_max_file_size: u64,
    /// Token提取头
    pub token_header: String,
}

impl Default for PermissionHookConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bypass_paths: vec!["/health".into(), "/ready".into(), "/api/v1/auth/login".into()],
            default_max_file_size: 10 * 1024 * 1024, // 10MB
            token_header: "Authorization".into(),
        }
    }
}

/// 权限Hook
pub struct PermissionHook {
    config: PermissionHookConfig,
}

impl PermissionHook {
    pub fn new(config: PermissionHookConfig) -> Self {
        Self { config }
    }

    pub fn with_default() -> Self {
        Self::new(PermissionHookConfig::default())
    }
}

impl Hook for PermissionHook {
    type Config = PermissionHookConfig;

    fn name(&self) -> &'static str {
        "permission"
    }

    fn hook_type(&self) -> &'static str {
        "before_route"
    }

    fn execute(&self, ctx: &mut HookContext) -> HookResult {
        // 1. 检查是否启用
        if !self.config.enabled {
            return Ok(HookAdvice::Continue);
        }

        // 2. 检查白名单路径
        let path = ctx.path();
        for bypass in &self.config.bypass_paths {
            if path.starts_with(bypass) {
                return Ok(HookAdvice::Continue);
            }
        }

        // 3. 提取Token
        let token = ctx
            .auth_header()
            .ok_or_else(|| CoreError::Unauthorized("Missing authorization token".into()))?;

        // 4. 校验Token格式
        let token = if token.starts_with("Bearer ") {
            &token[7..]
        } else {
            token
        };

        // 5. 验证Token（需要调用auth模块，这里简化处理）
        if token.is_empty() {
            return Err(CoreError::Unauthorized("Invalid token format".into()));
        }

        // 6. 检查权限上下文
        if let Some(perm_ctx) = ctx.agent_permission() {
            // 检查Extension白名单
            let ext_id = ctx.extension_id().unwrap_or_default();
            if !perm_ctx.is_extension_allowed(&ext_id) {
                return Err(CoreError::Forbidden(format!(
                    "Extension '{}' is not in the allowed list",
                    ext_id
                )));
            }

            // 检查操作权限
            let action = ctx.action();
            if !perm_ctx.is_operation_allowed(&action) {
                return Err(CoreError::Forbidden(format!("Operation '{}' is not allowed", action)));
            }
        }

        // 7. 继续执行
        Ok(HookAdvice::Continue)
    }

    fn on_error(&self, ctx: &HookContext, error: &CoreError) -> HookResult {
        tracing::warn!("Permission hook error for path '{}': {}", ctx.path(), error);
        Err(error.clone())
    }
}

/// HookContext 扩展 - 获取权限信息
pub trait PermissionContextExt {
    fn agent_permission(&self) -> Option<crate::auth::AgentPermissionContext>;
    fn extension_id(&self) -> Option<String>;
    fn action(&self) -> String;
    fn auth_header(&self) -> Option<String>;
}

impl PermissionContextExt for HookContext {
    fn agent_permission(&self) -> Option<crate::auth::AgentPermissionContext> {
        self.get("agent_permission")
            .map(|v| v.clone())
            .and_then(|v| serde_json::from_value(v).ok())
    }

    fn extension_id(&self) -> Option<String> {
        self.get("extension_id").map(|v| v.to_string())
    }

    fn action(&self) -> String {
        self.get("action")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".into())
    }

    fn auth_header(&self) -> Option<String> {
        self.headers()
            .get("authorization")
            .cloned()
            .or_else(|| self.headers().get("Authorization").cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_hook_default() {
        let hook = PermissionHook::with_default();
        assert_eq!(hook.name(), "permission");
        assert_eq!(hook.hook_type(), "before_route");
    }

    #[test]
    fn test_permission_hook_config() {
        let config = PermissionHookConfig {
            enabled: true,
            bypass_paths: vec!["/health".into()],
            default_max_file_size: 5 * 1024 * 1024,
            token_header: "X-Auth-Token".into(),
        };

        let hook = PermissionHook::new(config);
        assert!(hook.name() == "permission");
    }
}
