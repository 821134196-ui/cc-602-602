use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{current_freeze, load_thesis, load_versions, user_display_name};
use crate::auth::{now, AuthUser};
use crate::error::ApiError;
use crate::models::{status, Assignment, Freeze, Role, Thesis};
use crate::notify::send_email;
use crate::AppState;

/// 秘书查看全部论文：送审版本、冻结历史与分配记录完整可见
pub async fn list_theses(auth: AuthUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Secretary)?;
    let theses = sqlx::query_as::<_, Thesis>("SELECT * FROM theses ORDER BY id")
        .fetch_all(&state.pool)
        .await?;
    let mut out = Vec::new();
    for t in &theses {
        out.push(thesis_json_full(&state, t).await?);
    }
    Ok(Json(json!(out)))
}

async fn thesis_json_full(state: &AppState, t: &Thesis) -> Result<Value, ApiError> {
    let pool = &state.pool;
    let versions = load_versions(pool, t.id).await?;
    let freeze = current_freeze(pool, t.id).await?;
    let review_version_id = freeze.as_ref().map(|f| f.version_id);

    let versions_json: Vec<Value> = versions
        .iter()
        .map(|v| {
            json!({
                "id": v.id,
                "version_no": v.version_no,
                "filename": v.filename,
                "size_bytes": v.size_bytes,
                "uploaded_at": v.uploaded_at,
                "is_review_version": review_version_id == Some(v.id),
            })
        })
        .collect();

    // 全部冻结轮次 + 分配记录 + 评阅意见（秘书可见评阅人身份）
    let freezes = sqlx::query_as::<_, Freeze>(
        "SELECT * FROM freezes WHERE thesis_id = ? ORDER BY id",
    )
    .bind(t.id)
    .fetch_all(pool)
    .await?;
    let mut freezes_json = Vec::new();
    for f in &freezes {
        let version_no: i64 =
            sqlx::query_scalar("SELECT version_no FROM versions WHERE id = ?")
                .bind(f.version_id)
                .fetch_one(pool)
                .await?;
        let assignments = sqlx::query_as::<_, Assignment>(
            "SELECT * FROM assignments WHERE freeze_id = ? ORDER BY id",
        )
        .bind(f.id)
        .fetch_all(pool)
        .await?;
        let mut assignments_json = Vec::new();
        for a in &assignments {
            let reviewer_name = user_display_name(pool, a.reviewer_id).await?;
            let review = sqlx::query_as::<_, (String, String, Option<i64>, String)>(
                "SELECT comment, recommendation, score, submitted_at FROM reviews WHERE assignment_id = ?",
            )
            .bind(a.id)
            .fetch_optional(pool)
            .await?;
            let review_json = review.map(|(comment, recommendation, score, submitted_at)| {
                json!({
                    "comment": comment,
                    "recommendation": recommendation,
                    "score": score,
                    "submitted_at": submitted_at,
                })
            });
            assignments_json.push(json!({
                "id": a.id,
                "reviewer_id": a.reviewer_id,
                "reviewer_name": reviewer_name,
                "assigned_at": a.assigned_at,
                "review": review_json,
            }));
        }
        freezes_json.push(json!({
            "id": f.id,
            "version_id": f.version_id,
            "version_no": version_no,
            "frozen_at": f.frozen_at,
            "anonymity_note": f.anonymity_note,
            "released_at": f.released_at,
            "assignments": assignments_json,
        }));
    }

    let student_name = user_display_name(pool, t.student_id).await?;
    let advisor_name = user_display_name(pool, t.advisor_id).await?;
    let latest_version_id = versions.last().map(|v| v.id);

    Ok(json!({
        "id": t.id,
        "title": t.title,
        "status": t.status,
        "created_at": t.created_at,
        "student_name": student_name,
        "advisor_name": advisor_name,
        "versions": versions_json,
        "latest_version_id": latest_version_id,
        "review_version_id": review_version_id,
        "freezes": freezes_json,
    }))
}

/// 评阅人名单（仅秘书可见，用于分配）
pub async fn list_reviewers(auth: AuthUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Secretary)?;
    let rows = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, display_name FROM users WHERE role = 'reviewer' ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(|(id, name)| json!({ "id": id, "display_name": name }))
        .collect();
    Ok(Json(json!(list)))
}

#[derive(Deserialize)]
pub struct FreezeReq {
    version_id: i64,
    /// 匿名检查结论说明，例如“已确认无作者/导师身份信息”
    anonymity_note: Option<String>,
}

/// 匿名检查通过后冻结送审版本。冻结后该版本文件即固定，
/// 学生后续上传的新版本不会影响评阅人拿到的附件。
pub async fn freeze_version(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(thesis_id): Path<i64>,
    Json(req): Json<FreezeReq>,
) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Secretary)?;
    let thesis = load_thesis(&state.pool, thesis_id).await?;
    if thesis.status == status::UPLOADED {
        return Err(ApiError::BadRequest("导师尚未确认，不能冻结送审".into()));
    }
    // 版本必须属于该论文
    let version_no: Option<i64> = sqlx::query_scalar(
        "SELECT version_no FROM versions WHERE id = ? AND thesis_id = ?",
    )
    .bind(req.version_id)
    .bind(thesis_id)
    .fetch_optional(&state.pool)
    .await?;
    let version_no = version_no.ok_or(ApiError::BadRequest("该版本不属于此论文".into()))?;

    // 不允许把当前送审版本重复冻结
    if let Some(f) = current_freeze(&state.pool, thesis_id).await? {
        if f.version_id == req.version_id {
            return Err(ApiError::Conflict("该版本已是当前送审稿".into()));
        }
    }

    let frozen_at = now();
    let freeze_id = sqlx::query(
        "INSERT INTO freezes (thesis_id, version_id, frozen_by, frozen_at, anonymity_note)
         VALUES (?,?,?,?,?)",
    )
    .bind(thesis_id)
    .bind(req.version_id)
    .bind(auth.0.id)
    .bind(&frozen_at)
    .bind(&req.anonymity_note)
    .execute(&state.pool)
    .await?
    .last_insert_rowid();

    sqlx::query("UPDATE theses SET status = ? WHERE id = ?")
        .bind(status::FROZEN)
        .bind(thesis_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({ "id": freeze_id, "version_no": version_no })))
}

#[derive(Deserialize)]
pub struct AssignReq {
    reviewer_ids: Vec<i64>,
}

/// 将某次冻结分配给评阅人（可追加；重复分配自动忽略）
pub async fn assign_reviewers(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(freeze_id): Path<i64>,
    Json(req): Json<AssignReq>,
) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Secretary)?;
    let freeze = sqlx::query_as::<_, Freeze>("SELECT * FROM freezes WHERE id = ?")
        .bind(freeze_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    if freeze.released_at.is_some() {
        return Err(ApiError::BadRequest("该轮评阅意见已开放，不能再分配".into()));
    }
    if req.reviewer_ids.is_empty() {
        return Err(ApiError::BadRequest("请至少选择一名评阅人".into()));
    }

    let thesis = load_thesis(&state.pool, freeze.thesis_id).await?;
    let assigned_at = now();
    let mut assigned = 0i64;
    for rid in &req.reviewer_ids {
        let ok: Option<i64> =
            sqlx::query_scalar("SELECT id FROM users WHERE id = ? AND role = 'reviewer'")
                .bind(rid)
                .fetch_optional(&state.pool)
                .await?;
        if ok.is_none() {
            return Err(ApiError::BadRequest(format!("用户 {rid} 不是评阅人")));
        }
        let res = sqlx::query(
            "INSERT OR IGNORE INTO assignments (freeze_id, reviewer_id, assigned_by, assigned_at)
             VALUES (?,?,?,?)",
        )
        .bind(freeze_id)
        .bind(rid)
        .bind(auth.0.id)
        .bind(&assigned_at)
        .execute(&state.pool)
        .await?;
        if res.rows_affected() > 0 {
            assigned += 1;
            send_email(
                &state.pool,
                *rid,
                "新的评阅任务",
                &format!("您有新的论文评阅任务（编号 T-{:04}），请登录系统下载稿件并评阅。", thesis.id),
            )
            .await;
        }
    }

    sqlx::query("UPDATE theses SET status = ? WHERE id = ?")
        .bind(status::IN_REVIEW)
        .bind(thesis.id)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({ "assigned": assigned })))
}

/// 评阅结束后向学生与导师开放意见（开放前学生/导师看不到任何评阅内容）
pub async fn release_reviews(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(freeze_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Secretary)?;
    let freeze = sqlx::query_as::<_, Freeze>("SELECT * FROM freezes WHERE id = ?")
        .bind(freeze_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    if freeze.released_at.is_some() {
        return Err(ApiError::Conflict("该轮意见已开放".into()));
    }
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM assignments WHERE freeze_id = ?",
    )
    .bind(freeze_id)
    .fetch_one(&state.pool)
    .await?;
    if total == 0 {
        return Err(ApiError::BadRequest("尚未分配评阅人，不能开放".into()));
    }
    let done: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM assignments a JOIN reviews r ON r.assignment_id = a.id WHERE a.freeze_id = ?",
    )
    .bind(freeze_id)
    .fetch_one(&state.pool)
    .await?;
    if done < total {
        return Err(ApiError::BadRequest(format!(
            "还有 {} 位评阅人未提交意见，不能开放",
            total - done
        )));
    }

    sqlx::query("UPDATE freezes SET released_at = ? WHERE id = ?")
        .bind(now())
        .bind(freeze_id)
        .execute(&state.pool)
        .await?;
    sqlx::query("UPDATE theses SET status = ? WHERE id = ?")
        .bind(status::RELEASED)
        .bind(freeze.thesis_id)
        .execute(&state.pool)
        .await?;

    let thesis = load_thesis(&state.pool, freeze.thesis_id).await?;
    for uid in [thesis.student_id, thesis.advisor_id] {
        send_email(
            &state.pool,
            uid,
            "评阅意见已开放",
            &format!("论文《{}》(T-{:04}) 的评阅意见已开放，请登录系统查看。", thesis.title, thesis.id),
        )
        .await;
    }

    Ok(Json(json!({ "ok": true })))
}
