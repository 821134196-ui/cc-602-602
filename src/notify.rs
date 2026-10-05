use sqlx::SqlitePool;

use crate::auth::now;

/// 模拟邮件接口：写入 notifications 表并打印日志（不真正外发）
pub async fn send_email(pool: &SqlitePool, user_id: i64, subject: &str, body: &str) {
    if let Err(e) = sqlx::query(
        "INSERT INTO notifications (user_id, subject, body, created_at) VALUES (?,?,?,?)",
    )
    .bind(user_id)
    .bind(subject)
    .bind(body)
    .bind(now())
    .execute(pool)
    .await
    {
        tracing::warn!("模拟邮件写入失败: {e}");
    }
    tracing::info!(target: "mock-mailer", "模拟邮件 -> user#{user_id}: {subject} | {body}");
}
