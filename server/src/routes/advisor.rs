use super::{load_thesis, send_email};
use crate::auth::AuthUser;
use crate::{ApiError, ApiResult, AppState};
use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/theses/{id}/confirm", post(confirm_thesis))
}

/// 导师确认当前版本可以送审
async fn confirm_thesis(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    user.require_role("advisor")?;
    let thesis = load_thesis(&state.db, id).await?;
    if thesis.advisor_id != user.id {
        return Err(ApiError::forbidden("只能确认自己指导的论文"));
    }
    if thesis.status != "draft" {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，仅草稿状态可确认送审",
            thesis.status
        )));
    }
    if thesis.current_version_id.is_none() {
        return Err(ApiError::bad_request("学生尚未上传稿件"));
    }
    sqlx::query("UPDATE theses SET status = 'confirmed' WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    let secretaries: Vec<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE role = 'secretary'")
        .fetch_all(&state.db)
        .await?;
    for (sid,) in secretaries {
        send_email(
            &state.db,
            sid,
            "论文待匿名检查",
            &format!("论文《{}》(id={}) 已经导师确认，请进行匿名检查并安排送审。", thesis.title, id),
        )
        .await?;
    }
    Ok(Json(json!({ "ok": true })))
}
