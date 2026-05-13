//! # ProjectShare — 项目分享
//!
//! 支持项目打包、深链接生成、权限控制。

use serde::{Deserialize, Serialize};

/// 项目分享权限
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SharePermission {
    /// 公开
    Public,
    /// 私有（仅创建者）
    Private,
    /// 团队（指定成员）
    Team,
}

/// 项目分享请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareProjectRequest {
    /// 项目 ID
    pub project_id: String,
    /// 项目名称
    pub project_name: String,
    /// 项目描述
    #[serde(default)]
    pub description: String,
    /// 项目 URL 或内容
    pub project_url: String,
    /// 权限
    #[serde(default = "default_permission")]
    pub permission: SharePermission,
    /// 团队成员（Team 权限时使用）
    #[serde(default)]
    pub team_members: Vec<String>,
    /// 过期时间（秒），0 表示永不过期
    #[serde(default)]
    pub ttl_secs: u64,
}

fn default_permission() -> SharePermission {
    SharePermission::Public
}

/// 项目分享信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedProject {
    /// 分享 ID
    pub id: String,
    /// 项目 ID
    pub project_id: String,
    /// 项目名称
    pub project_name: String,
    /// 项目描述
    #[serde(default)]
    pub description: String,
    /// DAW 深链接
    pub deeplink: String,
    /// 权限
    pub permission: SharePermission,
    /// 分享码（短链）
    pub share_code: String,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 过期时间
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 项目打包结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBundle {
    /// 打包 ID
    pub bundle_id: String,
    /// 项目 ID
    pub project_id: String,
    /// 依赖列表
    pub dependencies: Vec<String>,
    /// 资源文件列表
    pub resources: Vec<String>,
    /// 打包大小（字节）
    pub size: u64,
    /// 下载 URL
    pub download_url: String,
}

/// 项目分享管理器
pub struct ProjectShareManager {
    shares: std::sync::Mutex<Vec<SharedProject>>,
}

impl ProjectShareManager {
    pub fn new() -> Self {
        Self {
            shares: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// 分享项目
    pub fn share_project(&self, request: ShareProjectRequest) -> SharedProject {
        let share_id = uuid::Uuid::new_v4().to_string();
        let share_code = openlink_core::generate_default();
        let now = chrono::Utc::now();

        // Build DAW deep link
        let deeplink = if request.project_url.starts_with("http") {
            format!("opendaw://project?url={}", urlencoding::encode(&request.project_url))
        } else {
            format!("opendaw://project/{}", request.project_id)
        };

        let expires_at = if request.ttl_secs > 0 {
            Some(now + chrono::Duration::seconds(request.ttl_secs as i64))
        } else {
            None
        };

        let shared = SharedProject {
            id: share_id,
            project_id: request.project_id,
            project_name: request.project_name,
            description: request.description,
            deeplink,
            permission: request.permission,
            share_code,
            created_at: now,
            expires_at,
        };

        self.shares.lock().unwrap().push(shared.clone());
        shared
    }

    /// 获取分享的项目
    pub fn get_share(&self, share_id: &str) -> Option<SharedProject> {
        self.shares
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == share_id || s.share_code == share_id)
            .cloned()
    }

    /// 检查访问权限
    pub fn check_access(&self, share_id: &str, _user_id: &str) -> bool {
        if let Some(share) = self.get_share(share_id) {
            match share.permission {
                SharePermission::Public => true,
                SharePermission::Private => false, // Only creator (not tracked here)
                SharePermission::Team => {
                    // For team shares, check if user is a team member
                    // In a real implementation, this would check the team_members list
                    // For now, just return true for team shares
                    true
                }
            }
        } else {
            false
        }
    }

    /// 打包项目
    pub fn bundle_project(
        &self,
        project_id: &str,
        dependencies: Vec<String>,
        resources: Vec<String>,
        download_url: String,
        size: u64,
    ) -> ProjectBundle {
        ProjectBundle {
            bundle_id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.to_string(),
            dependencies,
            resources,
            size,
            download_url,
        }
    }

    /// 列出所有分享
    pub fn list_shares(&self) -> Vec<SharedProject> {
        self.shares.lock().unwrap().clone()
    }

    /// 删除分享
    pub fn delete_share(&self, share_id: &str) -> bool {
        let mut shares = self.shares.lock().unwrap();
        let before = shares.len();
        shares.retain(|s| s.id != share_id && s.share_code != share_id);
        shares.len() < before
    }
}

impl Default for ProjectShareManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_permission_serialization() {
        let perm = SharePermission::Public;
        let json = serde_json::to_string(&perm).unwrap();
        assert_eq!(json, "\"public\"");

        let perm = SharePermission::Team;
        let json = serde_json::to_string(&perm).unwrap();
        assert_eq!(json, "\"team\"");
    }

    #[test]
    fn test_share_project() {
        let manager = ProjectShareManager::new();
        let request = ShareProjectRequest {
            project_id: "proj-1".to_string(),
            project_name: "My Song".to_string(),
            description: "A cool track".to_string(),
            project_url: "https://example.com/proj-1.opendaw".to_string(),
            permission: SharePermission::Public,
            team_members: vec![],
            ttl_secs: 0,
        };

        let shared = manager.share_project(request);
        assert_eq!(shared.project_name, "My Song");
        assert!(shared.deeplink.starts_with("opendaw://project?url="));
        assert_eq!(shared.permission, SharePermission::Public);
        assert!(shared.expires_at.is_none());
    }

    #[test]
    fn test_share_project_with_ttl() {
        let manager = ProjectShareManager::new();
        let request = ShareProjectRequest {
            project_id: "proj-2".to_string(),
            project_name: "Timed".to_string(),
            description: String::new(),
            project_url: "local:proj-2".to_string(),
            permission: SharePermission::Private,
            team_members: vec![],
            ttl_secs: 3600,
        };

        let shared = manager.share_project(request);
        assert!(shared.expires_at.is_some());
    }

    #[test]
    fn test_get_share_by_id() {
        let manager = ProjectShareManager::new();
        let request = ShareProjectRequest {
            project_id: "proj-3".to_string(),
            project_name: "Found".to_string(),
            description: String::new(),
            project_url: "local:proj-3".to_string(),
            permission: SharePermission::Public,
            team_members: vec![],
            ttl_secs: 0,
        };

        let shared = manager.share_project(request);
        let found = manager.get_share(&shared.id).unwrap();
        assert_eq!(found.project_name, "Found");
    }

    #[test]
    fn test_get_share_by_code() {
        let manager = ProjectShareManager::new();
        let request = ShareProjectRequest {
            project_id: "proj-4".to_string(),
            project_name: "Code".to_string(),
            description: String::new(),
            project_url: "local:proj-4".to_string(),
            permission: SharePermission::Public,
            team_members: vec![],
            ttl_secs: 0,
        };

        let shared = manager.share_project(request);
        let found = manager.get_share(&shared.share_code).unwrap();
        assert_eq!(found.project_name, "Code");
    }

    #[test]
    fn test_check_access_public() {
        let manager = ProjectShareManager::new();
        let request = ShareProjectRequest {
            project_id: "proj-5".to_string(),
            project_name: "Public".to_string(),
            description: String::new(),
            project_url: "local:proj-5".to_string(),
            permission: SharePermission::Public,
            team_members: vec![],
            ttl_secs: 0,
        };

        let shared = manager.share_project(request);
        assert!(manager.check_access(&shared.id, "any-user"));
    }

    #[test]
    fn test_bundle_project() {
        let manager = ProjectShareManager::new();
        let bundle = manager.bundle_project(
            "proj-6",
            vec!["plugin-1".to_string()],
            vec!["audio/wav".to_string()],
            "https://example.com/bundle.tar.gz".to_string(),
            1024,
        );
        assert_eq!(bundle.project_id, "proj-6");
        assert_eq!(bundle.dependencies.len(), 1);
        assert_eq!(bundle.size, 1024);
    }

    #[test]
    fn test_delete_share() {
        let manager = ProjectShareManager::new();
        let request = ShareProjectRequest {
            project_id: "proj-7".to_string(),
            project_name: "Delete Me".to_string(),
            description: String::new(),
            project_url: "local:proj-7".to_string(),
            permission: SharePermission::Public,
            team_members: vec![],
            ttl_secs: 0,
        };

        let shared = manager.share_project(request);
        assert!(manager.delete_share(&shared.id));
        assert!(manager.get_share(&shared.id).is_none());
    }
}
