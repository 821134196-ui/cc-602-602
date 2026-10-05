use axum::{
    extract::{FromRequestParts, State},
    http::{header, request::Parts},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::{Role, User};
use crate::AppState;

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn hash_password(password: &str, salt: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b":");
    h.update(password.as_bytes());
    hex::encode(h.finalize())
}

fn verify_password(stored: &str, password: &str) -> bool {
    match stored.split_once('$') {
        Some((salt, hash)) => hash_password(password, salt) == hash,
        None => false,
    }
}

pub fn user_json(user: &User) -> Value {
    json!({
        "id": user.id,
        "username": user.username,
        "role": user.role,
        "display_name": user.display_name,
        "email": user.email,
    })
}

/// 登录态：从 Authorization: Bearer <token> 解析出当前用户
pub struct AuthUser(pub User);

impl AuthUser {
    pub fn require(&self, role: Role) -> Result<(), ApiError> {
        if self.0.role == role {
            Ok(())
        } else {
            Err(ApiError::Forbidden)
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let header_value = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;
        let token = header_value
            .strip_prefix("Bearer ")
            .ok_or(ApiError::Unauthorized)?;
        let user = sqlx::query_as::<_, User>(
            "SELECT u.* FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token = ?",
        )
        .bind(token)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(ApiError::Unauthorized)?;
        Ok(AuthUser(user))
    }
}

#[derive(Deserialize)]
pub struct LoginReq {
    username: String,
    password: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<Value>, ApiError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
        .bind(&req.username)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !verify_password(&user.password_hash, &req.password) {
        return Err(ApiError::Unauthorized);
    }
    let token = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO sessions (token, user_id, created_at) VALUES (?,?,?)")
        .bind(&token)
        .bind(user.id)
        .bind(now())
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "token": token, "user": user_json(&user) })))
}

pub async fn logout(auth: AuthUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(auth.0.id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn me(auth: AuthUser) -> Json<Value> {
    Json(user_json(&auth.0))
}

/// 当前用户收到的模拟邮件
pub async fn notifications(auth: AuthUser, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT id, subject, body, created_at FROM notifications WHERE user_id = ? ORDER BY id DESC LIMIT 50",
    )
    .bind(auth.0.id)
    .fetch_all(&state.pool)
    .await?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(|(id, subject, body, created_at)| {
            json!({ "id": id, "subject": subject, "body": body, "created_at": created_at })
        })
        .collect();
    Ok(Json(json!(list)))
}
