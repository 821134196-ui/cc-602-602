use super::{load_round, send_email};
use crate::auth::AuthUser;
use crate::{ApiError, ApiResult, AppState};
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/reviewer/assignments", get(list_assignments))
        .route("/api/assignments/{id}/report", post(submit_report))
}

/// 评阅人工作台：只列出分配给自己的稿件，不暴露学生身份
async fn list_assignments(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    user.require_role("reviewer")?;
    let rows: Vec<(i64, i64, String, i64, String, String, String, i64, i64, Option<i64>)> =
        sqlx::query_as(
            "SELECT a.id, r.id, r.status, r.round_no, t.title, v.orig_filename, a.created_at, \
                    v.id, v.seq, rep.id \
             FROM assignments a \
             JOIN rounds r ON r.id = a.round_id \
             JOIN theses t ON t.id = r.thesis_id \
             JOIN versions v ON v.id = r.version_id \
             LEFT JOIN reports rep ON rep.assignment_id = a.id \
             WHERE a.reviewer_id = ? AND r.status NOT IN ('failed','superseded') \
             ORDER BY a.id DESC",
        )
        .bind(user.id)
        .fetch_all(&state.db)
        .await?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(
            |(assignment_id, round_id, rstatus, round_no, title, filename, assigned_at, version_id, seq, report_id)| {
                json!({
                    "assignment_id": assignment_id,
                    "round_id": round_id,
                    "round_no": round_no,
                    "round_status": rstatus,
                    "title": title,
                    "filename": filename,
                    "version_id": version_id,
                    "version_seq": seq,
                    "assigned_at": assigned_at,
                    "submitted": report_id.is_some(),
                })
            },
        )
        .collect();
    Ok(Json(json!({ "assignments": list })))
}

#[derive(Deserialize)]
struct ReportReq {
    score: Option<i64>,
    recommendation: Option<String>,
    comment: String,
}

async fn submit_report(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<ReportReq>,
) -> ApiResult<Json<Value>> {
    user.require_role("reviewer")?;
    if req.comment.trim().is_empty() {
        return Err(ApiError::bad_request("评阅意见不能为空"));
    }
    if let Some(s) = req.score {
        if !(0..=100).contains(&s) {
            return Err(ApiError::bad_request("评分须在 0-100 之间"));
        }
    }
    // 只能提交分配给自己的任务
    let assignment: Option<(i64, i64)> =
        sqlx::query_as("SELECT id, round_id FROM assignments WHERE id = ? AND reviewer_id = ?")
            .bind(id)
            .bind(user.id)
            .fetch_optional(&state.db)
            .await?;
    let (_, round_id) = assignment.ok_or_else(|| ApiError::forbidden("该评阅任务不属于当前账号"))?;
    let round = load_round(&state.db, round_id).await?;
    if round.status != "in_review" {
        return Err(ApiError::conflict("该轮次当前不接受评阅意见"));
    }
    let existing: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM reports WHERE assignment_id = ?")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    if existing.0 > 0 {
        return Err(ApiError::conflict("已提交过评阅意见"));
    }
    sqlx::query(
        "INSERT INTO reports (assignment_id, score, recommendation, comment, submitted_at) VALUES (?,?,?,?,?)",
    )
    .bind(id)
    .bind(req.score)
    .bind(&req.recommendation)
    .bind(req.comment.trim())
    .bind(crate::now())
    .execute(&state.db)
    .await?;

    // 全部评阅人提交后，轮次自动进入 completed
    let pending: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM assignments a LEFT JOIN reports rep ON rep.assignment_id = a.id \
         WHERE a.round_id = ? AND rep.id IS NULL",
    )
    .bind(round_id)
    .fetch_one(&state.db)
    .await?;
    if pending.0 == 0 {
        sqlx::query("UPDATE rounds SET status = 'completed' WHERE id = ?")
            .bind(round_id)
            .execute(&state.db)
            .await?;
        sqlx::query("UPDATE theses SET status = 'completed' WHERE id = ?")
            .bind(round.thesis_id)
            .execute(&state.db)
            .await?;
        let secretaries: Vec<(i64,)> =
            sqlx::query_as("SELECT id FROM users WHERE role = 'secretary'")
                .fetch_all(&state.db)
                .await?;
        for (sid,) in secretaries {
            send_email(
                &state.db,
                sid,
                "评阅已全部完成",
                &format!("论文 id={} 第 {} 轮评阅已全部提交，可归档并向学生开放意见。", round.thesis_id, round.round_no),
            )
            .await?;
        }
    }
    Ok(Json(json!({ "ok": true })))
}
