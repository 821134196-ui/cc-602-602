use axum::{
    extract::{Multipart, Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    current_freeze, load_thesis, thesis_json_anonymized, user_display_name,
};
use crate::auth::{now, AuthUser};
use crate::error::ApiError;
use crate::models::{status, Role, Thesis};
use crate::notify::send_email;
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateThesisReq {
    title: String,
    advisor_id: i64,
}

pub async fn create_thesis(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateThesisReq>,
) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Student)?;
    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest("论文题目不能为空".into()));
    }
    // 校验导师身份真实有效
    let advisor_ok: Option<i64> =
        sqlx::query_scalar("SELECT id FROM users WHERE id = ? AND role = 'advisor'")
            .bind(req.advisor_id)
            .fetch_optional(&state.pool)
            .await?;
    if advisor_ok.is_none() {
        return Err(ApiError::BadRequest("所选导师不存在".into()));
    }
    let created_at = now();
    let id = sqlx::query(
        "INSERT INTO theses (student_id, advisor_id, title, status, created_at)
         VALUES (?,?,?,?,?)",
    )
    .bind(auth.0.id)
    .bind(req.advisor_id)
    .bind(title)
    .bind(status::UPLOADED)
    .bind(&created_at)
    .execute(&state.pool)
    .await?
    .last_insert_rowid();

    send_email(
        &state.pool,
        req.advisor_id,
        "学生提交了论文待确认",
        &format!("学生 {} 提交了论文《{}》，请登录系统确认。", auth.0.display_name, title),
    )
    .await;

    Ok(Json(json!({ "id": id })))
}

pub async fn my_theses(auth: AuthUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Student)?;
    let theses = sqlx::query_as::<_, Thesis>(
        "SELECT * FROM theses WHERE student_id = ? ORDER BY id",
    )
    .bind(auth.0.id)
    .fetch_all(&state.pool)
    .await?;
    let mut out = Vec::new();
    for t in &theses {
        let advisor_name = user_display_name(&state.pool, t.advisor_id).await?;
        out.push(thesis_json_anonymized(&state.pool, t, &advisor_name).await?);
    }
    Ok(Json(json!(out)))
}

/// 学生上传新版本。
/// 关键约束：只新增版本记录，绝不改动已冻结的送审稿，
/// 评阅人继续下载冻结时的文件；是否用新版本重新送审由秘书决定。
pub async fn upload_version(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(thesis_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    auth.require(Role::Student)?;
    let thesis = load_thesis(&state.pool, thesis_id).await?;
    if thesis.student_id != auth.0.id {
        return Err(ApiError::Forbidden);
    }

    let mut filename: Option<String> = None;
    let mut stored_name: Option<String> = None;
    let mut size: i64 = 0;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("上传解析失败: {e}")))?
    {
        if field.name() == Some("file") {
            let orig = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "thesis.bin".to_string());
            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::BadRequest(format!("读取上传内容失败: {e}")))?;
            if data.is_empty() {
                return Err(ApiError::BadRequest("文件内容为空".into()));
            }
            // 磁盘文件名使用随机 UUID，不暴露可猜测的路径
            let name = format!("{}.bin", Uuid::new_v4());
            let path = state.upload_dir.join(&name);
            tokio::fs::write(&path, &data).await?;
            size = data.len() as i64;
            filename = Some(orig);
            stored_name = Some(name);
        }
    }

    let (filename, stored_name) = match (filename, stored_name) {
        (Some(f), Some(s)) => (f, s),
        _ => return Err(ApiError::BadRequest("缺少文件字段 file".into())),
    };

    let next_no: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_no), 0) + 1 FROM versions WHERE thesis_id = ?",
    )
    .bind(thesis_id)
    .fetch_one(&state.pool)
    .await?;

    let uploaded_at = now();
    let version_id = sqlx::query(
        "INSERT INTO versions (thesis_id, version_no, filename, stored_name, size_bytes, uploaded_at)
         VALUES (?,?,?,?,?,?)",
    )
    .bind(thesis_id)
    .bind(next_no)
    .bind(&filename)
    .bind(&stored_name)
    .bind(size)
    .bind(&uploaded_at)
    .execute(&state.pool)
    .await?
    .last_insert_rowid();

    // 若已有送审冻结，提示秘书存在未送审的新版本（是否重新分配由秘书决定）
    if let Some(freeze) = current_freeze(&state.pool, thesis_id).await? {
        let secretaries = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM users WHERE role = 'secretary'",
        )
        .fetch_all(&state.pool)
        .await?;
        for sec_id in secretaries {
            send_email(
                &state.pool,
                sec_id,
                "论文在送审后有新版本上传",
                &format!(
                    "论文《{}》(T-{:04}) 上传了第 {} 版；当前送审稿仍为第 {} 版（冻结 #{}）。如需以新版本送审，请重新冻结并分配。",
                    thesis.title, thesis.id, next_no,
                    sqlx::query_scalar::<_, i64>("SELECT version_no FROM versions WHERE id = ?")
                        .bind(freeze.version_id)
                        .fetch_one(&state.pool)
                        .await
                        .unwrap_or(0),
                    freeze.id
                ),
            )
            .await;
        }
    } else {
        send_email(
            &state.pool,
            thesis.advisor_id,
            "学生上传了论文新版本",
            &format!("学生 {} 为论文《{}》上传了第 {} 版。", auth.0.display_name, thesis.title, next_no),
        )
        .await;
    }

    Ok(Json(json!({
        "id": version_id,
        "version_no": next_no,
        "filename": filename,
        "uploaded_at": uploaded_at,
    })))
}
