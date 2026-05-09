//! # OpenLink Edge — 边缘重定向（WASM）
//!
//! Phase 5 将编译为 WASM，部署在 Cloudflare Workers 等边缘节点。
//! 当前为空壳，仅定义模块结构。
//!
//! 目标：极简重定向，将核心路径 GET /:code → 302 编译为 WASM，
//! 在边缘节点实现毫秒级响应。
