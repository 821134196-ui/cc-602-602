pub mod auth;
pub mod routes;
pub mod schema;

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub sessions: Arc<RwLock<HashMap<String, i64>>>,
    pub upload_dir: PathBuf,
}

impl AppState {
    pub fn new(db: SqlitePool, upload_dir: PathBuf) -> Self {
        Self {
            db,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            upload_dir,
        }
    }
}

/// 统一 API 错误：HTTP 状态码 + JSON 消息
pub struct ApiError(pub StatusCode, pub String);

impl ApiError {
    pub fn forbidden(msg: &str) -> Self {
        ApiError(StatusCode::FORBIDDEN, msg.to_string())
    }
    pub fn not_found(msg: &str) -> Self {
        ApiError(StatusCode::NOT_FOUND, msg.to_string())
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        ApiError(StatusCode::BAD_REQUEST, msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        ApiError(StatusCode::CONFLICT, msg.into())
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, msg.into())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::internal(format!("数据库错误: {e}"))
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::internal(format!("文件系统错误: {e}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(serde_json::json!({ "error": self.1 })),
        )
            .into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// 组装完整应用：/api 路由 + 可选的前端静态文件服务
pub fn build_app(state: AppState, web_dist: Option<PathBuf>) -> Router {
    let app = Router::new()
        .merge(routes::router(state.clone()))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024));
    let app = match web_dist {
        Some(dir) => {
            let index = dir.join("index.html");
            app.fallback_service(ServeDir::new(&dir).not_found_service(ServeFile::new(index)))
        }
        None => app,
    };
    app.with_state(state)
}
