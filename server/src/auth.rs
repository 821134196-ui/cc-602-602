use crate::{ApiError, AppState};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use rand::RngCore;
use sha2::{Digest, Sha256};

pub fn hash_password(password: &str) -> String {
    // 演示系统使用加盐 SHA-256；生产环境应换为 argon2/bcrypt
    let mut h = Sha256::new();
    h.update(b"thesis-review-demo-salt:");
    h.update(password.as_bytes());
    hex::encode(h.finalize())
}

pub fn new_token() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub display_name: String,
}

impl AuthUser {
    pub fn require_role(&self, role: &str) -> Result<(), ApiError> {
        if self.role != role {
            return Err(ApiError::forbidden("当前角色无权执行此操作"));
        }
        Ok(())
    }
}

fn parse_cookie(header: &str, name: &str) -> Option<String> {
    for part in header.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let cookie_header = parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "未登录".into()))?;
        let token = parse_cookie(cookie_header, "session")
            .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "未登录".into()))?;
        let user_id = {
            let sessions = state.sessions.read().unwrap();
            *sessions
                .get(&token)
                .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "会话已失效，请重新登录".into()))?
        };
        let row: Option<(i64, String, String, String)> = sqlx::query_as(
            "SELECT id, username, role, display_name FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?;
        let (id, username, role, display_name) =
            row.ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "用户不存在".into()))?;
        Ok(AuthUser {
            id,
            username,
            role,
            display_name,
        })
    }
}
