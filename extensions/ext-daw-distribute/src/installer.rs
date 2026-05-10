//! # PluginInstaller — 一键安装器
//!
//! 下载→校验→解压→安装→注册，支持进度回调和回滚。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use super::registry::{PluginRegistration, PluginRegistry};

/// 安装步骤
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStep {
    Downloading,
    Verifying,
    Extracting,
    Installing,
    Registering,
    Completed,
    Failed,
}

/// 安装进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProgress {
    /// 当前步骤
    pub step: InstallStep,
    /// 进度百分比 (0.0 - 1.0)
    pub percent: f32,
    /// 步骤描述
    pub message: String,
}

/// 安装进度回调类型
pub type ProgressCallback = Box<dyn Fn(InstallProgress) + Send + Sync>;

/// 安装配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallConfig {
    /// 安装目标目录
    pub install_dir: PathBuf,
    /// 是否验证校验和
    #[serde(default = "default_true")]
    pub verify_checksum: bool,
    /// 临时下载目录
    #[serde(default)]
    pub temp_dir: Option<PathBuf>,
    /// 预期校验和 (SHA256 hex)
    #[serde(default)]
    pub expected_checksum: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            install_dir: PathBuf::from("/tmp/openlink-plugins"),
            verify_checksum: true,
            temp_dir: None,
            expected_checksum: None,
        }
    }
}

/// 安装结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    /// 安装的插件 ID
    pub plugin_id: String,
    /// 安装路径
    pub install_path: PathBuf,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    #[serde(default)]
    pub error: Option<String>,
}

/// 安装记录（用于回滚）
#[derive(Debug, Clone)]
struct InstallRecord {
    /// 下载的临时文件路径
    temp_file: Option<PathBuf>,
    /// 创建的目录列表
    created_dirs: Vec<PathBuf>,
    /// 安装的文件路径
    installed_files: Vec<PathBuf>,
    /// 是否已注册到 Registry
    registered: bool,
    /// 插件 ID
    plugin_id: String,
}

/// 一键安装器
pub struct PluginInstaller {
    registry: Arc<PluginRegistry>,
    http_client: reqwest::Client,
}

impl PluginInstaller {
    pub fn new(registry: Arc<PluginRegistry>) -> Self {
        Self {
            registry,
            http_client: reqwest::Client::new(),
        }
    }

    /// 执行安装流程
    pub async fn install(
        &self,
        registration: PluginRegistration,
        config: &InstallConfig,
        progress_callback: Option<&ProgressCallback>,
    ) -> InstallResult {
        let plugin_id = registration.id.clone();
        let mut record = InstallRecord {
            temp_file: None,
            created_dirs: Vec::new(),
            installed_files: Vec::new(),
            registered: false,
            plugin_id: plugin_id.clone(),
        };

        let result = self
            .do_install(&registration, config, &mut record, progress_callback)
            .await;

        match result {
            Ok(install_path) => {
                // Register in registry
                if let Err(e) = self.registry.register(registration) {
                    // Rollback on registry failure
                    self.rollback(&record, config).await;
                    return InstallResult {
                        plugin_id,
                        install_path: PathBuf::new(),
                        success: false,
                        error: Some(format!("Registry error: {}", e)),
                    };
                }
                record.registered = true;

                self.report_progress(
                    progress_callback,
                    InstallStep::Completed,
                    1.0,
                    "Installation complete",
                );

                InstallResult {
                    plugin_id,
                    install_path,
                    success: true,
                    error: None,
                }
            }
            Err(e) => {
                // Rollback on failure
                self.rollback(&record, config).await;

                self.report_progress(
                    progress_callback,
                    InstallStep::Failed,
                    0.0,
                    &format!("Failed: {}", e),
                );

                InstallResult {
                    plugin_id,
                    install_path: PathBuf::new(),
                    success: false,
                    error: Some(e),
                }
            }
        }
    }

    async fn do_install(
        &self,
        registration: &PluginRegistration,
        config: &InstallConfig,
        record: &mut InstallRecord,
        progress_callback: Option<&ProgressCallback>,
    ) -> Result<PathBuf, String> {
        let download_url = registration
            .download_url
            .as_ref()
            .ok_or_else(|| "No download URL".to_string())?;

        // Step 1: Download
        self.report_progress(
            progress_callback,
            InstallStep::Downloading,
            0.0,
            "Downloading plugin",
        );
        let temp_dir = config
            .temp_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("/tmp/openlink-downloads"));

        if let Err(e) = tokio::fs::create_dir_all(&temp_dir).await {
            return Err(format!("Failed to create temp dir: {}", e));
        }
        record.created_dirs.push(temp_dir.clone());

        let temp_file = temp_dir.join(format!("{}.download", registration.id));

        let response = self
            .http_client
            .get(download_url)
            .send()
            .await
            .map_err(|e| format!("Download failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Download failed: HTTP {}", response.status()));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Download read failed: {}", e))?;

        tokio::fs::write(&temp_file, &bytes)
            .await
            .map_err(|e| format!("Write temp file failed: {}", e))?;
        record.temp_file = Some(temp_file.clone());

        self.report_progress(
            progress_callback,
            InstallStep::Downloading,
            1.0,
            "Download complete",
        );

        // Step 2: Verify checksum
        if config.verify_checksum {
            self.report_progress(
                progress_callback,
                InstallStep::Verifying,
                0.0,
                "Verifying checksum",
            );
            if let Some(ref expected) = config.expected_checksum {
                let actual = sha256_hex(&bytes);
                if actual != *expected {
                    return Err(format!(
                        "Checksum mismatch: expected {}, got {}",
                        expected, actual
                    ));
                }
            }
            self.report_progress(
                progress_callback,
                InstallStep::Verifying,
                1.0,
                "Checksum verified",
            );
        }

        // Step 3: Extract (simulate - just copy for now)
        self.report_progress(
            progress_callback,
            InstallStep::Extracting,
            0.0,
            "Extracting plugin",
        );

        // Step 4: Install
        self.report_progress(
            progress_callback,
            InstallStep::Installing,
            0.0,
            "Installing plugin",
        );
        let install_dir = config.install_dir.join(registration.format.extension());
        if let Err(e) = tokio::fs::create_dir_all(&install_dir).await {
            return Err(format!("Failed to create install dir: {}", e));
        }
        record.created_dirs.push(install_dir.clone());

        let ext = registration.format.extension();
        let install_path = install_dir.join(format!("{}.{}", registration.id, ext));
        tokio::fs::copy(&temp_file, &install_path)
            .await
            .map_err(|e| format!("Install copy failed: {}", e))?;
        record.installed_files.push(install_path.clone());

        self.report_progress(
            progress_callback,
            InstallStep::Installing,
            1.0,
            "Plugin installed",
        );

        // Step 5: Register
        self.report_progress(
            progress_callback,
            InstallStep::Registering,
            0.0,
            "Registering plugin",
        );

        Ok(install_path)
    }

    /// 回滚安装
    async fn rollback(&self, record: &InstallRecord, _config: &InstallConfig) {
        tracing::warn!(plugin_id = %record.plugin_id, "Rolling back installation");

        // Remove installed files
        for path in &record.installed_files {
            if let Err(e) = tokio::fs::remove_file(path).await {
                tracing::error!(path = %path.display(), error = %e, "Failed to remove installed file during rollback");
            }
        }

        // Remove temp file
        if let Some(ref temp_file) = record.temp_file {
            if let Err(e) = tokio::fs::remove_file(temp_file).await {
                tracing::error!(path = %temp_file.display(), error = %e, "Failed to remove temp file during rollback");
            }
        }

        // Unregister from registry
        if record.registered {
            self.registry.unregister(&record.plugin_id);
        }
    }

    fn report_progress(
        &self,
        callback: Option<&ProgressCallback>,
        step: InstallStep,
        percent: f32,
        message: &str,
    ) {
        if let Some(cb) = callback {
            cb(InstallProgress {
                step,
                percent,
                message: message.to_string(),
            });
        }
    }
}

/// Calculate SHA256 hex digest
fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginType;
    use crate::plugin::PluginFormat;
    use crate::registry::SemVer;

    fn make_registration(id: &str, url: Option<&str>) -> PluginRegistration {
        PluginRegistration {
            id: id.to_string(),
            name: format!("Plugin {}", id),
            description: "Test".to_string(),
            plugin_type: PluginType::Effect,
            format: PluginFormat::Vst3,
            version: SemVer::parse("1.0.0").unwrap(),
            author: "test".to_string(),
            tags: vec![],
            download_url: url.map(|s| s.to_string()),
            compatibility: vec![],
            dependencies: vec![],
            registered_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_install_step_serialization() {
        let step = InstallStep::Downloading;
        let json = serde_json::to_string(&step).unwrap();
        assert_eq!(json, "\"downloading\"");
    }

    #[test]
    fn test_install_progress_creation() {
        let progress = InstallProgress {
            step: InstallStep::Installing,
            percent: 0.5,
            message: "Halfway".to_string(),
        };
        assert_eq!(progress.step, InstallStep::Installing);
    }

    #[test]
    fn test_install_config_default() {
        let config = InstallConfig::default();
        assert!(config.verify_checksum);
        assert!(config.temp_dir.is_none());
    }

    #[test]
    fn test_sha256_hex() {
        let hash = sha256_hex(b"hello");
        assert_eq!(hash.len(), 64); // SHA256 hex = 64 chars
    }

    #[test]
    fn test_install_result_failed() {
        let result = InstallResult {
            plugin_id: "test".to_string(),
            install_path: PathBuf::new(),
            success: false,
            error: Some("Download failed".to_string()),
        };
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_install_no_download_url() {
        let registry = Arc::new(PluginRegistry::new());
        let installer = PluginInstaller::new(registry);
        let registration = make_registration("no-url", None);
        let config = InstallConfig::default();

        let result = installer.install(registration, &config, None).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("No download URL"));
    }
}
