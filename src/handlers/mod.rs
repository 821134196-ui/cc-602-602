pub mod advisor;
pub mod files;
pub mod reviewer;
pub mod secretary;
pub mod student;

use axum::{extract::State, Json};
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::models::{Freeze, Thesis, Version};
use crate::AppState;

/// 导师下拉列表（学生创建论文时选择导师）
pub async fn list_advisors(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, display_name FROM users WHERE role = 'advisor' ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(|(id, name)| json!({ "id": id, "display_name": name }))
        .collect();
    Ok(Json(json!(list)))
}

pub async fn load_thesis(pool: &SqlitePool, thesis_id: i64) -> Result<Thesis, ApiError> {
    sqlx::query_as::<_, Thesis>("SELECT * FROM theses WHERE id = ?")
        .bind(thesis_id)
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn load_versions(pool: &SqlitePool, thesis_id: i64) -> Result<Vec<Version>, ApiError> {
    Ok(sqlx::query_as::<_, Version>(
        "SELECT * FROM versions WHERE thesis_id = ? ORDER BY version_no",
    )
    .bind(thesis_id)
    .fetch_all(pool)
    .await?)
}

/// 当前生效的送审冻结（最近一次）
pub async fn current_freeze(pool: &SqlitePool, thesis_id: i64) -> Result<Option<Freeze>, ApiError> {
    Ok(sqlx::query_as::<_, Freeze>(
        "SELECT * FROM freezes WHERE thesis_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(thesis_id)
    .fetch_optional(pool)
    .await?)
}

fn version_json(v: &Version, review_version_id: Option<i64>) -> Value {
    json!({
        "id": v.id,
        "version_no": v.version_no,
        "filename": v.filename,
        "size_bytes": v.size_bytes,
        "uploaded_at": v.uploaded_at,
        // 标记该版本是否为当前实际送审稿，前端据此区分“最新上传稿”和“实际送审稿”
        "is_review_version": review_version_id == Some(v.id),
    })
}

/// 学生/导师视角的论文详情。
/// 匿名性约束：评阅意见只带“评阅人 N”编号，绝不包含评阅人 id / 姓名 / 账号。
pub async fn thesis_json_anonymized(
    pool: &SqlitePool,
    thesis: &Thesis,
    advisor_name: &str,
) -> Result<Value, ApiError> {
    let versions = load_versions(pool, thesis.id).await?;
    let freeze = current_freeze(pool, thesis.id).await?;
    let review_version_id = freeze.as_ref().map(|f| f.version_id);
    let latest_version_id = versions.last().map(|v| v.id);

    let versions_json: Vec<Value> = versions
        .iter()
        .map(|v| version_json(v, review_version_id))
        .collect();

    // 评阅意见仅在秘书开放后可见，且一律匿名
    let mut reviews: Vec<Value> = Vec::new();
    let mut reviews_released = false;
    if let Some(f) = &freeze {
        if f.released_at.is_some() {
            reviews_released = true;
            reviews = anonymized_reviews(pool, f.id).await?;
        }
    }

    Ok(json!({
        "id": thesis.id,
        "title": thesis.title,
        "status": thesis.status,
        "created_at": thesis.created_at,
        "advisor_name": advisor_name,
        "versions": versions_json,
        "latest_version_id": latest_version_id,
        "review_version_id": review_version_id,
        "reviews_released": reviews_released,
        "reviews": reviews,
    }))
}

/// 某次送审的匿名评阅意见：按分配顺序编号为“评阅人 1 / 2 / ...”
async fn anonymized_reviews(pool: &SqlitePool, freeze_id: i64) -> Result<Vec<Value>, ApiError> {
    let rows = sqlx::query_as::<_, (i64, String, String, Option<i64>, String)>(
        "SELECT a.id, r.comment, r.recommendation, r.score, r.submitted_at
         FROM assignments a JOIN reviews r ON r.assignment_id = a.id
         WHERE a.freeze_id = ? ORDER BY a.id",
    )
    .bind(freeze_id)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for (idx, (_assignment_id, comment, recommendation, score, submitted_at)) in
        rows.into_iter().enumerate()
    {
        out.push(json!({
            "label": format!("评阅人 {}", idx + 1),
            "comment": comment,
            "recommendation": recommendation,
            "score": score,
            "submitted_at": submitted_at,
        }));
    }
    Ok(out)
}

pub async fn user_display_name(pool: &SqlitePool, user_id: i64) -> Result<String, ApiError> {
    let name: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    name.ok_or(ApiError::NotFound)
}
