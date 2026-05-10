//! # DAW 深链接模块
//!
//! 实现 DAW 项目深链接协议，支持拉起 OpenDAW 应用。

use serde::{Deserialize, Serialize};

/// DAW 深链接类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DawDeeplinkType {
    /// 打开项目
    OpenProject,
    /// 新建项目
    NewProject,
    /// 打开插件
    OpenPlugin,
    /// 执行脚本
    RunScript,
}

/// DAW 深链接
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DawDeeplink {
    /// 深链接类型
    pub deeplink_type: DawDeeplinkType,
    /// 项目 URL
    pub project_url: Option<String>,
    /// 插件 ID
    pub plugin_id: Option<String>,
    /// 脚本内容
    pub script: Option<String>,
}

impl DawDeeplink {
    /// 从 URL 构建深链接
    pub fn from_url(url: &str) -> Result<Self, DeeplinkError> {
        let url = url.trim_start_matches("opendaw://");
        let parts: Vec<&str> = url.splitn(2, '?').collect();
        let action = parts.get(0).unwrap_or(&"");

        match *action {
            "project" => {
                let params = parts.get(1).map(|s| {
                    s.split('&')
                        .filter_map(|kv| {
                            let mut iter = kv.splitn(2, '=');
                            Some((iter.next()?, iter.next()?))
                        })
                        .collect::<std::collections::HashMap<&str, &str>>()
                });

                Ok(Self {
                    deeplink_type: DawDeeplinkType::OpenProject,
                    project_url: params.and_then(|p| {
                        p.get("url")
                            .map(|s| urlencoding::decode(s).unwrap().to_string())
                    }),
                    plugin_id: None,
                    script: None,
                })
            }
            "plugin" => Ok(Self {
                deeplink_type: DawDeeplinkType::OpenPlugin,
                project_url: None,
                plugin_id: parts.get(1).map(|s| s.to_string()),
                script: None,
            }),
            _ => Err(DeeplinkError::UnknownAction(action.to_string())),
        }
    }

    /// 转换为 URL
    pub fn to_url(&self) -> String {
        match self.deeplink_type {
            DawDeeplinkType::OpenProject => {
                if let Some(ref url) = self.project_url {
                    format!("opendaw://project?url={}", urlencoding::encode(url))
                } else {
                    "opendaw://new_project".to_string()
                }
            }
            DawDeeplinkType::OpenPlugin => {
                if let Some(ref id) = self.plugin_id {
                    format!("opendaw://plugin/{}", id)
                } else {
                    "opendaw://new_project".to_string()
                }
            }
            DawDeeplinkType::RunScript => "opendaw://run_script".to_string(),
            DawDeeplinkType::NewProject => "opendaw://new_project".to_string(),
        }
    }
}

/// 深链接错误
#[derive(Debug, thiserror::Error)]
pub enum DeeplinkError {
    #[error("Unknown action: {0}")]
    UnknownAction(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deeplink_from_url() {
        let url = "opendaw://project?url=https%3A%2F%2Fexample.com%2Fproj.opendaw";
        let deeplink = DawDeeplink::from_url(url).unwrap();
        assert!(matches!(
            deeplink.deeplink_type,
            DawDeeplinkType::OpenProject
        ));
    }

    #[test]
    fn test_deeplink_to_url() {
        let deeplink = DawDeeplink {
            deeplink_type: DawDeeplinkType::OpenProject,
            project_url: Some("https://example.com/proj.opendaw".to_string()),
            plugin_id: None,
            script: None,
        };
        let url = deeplink.to_url();
        assert!(url.starts_with("opendaw://project?url="));
    }
}
