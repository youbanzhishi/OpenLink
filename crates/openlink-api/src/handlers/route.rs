//! # 路由规则管理处理器
//!
//! POST/PUT/DELETE /v1/links/:code/routes

use axum::{
    extract::{State, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use openlink_core::{Route, Rule, Target, Action, Condition};
use openlink_store::Store;
use crate::state::AppState;

/// 创建路由规则请求
#[derive(Debug, Deserialize)]
pub struct CreateRouteRequest {
    /// 路由规则列表
    #[serde(default)]
    pub rules: Vec<RuleInput>,
    /// 兜底目标
    pub default_target: TargetInput,
}

/// 规则输入
#[derive(Debug, Deserialize)]
pub struct RuleInput {
    pub condition: ConditionInput,
    pub target: TargetInput,
    #[serde(default)]
    pub priority: i32,
}

/// 条件输入
#[derive(Debug, Deserialize)]
pub struct ConditionInput {
    #[serde(rename = "type")]
    pub condition_type: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// 目标输入
#[derive(Debug, Deserialize)]
pub struct TargetInput {
    pub action: Action,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// 路由响应
#[derive(Debug, Serialize)]
pub struct RouteResponse {
    pub id: String,
    pub link_id: String,
    pub rules: Vec<Rule>,
    pub default: Target,
    pub version: i32,
}

impl From<Route> for RouteResponse {
    fn from(route: Route) -> Self {
        Self {
            id: route.id,
            link_id: route.link_id,
            rules: route.rules,
            default: route.default_target,
            version: route.version,
        }
    }
}

/// 更新路由规则请求
#[derive(Debug, Deserialize)]
pub struct UpdateRouteRequest {
    #[serde(default)]
    pub rules: Vec<RuleInput>,
    pub default_target: Option<TargetInput>,
}

/// 创建路由规则
///
/// POST /v1/links/:code/routes
pub async fn create_route(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Json(req): Json<CreateRouteRequest>,
) -> Result<(StatusCode, Json<RouteResponse>), (StatusCode, String)> {
    // 查找链接
    let link = state
        .store
        .get_link_by_code(&code)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Link '{}' not found", code)))?;

    // 检查是否已有路由
    if state.store.get_route_by_link_id(&link.id).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.is_some() {
        return Err((StatusCode::CONFLICT, "Route already exists for this link".to_string()));
    }

    // 构建路由
    let rules: Vec<Rule> = req
        .rules
        .into_iter()
        .map(|r| Rule {
            condition: Condition {
                condition_type: r.condition.condition_type,
                params: r.condition.params,
            },
            target: Target {
                action: r.target.action,
                params: r.target.params,
            },
            priority: r.priority,
        })
        .collect();

    let route = Route {
        id: uuid::Uuid::new_v4().to_string(),
        link_id: link.id,
        rules,
        default_target: Target {
            action: req.default_target.action,
            params: req.default_target.params,
        },
        version: 1,
        created_at: chrono::Utc::now(),
    };

    let created = state
        .store
        .create_route(&route)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(code = %code, route_id = %created.id, "Route created");
    Ok((StatusCode::CREATED, Json(RouteResponse::from(created))))
}

/// 更新路由规则
///
/// PUT /v1/links/:code/routes/:route_id
pub async fn update_route(
    State(state): State<Arc<AppState>>,
    Path((code, route_id)): Path<(String, String)>,
    Json(req): Json<UpdateRouteRequest>,
) -> Result<Json<RouteResponse>, (StatusCode, String)> {
    // 验证链接存在
    let link = state
        .store
        .get_link_by_code(&code)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Link '{}' not found", code)))?;

    // 获取当前路由
    let current = state
        .store
        .get_route_by_link_id(&link.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Route not found".to_string()))?;

    // 合并更新
    let rules: Vec<Rule> = if req.rules.is_empty() {
        current.rules
    } else {
        req.rules
            .into_iter()
            .map(|r| Rule {
                condition: Condition {
                    condition_type: r.condition.condition_type,
                    params: r.condition.params,
                },
                target: Target {
                    action: r.target.action,
                    params: r.target.params,
                },
                priority: r.priority,
            })
            .collect()
    };

    let default_target = req
        .default_target
        .map(|t| Target {
            action: t.action,
            params: t.params,
        })
        .unwrap_or(current.default_target);

    let updated = state
        .store
        .update_route(&route_id, &Route {
            id: current.id,
            link_id: current.link_id,
            rules,
            default_target,
            version: current.version,
            created_at: current.created_at,
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(code = %code, route_id = %route_id, "Route updated");
    Ok(Json(RouteResponse::from(updated)))
}

/// 删除路由规则
///
/// DELETE /v1/links/:code/routes/:route_id
pub async fn delete_route(
    State(state): State<Arc<AppState>>,
    Path((code, route_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    // 验证链接存在
    let _link = state
        .store
        .get_link_by_code(&code)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Link '{}' not found", code)))?;

    state
        .store
        .delete_route(&route_id)
        .await
        .map_err(|e| {
            match e {
                openlink_store::StoreError::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            }
        })?;

    tracing::info!(code = %code, route_id = %route_id, "Route deleted");
    Ok(StatusCode::NO_CONTENT)
}
