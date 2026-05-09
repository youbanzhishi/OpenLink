//! # 请求日志中间件
//!
//! 记录每个请求的方法、路径、耗时等信息。
//! 可观测内置：每次路由决策都有完整上下文记录。

use axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::Response,
};

/// 请求日志中间件
pub async fn request_logging(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let start = std::time::Instant::now();

    let response = next.run(req).await;

    let elapsed = start.elapsed();
    let status = response.status();

    tracing::info!(
        method = %method,
        path = %path,
        status = %status.as_u16(),
        elapsed_ms = elapsed.as_millis() as u64,
        "Request processed"
    );

    response
}
