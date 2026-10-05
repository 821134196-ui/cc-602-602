use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue},
    response::IntoResponse,
};

use super::load_thesis;
use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::models::{Role, Version};
use crate::AppState;

/// 附件统一下载入口。所有角色都按身份校验：
/// - 学生：仅限本人论文的版本
/// - 导师：仅限自己指导的论文
/// - 秘书：全部
/// - 评阅人：仅限分配给自己的那次冻结所对应的版本（后续新版本不可见）
/// 无法通过猜测版本号/地址下载他人论文。
pub async fn download_version(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(version_id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let version = sqlx::query_as::<_, Version>("SELECT * FROM versions WHERE id = ?")
        .bind(version_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    let thesis = load_thesis(&state.pool, version.thesis_id).await?;

    let allowed = match auth.0.role {
        Role::Secretary => true,
        Role::Student => thesis.student_id == auth.0.id,
        Role::Advisor => thesis.advisor_id == auth.0.id,
        Role::Reviewer => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM assignments a
                 JOIN freezes f ON f.id = a.freeze_id
                 WHERE a.reviewer_id = ? AND f.version_id = ?",
            )
            .bind(auth.0.id)
            .bind(version_id)
            .fetch_one(&state.pool)
            .await?;
            count > 0
        }
    };
    if !allowed {
        return Err(ApiError::Forbidden);
    }

    let path = state.upload_dir.join(&version.stored_name);
    let bytes = tokio::fs::read(&path).await.map_err(|_| ApiError::NotFound)?;

    let mut headers = HeaderMap::new();
    let mime = mime_guess::from_path(&version.filename).first_or_octet_stream();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename*=UTF-8''{}",
            percent_encode(&version.filename)
        ))
        .map_err(|_| ApiError::Internal("文件名编码失败".into()))?,
    );
    Ok((headers, bytes))
}

/// Content-Disposition filename* 的百分号编码
fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
