use super::{active_round, load_thesis, send_email};
use crate::auth::AuthUser;
use crate::{ApiError, ApiResult, AppState};
use axum::extract::{Multipart, Path, State};
use axum::routing::post;
use axum::{Json, Router};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/theses", post(create_thesis))
        .route("/api/theses/{id}/upload", post(upload_version))
}

#[derive(Deserialize)]
struct CreateThesisReq {
    title: String,
}

async fn create_thesis(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateThesisReq>,
) -> ApiResult<Json<Value>> {
    user.require_role("student")?;
    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::bad_request("论文题目不能为空"));
    }
    let advisor_id: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT advisor_id FROM users WHERE id = ?")
            .bind(user.id)
            .fetch_optional(&state.db)
            .await?;
    let advisor_id = advisor_id
        .and_then(|r| r.0)
        .ok_or_else(|| ApiError::bad_request("当前学生未指定导师，无法创建论文"))?;
    let res = sqlx::query(
        "INSERT INTO theses (student_id, advisor_id, title, status, created_at) VALUES (?,?,?,'draft',?)",
    )
    .bind(user.id)
    .bind(advisor_id)
    .bind(title)
    .bind(crate::now())
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "id": res.last_insert_rowid() })))
}

/// 学生上传稿件：始终生成新版本。
/// 若已有进行中的送审轮次，新版本不会替换评阅人收到的冻结稿，
/// 是否以新稿重新送审由秘书决定。
async fn upload_version(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> ApiResult<Json<Value>> {
    user.require_role("student")?;
    let thesis = load_thesis(&state.db, id).await?;
    if thesis.student_id != user.id {
        return Err(ApiError::forbidden("只能给自己的论文上传稿件"));
    }

    let mut file_name: Option<String> = None;
    let mut data: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("上传解析失败: {e}")))?
    {
        if field.name() == Some("file") {
            file_name = field.file_name().map(|s| s.to_string());
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(format!("读取上传内容失败: {e}")))?;
            data = Some(bytes.to_vec());
        }
    }
    let orig_filename = file_name.ok_or_else(|| ApiError::bad_request("缺少上传文件"))?;
    let data = data.ok_or_else(|| ApiError::bad_request("缺少上传文件"))?;
    if data.is_empty() {
        return Err(ApiError::bad_request("上传文件为空"));
    }
    if data.len() > 50 * 1024 * 1024 {
        return Err(ApiError::bad_request("文件超过 50MB 限制"));
    }

    let seq: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM versions WHERE thesis_id = ?",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    let mut rand_bytes = [0u8; 8];
    rand::rng().fill_bytes(&mut rand_bytes);
    let stored_name = format!("thesis_{id}/v{}_{}.bin", seq.0, hex::encode(rand_bytes));
    let path = state.upload_dir.join(&stored_name);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, &data).await?;

    let res = sqlx::query(
        "INSERT INTO versions (thesis_id, seq, orig_filename, stored_name, size, uploaded_at) \
         VALUES (?,?,?,?,?,?)",
    )
    .bind(id)
    .bind(seq.0)
    .bind(&orig_filename)
    .bind(&stored_name)
    .bind(data.len() as i64)
    .bind(crate::now())
    .execute(&state.db)
    .await?;
    let version_id = res.last_insert_rowid();
    sqlx::query("UPDATE theses SET current_version_id = ? WHERE id = ?")
        .bind(version_id)
        .bind(id)
        .execute(&state.db)
        .await?;

    // 若上传发生在送审期间，提示秘书存在更新的稿件（是否重新送审由秘书决定）
    if let Some(round) = active_round(&state.db, id).await? {
        if matches!(round.status.as_str(), "frozen" | "in_review" | "completed") {
            let secretaries: Vec<(i64,)> =
                sqlx::query_as("SELECT id FROM users WHERE role = 'secretary'")
                    .fetch_all(&state.db)
                    .await?;
            for (sid,) in secretaries {
                send_email(
                    &state.db,
                    sid,
                    "学生在送审期间上传了新稿",
                    &format!(
                        "论文《{}》(id={}) 在送审期间上传了第 {} 版。第 {} 轮送审仍使用已冻结的第 {} 版；如需以新稿送审，请创建新的送审轮次。",
                        thesis.title, id, seq.0, round.round_no,
                        super::load_version(&state.db, round.version_id).await?.seq
                    ),
                )
                .await?;
            }
        }
    }

    Ok(Json(json!({
        "version_id": version_id,
        "seq": seq.0,
        "orig_filename": orig_filename,
    })))
}
