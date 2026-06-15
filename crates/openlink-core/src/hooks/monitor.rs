//! MonitorHook - 监控执行分离协议
//!
//! 核心原则：监控Hook只观测不阻塞，返回建议而非命令
//! - MonitorHook返回建议类型（continue/warn/suggest_abort）
//! - 主流程可忽略MonitorHook的建议
//! - 多个MonitorHook一致建议abort时，主流程才考虑终止
//! - 默认不启用MonitorHook（向后兼容）

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// 监控建议类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorAdvice {
    /// 继续，一切正常
    Continue,
    /// 警告，但不建议终止
    Warn(String),
    /// 建议终止（主流程可忽略）
    SuggestAbort(String),
}

/// MonitorHook观测上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorContext {
    /// 被监控的Link ID
    pub link_id: String,

    /// 路由阶段
    pub phase: String,

    /// 已执行时长(ms)
    pub elapsed_ms: u64,

    /// 自定义指标
    pub metrics: HashMap<String, f64>,
}

/// MonitorHook trait - 只观测不阻塞
#[async_trait::async_trait]
pub trait MonitorHook: Send + Sync {
    /// Hook名称
    fn name(&self) -> &str;

    /// 执行监控观测
    async fn observe(&self, ctx: &MonitorContext) -> MonitorAdvice;

    /// 是否启用
    fn is_enabled(&self) -> bool;
}

/// 监控校验引擎 - 聚合多个MonitorHook的建议
#[derive(Debug, Clone)]
pub struct MonitorEngine {
    hooks: Arc<RwLock<Vec<Arc<dyn MonitorHook>>>>,
    /// abort建议阈值：超过此数量的MonitorHook建议abort时，引擎返回abort
    abort_threshold: usize,
    /// 是否全局启用MonitorHook
    enabled: bool,
}

impl MonitorEngine {
    pub fn new(abort_threshold: usize) -> Self {
        Self {
            hooks: Arc::new(RwLock::new(Vec::new())),
            abort_threshold,
            enabled: false, // 默认不启用
        }
    }

    /// 启用监控
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// 禁用监控
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// 是否已启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 注册MonitorHook
    pub async fn register(&self, hook: Arc<dyn MonitorHook>) {
        let mut hooks = self.hooks.write().await;
        hooks.push(hook);
    }

    /// 注销MonitorHook
    pub async fn unregister(&self, name: &str) {
        let mut hooks = self.hooks.write().await;
        hooks.retain(|h| h.name() != name);
    }

    /// 运行所有MonitorHook并聚合建议
    pub async fn evaluate(&self, ctx: &MonitorContext) -> MonitorAdvice {
        if !self.enabled {
            return MonitorAdvice::Continue;
        }

        let hooks = self.hooks.read().await;
        let mut abort_count = 0;
        let mut warnings = Vec::new();
        let mut abort_reasons = Vec::new();

        for hook in hooks.iter() {
            if !hook.is_enabled() {
                continue;
            }

            let advice = hook.observe(ctx).await;
            match &advice {
                MonitorAdvice::Continue => {}
                MonitorAdvice::Warn(msg) => {
                    warnings.push(format!("[{}] {}", hook.name(), msg));
                }
                MonitorAdvice::SuggestAbort(msg) => {
                    abort_count += 1;
                    abort_reasons.push(format!("[{}] {}", hook.name(), msg));
                }
            }
        }

        if abort_count >= self.abort_threshold {
            MonitorAdvice::SuggestAbort(format!(
                "{} hooks suggest abort: {}",
                abort_count,
                abort_reasons.join("; ")
            ))
        } else if !warnings.is_empty() {
            MonitorAdvice::Warn(warnings.join("; "))
        } else {
            MonitorAdvice::Continue
        }
    }

    /// 获取已注册Hook数量
    pub async fn hook_count(&self) -> usize {
        self.hooks.read().await.len()
    }
}

/// 示例：延迟监控Hook - 检测路由执行延迟
#[derive(Debug)]
pub struct LatencyMonitorHook {
    /// 延迟警告阈值(ms)
    warn_threshold_ms: u64,
    /// 延迟abort阈值(ms)
    abort_threshold_ms: u64,
    /// 是否启用
    enabled: bool,
}

impl LatencyMonitorHook {
    pub fn new(warn_threshold_ms: u64, abort_threshold_ms: u64) -> Self {
        Self {
            warn_threshold_ms,
            abort_threshold_ms,
            enabled: true,
        }
    }
}

#[async_trait::async_trait]
impl MonitorHook for LatencyMonitorHook {
    fn name(&self) -> &str {
        "latency_monitor"
    }

    async fn observe(&self, ctx: &MonitorContext) -> MonitorAdvice {
        if ctx.elapsed_ms >= self.abort_threshold_ms {
            MonitorAdvice::SuggestAbort(format!(
                "Latency {}ms exceeds abort threshold {}ms",
                ctx.elapsed_ms, self.abort_threshold_ms
            ))
        } else if ctx.elapsed_ms >= self.warn_threshold_ms {
            MonitorAdvice::Warn(format!(
                "Latency {}ms exceeds warn threshold {}ms",
                ctx.elapsed_ms, self.warn_threshold_ms
            ))
        } else {
            MonitorAdvice::Continue
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// 示例：错误率监控Hook
#[derive(Debug)]
pub struct ErrorRateMonitorHook {
    /// 错误率字段名
    metric_key: String,
    /// 警告阈值
    warn_threshold: f64,
    /// abort阈值
    abort_threshold: f64,
    /// 是否启用
    enabled: bool,
}

impl ErrorRateMonitorHook {
    pub fn new(metric_key: &str, warn_threshold: f64, abort_threshold: f64) -> Self {
        Self {
            metric_key: metric_key.to_string(),
            warn_threshold,
            abort_threshold,
            enabled: true,
        }
    }
}

#[async_trait::async_trait]
impl MonitorHook for ErrorRateMonitorHook {
    fn name(&self) -> &str {
        "error_rate_monitor"
    }

    async fn observe(&self, ctx: &MonitorContext) -> MonitorAdvice {
        if let Some(&rate) = ctx.metrics.get(&self.metric_key) {
            if rate >= self.abort_threshold {
                MonitorAdvice::SuggestAbort(format!(
                    "Error rate {:.2}% exceeds abort threshold {:.2}%",
                    rate * 100.0, self.abort_threshold * 100.0
                ))
            } else if rate >= self.warn_threshold {
                MonitorAdvice::Warn(format!(
                    "Error rate {:.2}% exceeds warn threshold {:.2}%",
                    rate * 100.0, self.warn_threshold * 100.0
                ))
            } else {
                MonitorAdvice::Continue
            }
        } else {
            MonitorAdvice::Continue
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_monitor_engine_disabled() {
        let engine = MonitorEngine::new(2);
        let ctx = MonitorContext {
            link_id: "test".into(),
            phase: "before_route".into(),
            elapsed_ms: 5000,
            metrics: HashMap::new(),
        };

        let advice = engine.evaluate(&ctx).await;
        assert_eq!(advice, MonitorAdvice::Continue);
    }

    #[tokio::test]
    async fn test_latency_monitor_warn() {
        let hook = LatencyMonitorHook::new(1000, 5000);
        let ctx = MonitorContext {
            link_id: "test".into(),
            phase: "before_route".into(),
            elapsed_ms: 2000,
            metrics: HashMap::new(),
        };

        let advice = hook.observe(&ctx).await;
        match advice {
            MonitorAdvice::Warn(_) => {}
            _ => panic!("Expected Warn, got {:?}", advice),
        }
    }

    #[tokio::test]
    async fn test_monitor_engine_abort_threshold() {
        let mut engine = MonitorEngine::new(2); // 需要2个hook建议abort
        engine.enable();

        let hook1 = Arc::new(LatencyMonitorHook::new(100, 200));
        let hook2 = Arc::new(ErrorRateMonitorHook::new("error_rate", 0.1, 0.2));

        engine.register(hook1).await;
        engine.register(hook2).await;

        // 高延迟+高错误率 → 两个hook都建议abort
        let mut metrics = HashMap::new();
        metrics.insert("error_rate".into(), 0.5);
        let ctx = MonitorContext {
            link_id: "test".into(),
            phase: "before_route".into(),
            elapsed_ms: 5000,
            metrics,
        };

        let advice = engine.evaluate(&ctx).await;
        match advice {
            MonitorAdvice::SuggestAbort(_) => {}
            other => panic!("Expected SuggestAbort, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_monitor_engine_below_threshold() {
        let mut engine = MonitorEngine::new(2);
        engine.enable();

        let hook = Arc::new(LatencyMonitorHook::new(1000, 5000));
        engine.register(hook).await;

        // 只有一个hook建议abort，未达阈值
        let ctx = MonitorContext {
            link_id: "test".into(),
            phase: "before_route".into(),
            elapsed_ms: 6000,
            metrics: HashMap::new(),
        };

        let advice = engine.evaluate(&ctx).await;
        // 单个hook建议abort，但未达阈值(2)，所以返回warn级别的abort
        // 实际上只有一个hook，abort_count=1 < threshold=2，所以不会聚合为abort
        // 但这个hook返回的是SuggestAbort... 这里逻辑是聚合后判断
        match advice {
            MonitorAdvice::SuggestAbort(_) => panic!("Should not abort with only 1 hook"),
            _ => {} // Warn or Continue
        }
    }
}
