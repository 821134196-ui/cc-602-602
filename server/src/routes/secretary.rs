use super::{active_round, load_round, load_thesis, load_version, send_email, Round};
use crate::auth::AuthUser;
use crate::{ApiError, ApiResult, AppState};
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/secretary/reviewers", get(list_reviewers))
        .route("/api/theses/{id}/rounds", post(create_round))
        .route("/api/rounds/{id}/check", post(check_round))
        .route("/api/rounds/{id}/scan", get(scan_round))
        .route("/api/rounds/{id}/assign", post(assign_reviewers))
        .route("/api/rounds/{id}/release", post(release_round))
}

const ALIASES: &[&str] = &["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛"];

/// 秘书视图：包含评阅人真实身份与分配记录（仅秘书可见）
pub async fn round_admin_view(db: &sqlx::SqlitePool, round: &Round) -> ApiResult<Value> {
    let version = load_version(db, round.version_id).await?;
    let rows: Vec<(i64, i64, String, String, String)> = sqlx::query_as(
        "SELECT a.id, a.reviewer_id, a.alias, a.created_at, u.display_name \
         FROM assignments a JOIN users u ON u.id = a.reviewer_id \
         WHERE a.round_id = ? ORDER BY a.id",
    )
    .bind(round.id)
    .fetch_all(db)
    .await?;
    let mut assignments = Vec::new();
    for (assignment_id, reviewer_id, alias, assigned_at, reviewer_name) in rows {
        let report: Option<(Option<i64>, Option<String>, String, String)> = sqlx::query_as(
            "SELECT score, recommendation, comment, submitted_at FROM reports WHERE assignment_id = ?",
        )
        .bind(assignment_id)
        .fetch_optional(db)
        .await?;
        assignments.push(json!({
            "assignment_id": assignment_id,
            "alias": alias,
            "reviewer_id": reviewer_id,
            "reviewer_name": reviewer_name,
            "assigned_at": assigned_at,
            "report": report.map(|(score, recommendation, comment, submitted_at)| json!({
                "score": score, "recommendation": recommendation,
                "comment": comment, "submitted_at": submitted_at,
            })),
        }));
    }
    Ok(json!({
        "id": round.id,
        "round_no": round.round_no,
        "status": round.status,
        "version_id": round.version_id,
        "version_seq": version.seq,
        "version_filename": version.orig_filename,
        "anonymity_comment": round.anonymity_comment,
        "created_at": round.created_at,
        "released_at": round.released_at,
        "assignments": assignments,
    }))
}

async fn list_reviewers(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    user.require_role("secretary")?;
    let rows: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT id, username, display_name FROM users WHERE role = 'reviewer' ORDER BY id")
            .fetch_all(&state.db)
            .await?;
    let reviewers: Vec<Value> = rows
        .into_iter()
        .map(|(id, username, display_name)| {
            json!({"id": id, "username": username, "display_name": display_name})
        })
        .collect();
    Ok(Json(json!({ "reviewers": reviewers })))
}

/// 创建送审轮次并冻结当前版本。
/// 若已有进行中的轮次且学生上传了新稿，秘书可调用本接口以最新稿重新送审：
/// 旧轮次标记为 superseded（分配记录保留备查），评阅人随即失去旧稿下载权限。
async fn create_round(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    user.require_role("secretary")?;
    let thesis = load_thesis(&state.db, id).await?;
    let version_id = thesis
        .current_version_id
        .ok_or_else(|| ApiError::bad_request("学生尚未上传稿件"))?;

    let active = active_round(&state.db, id).await?;
    match (&thesis.status[..], &active) {
        ("confirmed", None) => {}
        (_, Some(r)) if r.version_id != version_id => {
            // 重新送审：作废旧轮次
            sqlx::query("UPDATE rounds SET status = 'superseded' WHERE id = ?")
                .bind(r.id)
                .execute(&state.db)
                .await?;
        }
        _ => {
            return Err(ApiError::conflict(
                "当前没有可送审的新稿（需导师确认，或学生上传新版本后再重新送审）",
            ))
        }
    }

    let round_no: (i64,) =
        sqlx::query_as("SELECT COALESCE(MAX(round_no), 0) + 1 FROM rounds WHERE thesis_id = ?")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    let res = sqlx::query(
        "INSERT INTO rounds (thesis_id, version_id, round_no, status, created_at) VALUES (?,?,?,'checking',?)",
    )
    .bind(id)
    .bind(version_id)
    .bind(round_no.0)
    .bind(crate::now())
    .execute(&state.db)
    .await?;
    sqlx::query("UPDATE theses SET status = 'checking' WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "round_id": res.last_insert_rowid(), "round_no": round_no.0 })))
}

#[derive(Deserialize)]
struct CheckReq {
    passed: bool,
    comment: Option<String>,
}

/// 匿名检查：通过后冻结版本；不通过则退回，学生需修改后重新上传、导师重新确认
async fn check_round(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<CheckReq>,
) -> ApiResult<Json<Value>> {
    user.require_role("secretary")?;
    let round = load_round(&state.db, id).await?;
    if round.status != "checking" {
        return Err(ApiError::conflict("该轮次不在待检查状态"));
    }
    let thesis = load_thesis(&state.db, round.thesis_id).await?;
    if req.passed {
        sqlx::query(
            "UPDATE rounds SET status = 'frozen', anonymity_comment = ?, checked_by = ?, checked_at = ? WHERE id = ?",
        )
        .bind(&req.comment)
        .bind(user.id)
        .bind(crate::now())
        .bind(id)
        .execute(&state.db)
        .await?;
        sqlx::query("UPDATE theses SET status = 'frozen' WHERE id = ?")
            .bind(thesis.id)
            .execute(&state.db)
            .await?;
    } else {
        sqlx::query(
            "UPDATE rounds SET status = 'failed', anonymity_comment = ?, checked_by = ?, checked_at = ? WHERE id = ?",
        )
        .bind(&req.comment)
        .bind(user.id)
        .bind(crate::now())
        .bind(id)
        .execute(&state.db)
        .await?;
        sqlx::query("UPDATE theses SET status = 'draft' WHERE id = ?")
            .bind(thesis.id)
            .execute(&state.db)
            .await?;
        send_email(
            &state.db,
            thesis.student_id,
            "匿名检查未通过",
            &format!(
                "论文《{}》第 {} 轮送审的匿名检查未通过：{}。请修改后重新上传并联系导师确认。",
                thesis.title,
                round.round_no,
                req.comment.clone().unwrap_or_default()
            ),
        )
        .await?;
    }
    Ok(Json(json!({ "ok": true })))
}

/// 匿名内容扫描（辅助）：在稿件文本中查找学生/导师姓名等敏感词，结果仅供秘书参考
async fn scan_round(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    user.require_role("secretary")?;
    let round = load_round(&state.db, id).await?;
    let thesis = load_thesis(&state.db, round.thesis_id).await?;
    let version = load_version(&state.db, round.version_id).await?;
    let names: Vec<(String,)> = sqlx::query_as(
        "SELECT display_name FROM users WHERE id IN (?, ?) UNION SELECT username FROM users WHERE id IN (?, ?)",
    )
    .bind(thesis.student_id)
    .bind(thesis.advisor_id)
    .bind(thesis.student_id)
    .bind(thesis.advisor_id)
    .fetch_all(&state.db)
    .await?;
    let path = state.upload_dir.join(&version.stored_name);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| ApiError::not_found("附件文件缺失"))?;
    let text = String::from_utf8_lossy(&bytes);
    let mut findings = Vec::new();
    for (term,) in names {
        let count = text.matches(&term).count();
        if count > 0 {
            findings.push(json!({ "term": term, "count": count }));
        }
    }
    Ok(Json(json!({ "findings": findings, "clean": findings.is_empty() })))
}

#[derive(Deserialize)]
struct AssignReq {
    reviewer_ids: Vec<i64>,
}

/// 分配评阅人：生成匿名别名，通知评阅人
async fn assign_reviewers(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<AssignReq>,
) -> ApiResult<Json<Value>> {
    user.require_role("secretary")?;
    let round = load_round(&state.db, id).await?;
    if round.status != "frozen" {
        return Err(ApiError::conflict("仅匿名检查通过（已冻结）的轮次可分配评阅人"));
    }
    if req.reviewer_ids.is_empty() {
        return Err(ApiError::bad_request("至少选择一名评阅人"));
    }
    let thesis = load_thesis(&state.db, round.thesis_id).await?;
    let existing: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM assignments WHERE round_id = ?")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    for (i, rid) in req.reviewer_ids.iter().enumerate() {
        let reviewer: Option<(i64, String)> =
            sqlx::query_as("SELECT id, display_name FROM users WHERE id = ? AND role = 'reviewer'")
                .bind(rid)
                .fetch_optional(&state.db)
                .await?;
        let (_, reviewer_name) = reviewer
            .ok_or_else(|| ApiError::bad_request(format!("评阅人 {rid} 不存在")))?;
        let alias = format!(
            "评阅人{}",
            ALIASES
                .get((existing.0 as usize) + i)
                .copied()
                .unwrap_or("?")
        );
        sqlx::query(
            "INSERT INTO assignments (round_id, reviewer_id, alias, created_at) VALUES (?,?,?,?)",
        )
        .bind(id)
        .bind(rid)
        .bind(&alias)
        .bind(crate::now())
        .execute(&state.db)
        .await?;
        send_email(
            &state.db,
            *rid,
            "新的评阅任务",
            &format!("您被分配评阅一篇论文（轮次 #{}，{}），请登录系统下载稿件并提交评阅意见。", round.round_no, alias),
        )
        .await?;
        let _ = reviewer_name;
    }
    sqlx::query("UPDATE rounds SET status = 'in_review' WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    sqlx::query("UPDATE theses SET status = 'in_review' WHERE id = ?")
        .bind(thesis.id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// 评阅结束后向学生开放意见
async fn release_round(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    user.require_role("secretary")?;
    let round = load_round(&state.db, id).await?;
    if round.status != "completed" {
        return Err(ApiError::conflict("所有评阅人提交意见后才能开放"));
    }
    let thesis = load_thesis(&state.db, round.thesis_id).await?;
    sqlx::query("UPDATE rounds SET status = 'released', released_at = ? WHERE id = ?")
        .bind(crate::now())
        .bind(id)
        .execute(&state.db)
        .await?;
    sqlx::query("UPDATE theses SET status = 'released' WHERE id = ?")
        .bind(thesis.id)
        .execute(&state.db)
        .await?;
    send_email(
        &state.db,
        thesis.student_id,
        "评阅意见已开放",
        &format!("论文《{}》第 {} 轮评阅意见已开放，请登录系统查看。", thesis.title, round.round_no),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}
