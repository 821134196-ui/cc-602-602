use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use sqlx::SqlitePool;
use std::path::PathBuf;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
};

pub mod auth;
pub mod db;
pub mod error;
pub mod handlers;
pub mod models;
pub mod notify;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// 稿件文件存放目录（不对外静态暴露，一律走带权限校验的下载接口）
    pub upload_dir: PathBuf,
}

/// 只包含 /api 路由，便于测试
pub fn api_router(state: AppState) -> Router {
    Router::new()
        // 认证
        .route("/api/login", post(auth::login))
        .route("/api/logout", post(auth::logout))
        .route("/api/me", get(auth::me))
        .route("/api/notifications", get(auth::notifications))
        .route("/api/advisors", get(handlers::list_advisors))
        // 学生
        .route("/api/student/theses", post(handlers::student::create_thesis))
        .route("/api/student/theses", get(handlers::student::my_theses))
        .route(
            "/api/student/theses/{id}/upload",
            post(handlers::student::upload_version),
        )
        // 导师
        .route("/api/advisor/theses", get(handlers::advisor::list_theses))
        .route(
            "/api/advisor/theses/{id}/confirm",
            post(handlers::advisor::confirm_thesis),
        )
        // 秘书
        .route(
            "/api/secretary/theses",
            get(handlers::secretary::list_theses),
        )
        .route(
            "/api/secretary/reviewers",
            get(handlers::secretary::list_reviewers),
        )
        .route(
            "/api/secretary/theses/{id}/freeze",
            post(handlers::secretary::freeze_version),
        )
        .route(
            "/api/secretary/freezes/{id}/assign",
            post(handlers::secretary::assign_reviewers),
        )
        .route(
            "/api/secretary/freezes/{id}/release",
            post(handlers::secretary::release_reviews),
        )
        // 评阅人
        .route(
            "/api/reviewer/assignments",
            get(handlers::reviewer::my_assignments),
        )
        .route(
            "/api/reviewer/assignments/{id}/review",
            post(handlers::reviewer::submit_review),
        )
        // 附件下载（统一权限校验）
        .route(
            "/api/files/versions/{id}/download",
            get(handlers::files::download_version),
        )
        .with_state(state)
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(CorsLayer::permissive())
}

/// 完整应用：API + 前端静态文件
pub fn build_app(state: AppState, frontend_dist: PathBuf) -> Router {
    let index = frontend_dist.join("index.html");
    api_router(state)
        .fallback_service(ServeDir::new(frontend_dist).fallback(ServeFile::new(index)))
}
