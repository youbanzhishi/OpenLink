//! # 路由规则管理处理器
//!
//! POST/PUT/DELETE /v1/links/:code/routes
//!
//! Phase 2: 支持多条件组合（AND/OR）

use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use openlink_core::{Action, Condition, ConditionLogic, Route, Rule, Target};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    /// 单条件（向后兼容 Phase 1）
    pub condition: Option<ConditionInput>,
    /// 多条件列表（Phase 2: AND/OR 组合）
    #[serde(default)]
    pub conditions: Vec<ConditionInput>,
    /// 条件组合逻辑（Phase 2: and/or）
    #[serde(default)]
    pub condition_logic: Option<String>,
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

impl From<ConditionInput> for Condition {
    fn from(input: ConditionInput) -> Self {
        Condition {
            condition_type: input.condition_type,
            params: input.params,
        }
    }
}

/// 目标输入
#[derive(Debug, Deserialize)]
pub struct TargetInput {
    pub action: Action,
    #[serde(default)]
    pub params: serde_json::Value,
}

impl From<TargetInput> for Target {
    fn from(input: TargetInput) -> Self {
        Target {
            action: input.action,
            params: input.params,
        }
    }
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

/// 将 RuleInput 转换为 Rule
fn rule_input_to_rule(input: RuleInput) -> Rule {
    let condition = input
        .condition
        .map(Condition::from)
        .unwrap_or_else(|| Condition {
            condition_type: "always".to_string(),
            params: serde_json::Value::Null,
        });

    let conditions: Vec<Condition> = input.conditions.into_iter().map(Condition::from).collect();

    let condition_logic = match input.condition_logic.as_deref() {
        Some("or") => ConditionLogic::Or,
        _ => ConditionLogic::And,
    };

    Rule {
        condition,
        conditions,
        condition_logic,
        target: Target::from(input.target),
        priority: input.priority,
    }
}

/// 创建路由规则
///
/// POST /v1/routes
pub async fn create_route(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRouteRequest>,
) -> Result<(StatusCode, Json<RouteResponse>), (StatusCode, String)> {
    // 获取 link_id 从请求中（假设必填）
    let link_id = req
        .default_target
        .params
        .get("link_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "Missing link_id in default_target".to_string(),
            )
        })?
        .to_string();

    // 验证链接存在
    state
        .store
        .get_link(&link_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Link '{}' not found", link_id),
            )
        })?;

    // 检查是否已有路由
    if state
        .store
        .get_route_by_link_id(&link_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_some()
    {
        return Err((
            StatusCode::CONFLICT,
            "Route already exists for this link".to_string(),
        ));
    }

    // 构建路由
    let rules: Vec<Rule> = req.rules.into_iter().map(rule_input_to_rule).collect();

    let route = Route {
        id: uuid::Uuid::new_v4().to_string(),
        link_id,
        rules,
        default_target: Target::from(req.default_target),
        version: 1,
        created_at: chrono::Utc::now(),
    };

    let created = state
        .store
        .create_route(&route)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(route_id = %created.id, "Route created");
    Ok((StatusCode::CREATED, Json(RouteResponse::from(created))))
}

/// 更新路由规则
///
/// PUT /v1/routes/:id
pub async fn update_route(
    State(state): State<Arc<AppState>>,
    Path(route_id): Path<String>,
    Json(req): Json<UpdateRouteRequest>,
) -> Result<Json<RouteResponse>, (StatusCode, String)> {
    // 获取当前路由
    let current = state
        .store
        .get_route(&route_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Route not found".to_string()))?;

    // 合并更新
    let rules: Vec<Rule> = if req.rules.is_empty() {
        current.rules
    } else {
        req.rules.into_iter().map(rule_input_to_rule).collect()
    };

    let default_target = req
        .default_target
        .map(Target::from)
        .unwrap_or(current.default_target);

    let updated = state
        .store
        .update_route(&Route {
            id: current.id,
            link_id: current.link_id,
            rules,
            default_target,
            version: current.version,
            created_at: current.created_at,
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(route_id = %route_id, "Route updated");
    Ok(Json(RouteResponse::from(updated)))
}

/// 删除路由规则
///
/// DELETE /v1/routes/:id
pub async fn delete_route(
    State(state): State<Arc<AppState>>,
    Path(route_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .store
        .delete_route(&route_id)
        .await
        .map_err(|e| match e {
            openlink_store::StoreError::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        })?;

    tracing::info!(route_id = %route_id, "Route deleted");
    Ok(StatusCode::NO_CONTENT)
}
