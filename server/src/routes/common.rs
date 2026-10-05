use super::{active_round, load_thesis, load_version, Assignment, Round, Thesis, Version};
use crate::auth::{hash_password, new_token, AuthUser};
use crate::{ApiError, ApiResult, AppState};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(me))
        .route("/api/theses", get(list_theses))
        .route("/api/theses/{id}", get(thesis_detail))
        .route("/api/files/{version_id}/download", get(download_version))
        .route("/api/emails", get(list_emails))
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> ApiResult<(HeaderMap, Json<Value>)> {
    let row: Option<(i64, String, String, String, String)> = sqlx::query_as(
        "SELECT id, username, password_hash, role, display_name FROM users WHERE username = ?",
    )
    .bind(&req.username)
    .fetch_optional(&state.db)
    .await?;
    let (id, username, password_hash, role, display_name) = row
        .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "用户名或密码错误".into()))?;
    if password_hash != hash_password(&req.password) {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "用户名或密码错误".into()));
    }
    let token = new_token();
    state.sessions.write().unwrap().insert(token.clone(), id);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "session={token}; HttpOnly; Path=/; SameSite=Lax"
        ))
        .unwrap(),
    );
    Ok((
        headers,
        Json(json!({
            "id": id, "username": username, "role": role, "display_name": display_name
        })),
    ))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    if let Some(cookie) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for part in cookie.split(';') {
            if let Some((k, v)) = part.trim().split_once('=') {
                if k.trim() == "session" {
                    state.sessions.write().unwrap().remove(v.trim());
                }
            }
        }
    }
    Ok(Json(json!({ "ok": true })))
}

async fn me(user: AuthUser) -> Json<Value> {
    Json(json!({
        "id": user.id,
        "username": user.username,
        "role": user.role,
        "display_name": user.display_name,
    }))
}

#[derive(FromRow)]
struct ThesisRow {
    #[sqlx(flatten)]
    thesis: Thesis,
    student_name: String,
    advisor_name: String,
}

async fn list_theses(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    let base = "SELECT theses.*, su.display_name AS student_name, au.display_name AS advisor_name \
                FROM theses \
                JOIN users su ON su.id = theses.student_id \
                JOIN users au ON au.id = theses.advisor_id";
    let rows: Vec<ThesisRow> = match user.role.as_str() {
        "student" => {
            sqlx::query_as(&format!("{base} WHERE theses.student_id = ? ORDER BY theses.id DESC"))
                .bind(user.id)
                .fetch_all(&state.db)
                .await?
        }
        "advisor" => {
            sqlx::query_as(&format!("{base} WHERE theses.advisor_id = ? ORDER BY theses.id DESC"))
                .bind(user.id)
                .fetch_all(&state.db)
                .await?
        }
        "secretary" => {
            sqlx::query_as(&format!("{base} ORDER BY theses.id DESC"))
                .fetch_all(&state.db)
                .await?
        }
        _ => return Err(ApiError::forbidden("评阅人请使用评阅工作台")),
    };
    let list: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.thesis.id,
                "title": r.thesis.title,
                "status": r.thesis.status,
                "current_version_id": r.thesis.current_version_id,
                "created_at": r.thesis.created_at,
                "student_name": r.student_name,
                "advisor_name": r.advisor_name,
            })
        })
        .collect();
    Ok(Json(json!({ "theses": list })))
}

/// 轮次的匿名化视图（学生/导师可见）：只含别名，绝不含评阅人身份
async fn round_public_view(db: &sqlx::SqlitePool, round: &Round) -> ApiResult<Value> {
    let version: Version = load_version(db, round.version_id).await?;
    let assignments = sqlx::query_as::<_, Assignment>(
        "SELECT * FROM assignments WHERE round_id = ? ORDER BY id",
    )
    .bind(round.id)
    .fetch_all(db)
    .await?;
    let mut assignment_views = Vec::new();
    let mut submitted = 0i64;
    for a in &assignments {
        let report: Option<(i64, Option<i64>, Option<String>, String, String)> = sqlx::query_as(
            "SELECT id, score, recommendation, comment, submitted_at FROM reports WHERE assignment_id = ?",
        )
        .bind(a.id)
        .fetch_optional(db)
        .await?;
        if let Some((_, score, recommendation, comment, submitted_at)) = report {
            submitted += 1;
            assignment_views.push(json!({
                "alias": a.alias,
                "submitted": true,
                // 意见内容仅在秘书开放后对学生/导师可见，由调用方按轮次状态决定是否包含
                "report": {
                    "score": score,
                    "recommendation": recommendation,
                    "comment": comment,
                    "submitted_at": submitted_at,
                }
            }));
        } else {
            assignment_views.push(json!({ "alias": a.alias, "submitted": false }));
        }
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
        "assignment_count": assignments.len(),
        "submitted_count": submitted,
        "assignments": assignment_views,
    }))
}

async fn thesis_detail(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let thesis = load_thesis(&state.db, id).await?;
    let is_owner = user.role == "student" && thesis.student_id == user.id;
    let is_advisor = user.role == "advisor" && thesis.advisor_id == user.id;
    let is_secretary = user.role == "secretary";
    if !is_owner && !is_advisor && !is_secretary {
        // 评阅人及其他无关人员一律拒绝，不暴露论文是否存在之外的任何信息
        return Err(ApiError::forbidden("无权查看该论文"));
    }

    let versions = sqlx::query_as::<_, Version>(
        "SELECT * FROM versions WHERE thesis_id = ? ORDER BY seq DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let versions_json: Vec<Value> = versions
        .iter()
        .map(|v| {
            json!({
                "id": v.id, "seq": v.seq, "orig_filename": v.orig_filename,
                "size": v.size, "uploaded_at": v.uploaded_at,
            })
        })
        .collect();

    let rounds = sqlx::query_as::<_, Round>(
        "SELECT * FROM rounds WHERE thesis_id = ? ORDER BY round_no DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let mut rounds_json = Vec::new();
    for r in &rounds {
        if is_secretary {
            rounds_json.push(super::secretary::round_admin_view(&state.db, r).await?);
        } else {
            let mut view = round_public_view(&state.db, r).await?;
            // 未开放的轮次，对学生/导师隐藏意见正文，仅展示进度
            if r.status != "released" {
                if let Some(arr) = view.get_mut("assignments").and_then(|a| a.as_array_mut()) {
                    for a in arr.iter_mut() {
                        if let Some(obj) = a.as_object_mut() {
                            obj.remove("report");
                        }
                    }
                }
            }
            rounds_json.push(view);
        }
    }

    // 当前实际送审稿（进行中轮次冻结的版本）
    let active = active_round(&state.db, id).await?;
    let review_version = match &active {
        Some(r) => {
            let v = load_version(&state.db, r.version_id).await?;
            json!({
                "version_id": v.id, "seq": v.seq, "orig_filename": v.orig_filename,
                "uploaded_at": v.uploaded_at, "round_id": r.id, "round_no": r.round_no,
                "round_status": r.status,
            })
        }
        None => Value::Null,
    };

    let mut out = json!({
        "id": thesis.id,
        "title": thesis.title,
        "status": thesis.status,
        "created_at": thesis.created_at,
        "current_version_id": thesis.current_version_id,
        "versions": versions_json,
        "rounds": rounds_json,
        "review_version": review_version,
    });
    if is_secretary || is_advisor {
        let names: (String, String) = sqlx::query_as(
            "SELECT su.display_name, au.display_name FROM theses \
             JOIN users su ON su.id = theses.student_id \
             JOIN users au ON au.id = theses.advisor_id WHERE theses.id = ?",
        )
        .bind(id)
        .fetch_one(&state.db)
        .await?;
        out["student_name"] = json!(names.0);
        out["advisor_name"] = json!(names.1);
    }
    Ok(Json(out))
}

/// 附件下载：按角色逐一核验，杜绝仅靠猜测地址越权下载
async fn download_version(
    State(state): State<AppState>,
    user: AuthUser,
    Path(version_id): Path<i64>,
) -> ApiResult<Response> {
    let version = load_version(&state.db, version_id).await?;
    let thesis = load_thesis(&state.db, version.thesis_id).await?;

    let allowed = match user.role.as_str() {
        "secretary" => true,
        "student" => thesis.student_id == user.id,
        "advisor" => thesis.advisor_id == user.id,
        "reviewer" => {
            // 评阅人只能下载“分配给自己的、未作废轮次”所冻结的那一个版本
            let cnt: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM assignments a \
                 JOIN rounds r ON r.id = a.round_id \
                 WHERE a.reviewer_id = ? AND r.version_id = ? \
                   AND r.status NOT IN ('failed','superseded')",
            )
            .bind(user.id)
            .bind(version_id)
            .fetch_one(&state.db)
            .await?;
            cnt.0 > 0
        }
        _ => false,
    };
    if !allowed {
        return Err(ApiError::forbidden("无权下载该附件"));
    }

    let path = state.upload_dir.join(&version.stored_name);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| ApiError::not_found("附件文件缺失"))?;
    let encoded: String = version
        .orig_filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c.to_string()
            } else {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf);
                buf.iter()
                    .take(c.len_utf8())
                    .map(|b| format!("%{b:02X}"))
                    .collect()
            }
        })
        .collect();
    let mut resp = Response::new(Body::from(bytes));
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"download.bin\"; filename*=UTF-8''{encoded}"
        ))
        .unwrap(),
    );
    Ok(resp)
}

async fn list_emails(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    // 模拟邮件箱：秘书可查看全部，其他角色只能看发给自己的
    let rows: Vec<(i64, i64, String, String, String)> = if user.role == "secretary" {
        sqlx::query_as("SELECT id, to_user_id, subject, body, sent_at FROM emails ORDER BY id DESC")
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_as(
            "SELECT id, to_user_id, subject, body, sent_at FROM emails WHERE to_user_id = ? ORDER BY id DESC",
        )
        .bind(user.id)
        .fetch_all(&state.db)
        .await?
    };
    let emails: Vec<Value> = rows
        .into_iter()
        .map(|(id, to, subject, body, sent_at)| {
            json!({"id": id, "to_user_id": to, "subject": subject, "body": body, "sent_at": sent_at})
        })
        .collect();
    Ok(Json(json!({ "emails": emails })))
}
