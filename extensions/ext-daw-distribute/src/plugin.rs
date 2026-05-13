//! # 插件管理模块
//!
//! 管理和分发 DAW 插件（VST3/CLAP/JSFX）。

use serde::{Deserialize, Serialize};

/// 插件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Instrument,
    Effect,
    Analyzer,
}

/// 插件格式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginFormat {
    Vst3,
    Clap,
    Jsfx,
    Au,
}

impl PluginFormat {
    /// 文件扩展名
    pub fn extension(&self) -> &'static str {
        match self {
            PluginFormat::Vst3 => "vst3",
            PluginFormat::Clap => "clap",
            PluginFormat::Jsfx => "jsfx",
            PluginFormat::Au => "component",
        }
    }

    /// 插件目录（标准路径）
    #[cfg(unix)]
    pub fn standard_path(&self) -> &'static str {
        match self {
            PluginFormat::Vst3 => "~/.vst3",
            PluginFormat::Clap => "~/.clap",
            PluginFormat::Jsfx => "~/REAPER/Effects",
            PluginFormat::Au => "~/Library/Audio/Plug-Ins/Components",
        }
    }
}

/// 插件分发状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DistributionStatus {
    Queued,
    Downloading,
    Installing,
    Installed,
    Failed,
    Removed,
}

/// 插件分发记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDistribution {
    pub id: String,
    pub plugin_id: String,
    pub target_device: String,
    pub status: DistributionStatus,
    pub progress_percent: f32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub error_message: Option<String>,
}

impl PluginDistribution {
    pub fn new(plugin_id: String, target_device: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id,
            target_device,
            status: DistributionStatus::Queued,
            progress_percent: 0.0,
            created_at: now,
            updated_at: now,
            error_message: None,
        }
    }

    pub fn update_status(&mut self, status: DistributionStatus, progress: f32) {
        self.status = status;
        self.progress_percent = progress;
        self.updated_at = chrono::Utc::now();
    }
}

/// 验证插件 URL
pub fn validate_plugin_url(url: &str) -> bool {
    url.starts_with("https://") && (url.ends_with(".vst3") || url.ends_with(".clap") || url.ends_with(".jsfx"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_format_extension() {
        assert_eq!(PluginFormat::Vst3.extension(), "vst3");
        assert_eq!(PluginFormat::Clap.extension(), "clap");
        assert_eq!(PluginFormat::Jsfx.extension(), "jsfx");
    }

    #[test]
    fn test_validate_plugin_url() {
        assert!(validate_plugin_url("https://example.com/plugin.vst3"));
        assert!(validate_plugin_url("https://example.com/plugin.clap"));
        assert!(validate_plugin_url("https://example.com/plugin.jsfx"));
        assert!(!validate_plugin_url("http://example.com/plugin.vst3"));
        assert!(!validate_plugin_url("https://example.com/plugin.dll"));
    }

    #[test]
    fn test_distribution_status_transitions() {
        let mut dist = PluginDistribution::new("plugin-1".to_string(), "daw-1".to_string());
        assert_eq!(dist.status, DistributionStatus::Queued);
        assert_eq!(dist.progress_percent, 0.0);

        dist.update_status(DistributionStatus::Downloading, 50.0);
        assert_eq!(dist.status, DistributionStatus::Downloading);
        assert_eq!(dist.progress_percent, 50.0);

        dist.update_status(DistributionStatus::Installed, 100.0);
        assert_eq!(dist.status, DistributionStatus::Installed);
        assert_eq!(dist.progress_percent, 100.0);
    }
}
