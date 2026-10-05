use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{now, AuthUser};
use crate::error::ApiError;
use crate::models::{Assignment, Role};
use crate::notify::send_email;
use crate::AppState;

/// 评阅人只能看到分配给自己的稿件。
/// 稿件以匿名编号展示，不暴露学生与导师身份。
pub async fn my_assignments(auth: AuthUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Reviewer)?;
    let rows = sqlx::query_as::<
        _,
        (i64, i64, String, i64, String, i64, String),
    >(
        "SELECT a.id, a.freeze_id, a.assigned_at, t.id, t.title, v.id, v.filename
         FROM assignments a
         JOIN freezes f ON f.id = a.freeze_id
         JOIN theses t ON t.id = f.thesis_id
         JOIN versions v ON v.id = f.version_id
         WHERE a.reviewer_id = ?
         ORDER BY a.id DESC",
    )
    .bind(auth.0.id)
    .fetch_all(&state.pool)
    .await?;

    let mut out = Vec::new();
    for (assignment_id, _freeze_id, assigned_at, thesis_id, title, version_id, filename) in rows {
        let review = sqlx::query_as::<_, (String, String, Option<i64>, String)>(
            "SELECT comment, recommendation, score, submitted_at FROM reviews WHERE assignment_id = ?",
        )
        .bind(assignment_id)
        .fetch_optional(&state.pool)
        .await?;
        let review_json = review.map(|(comment, recommendation, score, submitted_at)| {
            json!({
                "comment": comment,
                "recommendation": recommendation,
                "score": score,
                "submitted_at": submitted_at,
            })
        });
        // 匿名编号：不返回学生/导师姓名，只返回题目与文件
        out.push(json!({
            "assignment_id": assignment_id,
            "thesis_code": format!("T-{:04}", thesis_id),
            "title": title,
            "version_id": version_id,
            "filename": filename,
            "assigned_at": assigned_at,
            "submitted": review_json.is_some(),
            "my_review": review_json,
        }));
    }
    Ok(Json(json!(out)))
}

#[derive(Deserialize)]
pub struct ReviewReq {
    comment: String,
    recommendation: String,
    score: Option<i64>,
}

const RECOMMENDATIONS: [&str; 4] = ["pass", "minor_revision", "major_revision", "reject"];

/// 提交/更新评阅意见（意见开放给学生后不可再修改）
pub async fn submit_review(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(assignment_id): Path<i64>,
    Json(req): Json<ReviewReq>,
) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Reviewer)?;
    let assignment = sqlx::query_as::<_, Assignment>(
        "SELECT * FROM assignments WHERE id = ?",
    )
    .bind(assignment_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    // 只能评阅分配给自己的稿件
    if assignment.reviewer_id != auth.0.id {
        return Err(ApiError::Forbidden);
    }
    let released: Option<String> =
        sqlx::query_scalar("SELECT released_at FROM freezes WHERE id = ?")
            .bind(assignment.freeze_id)
            .fetch_optional(&state.pool)
            .await?
            .flatten();
    if released.is_some() {
        return Err(ApiError::BadRequest("该轮意见已开放，不能再修改".into()));
    }
    if req.comment.trim().is_empty() {
        return Err(ApiError::BadRequest("评阅意见不能为空".into()));
    }
    if !RECOMMENDATIONS.contains(&req.recommendation.as_str()) {
        return Err(ApiError::BadRequest("无效的评阅结论".into()));
    }
    if let Some(s) = req.score {
        if !(0..=100).contains(&s) {
            return Err(ApiError::BadRequest("分数须在 0-100 之间".into()));
        }
    }

    sqlx::query(
        "INSERT INTO reviews (assignment_id, comment, recommendation, score, submitted_at)
         VALUES (?,?,?,?,?)
         ON CONFLICT(assignment_id) DO UPDATE SET
            comment = excluded.comment,
            recommendation = excluded.recommendation,
            score = excluded.score,
            submitted_at = excluded.submitted_at",
    )
    .bind(assignment_id)
    .bind(req.comment.trim())
    .bind(&req.recommendation)
    .bind(req.score)
    .bind(now())
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
            "评阅意见已提交",
            &format!("一份评阅意见已提交（任务 #{}），全部完成后可开放给学生。", assignment_id),
        )
        .await;
    }

    Ok(Json(json!({ "ok": true })))
}
