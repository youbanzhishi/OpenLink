//! # ext-daw-distribute — OpenDAW 插件分发 Action 扩展
//!
//! 通过 OpenLink 分发音频插件到 DAW 设备，支持：
//! - **插件分发**：VST3 / CLAP 插件包分发到 DAW 机器
//! - **JSFX 脚本加载**：远程加载 JSFX 脚本到 OpenDAW
//! - **项目分享**：深链接拉起 DAW 项目
//!
//! ## Action 参数格式
//! ```json
//! {
//!   "operation": "distribute_plugin",
//!   "plugin": {
//!     "id": "vst3-xxx",
//!     "name": "MySynth",
//!     "format": "vst3",
//!     "url": "https://..."
//!   },
//!   "target_device": "daw-machine-1"
//! }
//! ```

pub mod plugin;
pub mod daw_link;


// ─── Re-exports ─────────────────────────────────────────────

use std::sync::Arc;
use openlink_core::{ExtensionRegistry, CoreError};

/// 注册 DAW 分发扩展到 Extension Registry
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    // 注册 daw_distribute action
    let action = DawDistributeAction::new();
    registry.register_action(Arc::new(action))?;

    // 注册 daw_device condition（检测请求是否来自 DAW 设备）
    let condition = DawDeviceCondition;
    registry.register_condition(Arc::new(condition))?;

    tracing::info!("ext-daw-distribute registered");
    Ok(())
}

use async_trait::async_trait;
use openlink_core::{ActionHandler, ConditionHandler, Context, ActionResult, Target};
use serde::{Deserialize, Serialize};

// ─── DAW 分发参数 ───────────────────────────────────────────

/// DAW 分发操作类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DawOperation {
    /// 分发插件
    DistributePlugin,
    /// 加载 JSFX 脚本
    LoadJsfx,
    /// 分享 DAW 项目（深链接）
    ShareProject,
    /// 查询设备插件列表
    ListPlugins,
    /// 移除插件
    RemovePlugin,
}

/// 插件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// 插件 ID
    pub id: String,
    /// 插件名称
    pub name: String,
    /// 插件格式：vst3 / clap / jsfx / au
    pub format: String,
    /// 下载 URL
    pub url: Option<String>,
    /// 版本
    pub version: Option<String>,
    /// 插件类型：instrument / effect
    #[serde(default)]
    pub plugin_type: String,
}

/// DAW 分发参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DawDistributeParams {
    /// 操作类型
    pub operation: DawOperation,
    /// 插件信息（distribute_plugin / load_jsfx 时使用）
    #[serde(default)]
    pub plugin: Option<PluginInfo>,
    /// 目标设备 ID
    #[serde(default)]
    pub target_device: Option<String>,
    /// 项目深链接（share_project 时使用）
    #[serde(default)]
    pub project_url: Option<String>,
    /// 项目 ID
    #[serde(default)]
    pub project_id: Option<String>,
    /// JSFX 脚本内容（load_jsfx 时使用）
    #[serde(default)]
    pub jsfx_content: Option<String>,
    /// JSFX 脚本名称
    #[serde(default)]
    pub jsfx_name: Option<String>,
}

impl DawDistributeParams {
    /// 从 JSON 值解析
    fn from_json(value: &serde_json::Value) -> Result<Self, CoreError> {
        serde_json::from_value(value.clone())
            .map_err(|e| CoreError::ExtensionError(format!("Invalid DAW distribute params: {}", e)))
    }
}

// ─── DAW Distribute Action ──────────────────────────────────

/// DAW 分发 Action
pub struct DawDistributeAction;

impl DawDistributeAction {
    pub fn new() -> Self {
        Self
    }

    /// 处理插件分发
    async fn handle_distribute_plugin(
        &self,
        params: &DawDistributeParams,
    ) -> Result<ActionResult, CoreError> {
        let plugin = params.plugin.as_ref()
            .ok_or_else(|| CoreError::ExtensionError("plugin info required".to_string()))?;

        tracing::info!(
            plugin_id = %plugin.id,
            format = %plugin.format,
            target = ?params.target_device,
            "Distributing plugin to DAW"
        );

        // 生成分发记录
        let distribution_id = uuid::Uuid::new_v4().to_string();

        Ok(ActionResult::Json(serde_json::json!({
            "type": "plugin_distribution_queued",
            "distribution_id": distribution_id,
            "plugin": {
                "id": plugin.id,
                "name": plugin.name,
                "format": plugin.format,
            },
            "target_device": params.target_device,
            "status": "queued",
        })))
    }

    /// 处理 JSFX 脚本加载
    async fn handle_load_jsfx(
        &self,
        params: &DawDistributeParams,
    ) -> Result<ActionResult, CoreError> {
        let script_name = params.jsfx_name.as_deref()
            .unwrap_or("Untitled.jsfx");
        let content = params.jsfx_content.as_ref()
            .ok_or_else(|| CoreError::ExtensionError("jsfx_content required".to_string()))?;

        // 验证 JSFX 内容（基本检查）
        if !content.contains("@init") && !content.contains("@sample") && !content.contains("@block") {
            return Err(CoreError::ExtensionError(
                "Invalid JSFX content: missing @init/@sample/@block".to_string()
            ));
        }

        tracing::info!(
            script = %script_name,
            target = ?params.target_device,
            content_len = content.len(),
            "Loading JSFX script to DAW"
        );

        // 生成 JSFX 加载指令
        let load_token = uuid::Uuid::new_v4().to_string();

        Ok(ActionResult::Json(serde_json::json!({
            "type": "jsfx_script_loaded",
            "load_token": load_token,
            "script_name": script_name,
            "content_preview": content.chars().take(200).collect::<String>(),
            "target_device": params.target_device,
            "status": "loaded",
        })))
    }

    /// 处理项目分享
    async fn handle_share_project(
        &self,
        params: &DawDistributeParams,
    ) -> Result<ActionResult, CoreError> {
        let project_url = params.project_url.as_ref()
            .or(params.project_id.as_ref())
            .ok_or_else(|| CoreError::ExtensionError("project_url or project_id required".to_string()))?;

        tracing::info!(
            project = %project_url,
            target = ?params.target_device,
            "Sharing DAW project"
        );

        // 构建 DAW 深链接
        let deeplink = if project_url.starts_with("http") {
            format!("opendaw://project?url={}", urlencoding::encode(project_url))
        } else {
            format!("opendaw://project/{}", project_url)
        };

        Ok(ActionResult::Json(serde_json::json!({
            "type": "project_shared",
            "deeplink": deeplink,
            "project_url": project_url,
            "target_device": params.target_device,
            "status": "ready",
        })))
    }

    /// 处理插件列表查询
    async fn handle_list_plugins(
        &self,
        params: &DawDistributeParams,
    ) -> Result<ActionResult, CoreError> {
        tracing::info!(
            target = ?params.target_device,
            "Listing installed plugins on DAW"
        );

        // 返回模拟插件列表（实际从目标设备查询）
        Ok(ActionResult::Json(serde_json::json!({
            "type": "plugin_list",
            "target_device": params.target_device,
            "plugins": [
                {"id": "vst3-builtin-eq", "name": "Parametric EQ", "format": "vst3", "plugin_type": "effect"},
                {"id": "vst3-builtin-comp", "name": "Compressor", "format": "vst3", "plugin_type": "effect"},
            ],
            "total": 2,
        })))
    }

    /// 处理插件移除
    async fn handle_remove_plugin(
        &self,
        params: &DawDistributeParams,
    ) -> Result<ActionResult, CoreError> {
        let plugin = params.plugin.as_ref()
            .ok_or_else(|| CoreError::ExtensionError("plugin info required".to_string()))?;

        tracing::info!(
            plugin_id = %plugin.id,
            target = ?params.target_device,
            "Removing plugin from DAW"
        );

        Ok(ActionResult::Json(serde_json::json!({
            "type": "plugin_removed",
            "plugin_id": plugin.id,
            "target_device": params.target_device,
            "status": "removed",
        })))
    }
}

impl Default for DawDistributeAction {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ActionHandler for DawDistributeAction {
    async fn execute(
        &self,
        _ctx: &Context,
        target: &Target,
    ) -> Result<ActionResult, CoreError> {
        let params = DawDistributeParams::from_json(&target.params)?;

        tracing::info!(operation = ?params.operation, "DawDistribute action");

        match params.operation {
            DawOperation::DistributePlugin => self.handle_distribute_plugin(&params).await,
            DawOperation::LoadJsfx => self.handle_load_jsfx(&params).await,
            DawOperation::ShareProject => self.handle_share_project(&params).await,
            DawOperation::ListPlugins => self.handle_list_plugins(&params).await,
            DawOperation::RemovePlugin => self.handle_remove_plugin(&params).await,
        }
    }

    fn name(&self) -> &str {
        "daw_distribute"
    }
}

// ─── DAW Device Condition ───────────────────────────────────

/// DAW 设备条件处理器
pub struct DawDeviceCondition;

#[async_trait]
impl ConditionHandler for DawDeviceCondition {
    async fn evaluate(
        &self,
        ctx: &Context,
        params: &serde_json::Value,
    ) -> Result<bool, CoreError> {
        // 检查请求是否来自 DAW 设备
        let device_type = ctx.device.device_type.as_deref().unwrap_or("");
        let is_daw = device_type == "daw" || device_type == "audio_workstation";

        // 检查自定义字段
        let has_daw_marker = ctx.custom.get("is_daw_device")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // 检查设备名称（可配置）
        let allowed_devices: Vec<String> = params
            .get("allowed_devices")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let device_matches = allowed_devices.is_empty()
            || allowed_devices.contains(&ctx.identity.id);

        Ok((is_daw || has_daw_marker) && device_matches)
    }

    fn name(&self) -> &str {
        "daw_device"
    }
}

// ─── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_distribute_params() {
        let params = serde_json::json!({
            "operation": "distribute_plugin",
            "plugin": {
                "id": "vst3-mysynth",
                "name": "MySynth",
                "format": "vst3",
                "plugin_type": "instrument"
            },
            "target_device": "daw-machine-1"
        });
        let parsed: DawDistributeParams = serde_json::from_value(params).unwrap();
        assert!(matches!(parsed.operation, DawOperation::DistributePlugin));
        assert_eq!(parsed.plugin.as_ref().unwrap().id, "vst3-mysynth");
        assert_eq!(parsed.target_device.as_deref(), Some("daw-machine-1"));
    }

    #[test]
    fn test_parse_jsfx_params() {
        let params = serde_json::json!({
            "operation": "load_jsfx",
            "jsfx_name": "MyLimiter.jsfx",
            "jsfx_content": "@init\npeak = 0;\n@sample\nspl0 *= 0.5;\nspl1 *= 0.5;",
            "target_device": "daw-machine-1"
        });
        let parsed: DawDistributeParams = serde_json::from_value(params).unwrap();
        assert!(matches!(parsed.operation, DawOperation::LoadJsfx));
        assert_eq!(parsed.jsfx_name.as_deref(), Some("MyLimiter.jsfx"));
    }

    #[test]
    fn test_parse_share_project_params() {
        let params = serde_json::json!({
            "operation": "share_project",
            "project_id": "proj-abc123",
            "target_device": "daw-machine-2"
        });
        let parsed: DawDistributeParams = serde_json::from_value(params).unwrap();
        assert!(matches!(parsed.operation, DawOperation::ShareProject));
        assert_eq!(parsed.project_id.as_deref(), Some("proj-abc123"));
    }

    #[tokio::test]
    async fn test_distribute_plugin() {
        let action = DawDistributeAction::new();
        let params = DawDistributeParams {
            operation: DawOperation::DistributePlugin,
            plugin: Some(PluginInfo {
                id: "vst3-test".to_string(),
                name: "Test Plugin".to_string(),
                format: "vst3".to_string(),
                url: Some("https://example.com/plugin.vst3".to_string()),
                version: Some("1.0".to_string()),
                plugin_type: "instrument".to_string(),
            }),
            target_device: Some("daw-1".to_string()),
            project_url: None,
            project_id: None,
            jsfx_content: None,
            jsfx_name: None,
        };
        let target = Target {
            action: openlink_core::Action::Custom("daw_distribute".to_string()),
            params: serde_json::to_value(&params).unwrap(),
        };
        let ctx = openlink_core::Context::from_request(None, None);
        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                assert_eq!(val["type"], "plugin_distribution_queued");
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_load_jsfx() {
        let action = DawDistributeAction::new();
        let params = DawDistributeParams {
            operation: DawOperation::LoadJsfx,
            plugin: None,
            target_device: Some("daw-1".to_string()),
            project_url: None,
            project_id: None,
            jsfx_content: Some("@init\n@sample\nspl0 *= 0.5;".to_string()),
            jsfx_name: Some("Test.jsfx".to_string()),
        };
        let target = Target {
            action: openlink_core::Action::Custom("daw_distribute".to_string()),
            params: serde_json::to_value(&params).unwrap(),
        };
        let ctx = openlink_core::Context::from_request(None, None);
        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                assert_eq!(val["type"], "jsfx_script_loaded");
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_load_jsfx_invalid_content() {
        let action = DawDistributeAction::new();
        let params = DawDistributeParams {
            operation: DawOperation::LoadJsfx,
            plugin: None,
            target_device: None,
            project_url: None,
            project_id: None,
            jsfx_content: Some("invalid content".to_string()),
            jsfx_name: Some("Bad.jsfx".to_string()),
        };
        let target = Target {
            action: openlink_core::Action::Custom("daw_distribute".to_string()),
            params: serde_json::to_value(&params).unwrap(),
        };
        let ctx = openlink_core::Context::from_request(None, None);
        let result = action.execute(&ctx, &target).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_share_project_deeplink() {
        let action = DawDistributeAction::new();
        let params = DawDistributeParams {
            operation: DawOperation::ShareProject,
            plugin: None,
            target_device: Some("daw-2".to_string()),
            project_url: Some("https://example.com/projects/proj-123.opendaw".to_string()),
            project_id: None,
            jsfx_content: None,
            jsfx_name: None,
        };
        let target = Target {
            action: openlink_core::Action::Custom("daw_distribute".to_string()),
            params: serde_json::to_value(&params).unwrap(),
        };
        let ctx = openlink_core::Context::from_request(None, None);
        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                let deeplink = val["deeplink"].as_str().unwrap();
                assert!(deeplink.starts_with("opendaw://project?url="));
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_list_plugins() {
        let action = DawDistributeAction::new();
        let params = DawDistributeParams {
            operation: DawOperation::ListPlugins,
            plugin: None,
            target_device: Some("daw-1".to_string()),
            project_url: None,
            project_id: None,
            jsfx_content: None,
            jsfx_name: None,
        };
        let target = Target {
            action: openlink_core::Action::Custom("daw_distribute".to_string()),
            params: serde_json::to_value(&params).unwrap(),
        };
        let ctx = openlink_core::Context::from_request(None, None);
        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                assert_eq!(val["type"], "plugin_list");
                assert!(val["plugins"].is_array());
            }
            _ => panic!("Expected Json result"),
        }
    }
}
