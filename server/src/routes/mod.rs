pub mod advisor;
pub mod common;
pub mod reviewer;
pub mod secretary;
pub mod student;

use crate::{ApiError, ApiResult, AppState};
use axum::Router;
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Thesis {
    pub id: i64,
    pub student_id: i64,
    pub advisor_id: i64,
    pub title: String,
    pub status: String,
    pub current_version_id: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Version {
    pub id: i64,
    pub thesis_id: i64,
    pub seq: i64,
    pub orig_filename: String,
    pub stored_name: String,
    pub size: i64,
    pub uploaded_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Round {
    pub id: i64,
    pub thesis_id: i64,
    pub version_id: i64,
    pub round_no: i64,
    pub status: String,
    pub anonymity_comment: Option<String>,
    pub checked_by: Option<i64>,
    pub checked_at: Option<String>,
    pub created_at: String,
    pub released_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Assignment {
    pub id: i64,
    pub round_id: i64,
    pub reviewer_id: i64,
    pub alias: String,
    pub created_at: String,
}

pub async fn load_thesis(db: &sqlx::SqlitePool, id: i64) -> ApiResult<Thesis> {
    sqlx::query_as::<_, Thesis>("SELECT * FROM theses WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| ApiError::not_found("论文不存在"))
}

pub async fn load_version(db: &sqlx::SqlitePool, id: i64) -> ApiResult<Version> {
    sqlx::query_as::<_, Version>("SELECT * FROM versions WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| ApiError::not_found("稿件版本不存在"))
}

pub async fn load_round(db: &sqlx::SqlitePool, id: i64) -> ApiResult<Round> {
    sqlx::query_as::<_, Round>("SELECT * FROM rounds WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| ApiError::not_found("送审轮次不存在"))
}

/// 论文当前进行中的轮次（failed / superseded 视为已结束）
pub async fn active_round(db: &sqlx::SqlitePool, thesis_id: i64) -> ApiResult<Option<Round>> {
    let r = sqlx::query_as::<_, Round>(
        "SELECT * FROM rounds WHERE thesis_id = ? AND status NOT IN ('failed','superseded') \
         ORDER BY round_no DESC LIMIT 1",
    )
    .bind(thesis_id)
    .fetch_optional(db)
    .await?;
    Ok(r)
}

/// 模拟邮件发送：写入 emails 表并输出日志
pub async fn send_email(
    db: &sqlx::SqlitePool,
    to_user_id: i64,
    subject: &str,
    body: &str,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO emails (to_user_id, subject, body, sent_at) VALUES (?,?,?,?)")
        .bind(to_user_id)
        .bind(subject)
        .bind(body)
        .bind(crate::now())
        .execute(db)
        .await?;
    tracing::info!("[mock-email] to_user={} subject={}", to_user_id, subject);
    Ok(())
}

pub fn router(state: AppState) -> Router<AppState> {
    let _ = state;
    Router::new()
        .merge(common::router())
        .merge(student::router())
        .merge(advisor::router())
        .merge(secretary::router())
        .merge(reviewer::router())
}
