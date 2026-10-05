use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};

use super::{load_thesis, thesis_json_anonymized, user_display_name};
use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::models::{status, Role, Thesis};
use crate::notify::send_email;
use crate::AppState;

/// 导师查看自己学生的论文（含进度与已开放的匿名评阅意见）。
/// 不返回任何评阅人身份信息。
pub async fn list_theses(auth: AuthUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Advisor)?;
    let theses = sqlx::query_as::<_, Thesis>(
        "SELECT * FROM theses WHERE advisor_id = ? ORDER BY id",
    )
    .bind(auth.0.id)
    .fetch_all(&state.pool)
    .await?;
    let mut out = Vec::new();
    for t in &theses {
        let mut obj = thesis_json_anonymized(&state.pool, t, &auth.0.display_name).await?;
        let student_name = user_display_name(&state.pool, t.student_id).await?;
        obj.as_object_mut()
            .unwrap()
            .insert("student_name".into(), json!(student_name));
        out.push(obj);
    }
    Ok(Json(json!(out)))
}

/// 导师确认学生的稿件可以进入匿名审查
pub async fn confirm_thesis(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(thesis_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Advisor)?;
    let thesis = load_thesis(&state.pool, thesis_id).await?;
    if thesis.advisor_id != auth.0.id {
        return Err(ApiError::Forbidden);
    }
    if thesis.status != status::UPLOADED {
        return Err(ApiError::BadRequest("当前状态无需确认或已确认过".into()));
    }
    let has_version: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM versions WHERE thesis_id = ?",
    )
    .bind(thesis_id)
    .fetch_one(&state.pool)
    .await?;
    if has_version == 0 {
        return Err(ApiError::BadRequest("学生尚未上传稿件，无法确认".into()));
    }
    sqlx::query("UPDATE theses SET status = ? WHERE id = ?")
        .bind(status::ADVISOR_CONFIRMED)
        .bind(thesis_id)
        .execute(&state.pool)
        .await?;

    let secretaries =
        sqlx::query_scalar::<_, i64>("SELECT id FROM users WHERE role = 'secretary'")
            .fetch_all(&state.pool)
            .await?;
    for sec_id in secretaries {
        send_email(
            &state.pool,
            sec_id,
            "论文待匿名审查",
            &format!("导师 {} 已确认论文《{}》(T-{:04})，请进行匿名内容检查并冻结送审版本。", auth.0.display_name, thesis.title, thesis.id),
        )
        .await;
    }

    Ok(Json(json!({ "ok": true })))
}
