use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;

use crate::auth::hash_password;

pub async fn connect(path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    SqlitePoolOptions::new().connect_with(opts).await
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL CHECK (role IN ('student','advisor','secretary','reviewer')),
    display_name  TEXT NOT NULL,
    email         TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    token      TEXT PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS theses (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    student_id INTEGER NOT NULL REFERENCES users(id),
    advisor_id INTEGER NOT NULL REFERENCES users(id),
    title      TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'uploaded',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS versions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    thesis_id    INTEGER NOT NULL REFERENCES theses(id),
    version_no   INTEGER NOT NULL,
    filename     TEXT NOT NULL,
    stored_name  TEXT NOT NULL,
    size_bytes   INTEGER NOT NULL,
    uploaded_at  TEXT NOT NULL
);

-- 送审冻结记录：某次送审实际使用的稿件版本，冻结后不可被后续上传覆盖
CREATE TABLE IF NOT EXISTS freezes (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    thesis_id      INTEGER NOT NULL REFERENCES theses(id),
    version_id     INTEGER NOT NULL REFERENCES versions(id),
    frozen_by      INTEGER NOT NULL REFERENCES users(id),
    frozen_at      TEXT NOT NULL,
    anonymity_note TEXT,
    released_at    TEXT
);

CREATE TABLE IF NOT EXISTS assignments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    freeze_id   INTEGER NOT NULL REFERENCES freezes(id),
    reviewer_id INTEGER NOT NULL REFERENCES users(id),
    assigned_by INTEGER NOT NULL REFERENCES users(id),
    assigned_at TEXT NOT NULL,
    UNIQUE (freeze_id, reviewer_id)
);

CREATE TABLE IF NOT EXISTS reviews (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    assignment_id  INTEGER NOT NULL UNIQUE REFERENCES assignments(id),
    comment        TEXT NOT NULL,
    recommendation TEXT NOT NULL,
    score          INTEGER,
    submitted_at   TEXT NOT NULL
);

-- 模拟邮件：所有外发通知落库，可在页面“通知”栏查看
CREATE TABLE IF NOT EXISTS notifications (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users(id),
    subject    TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at TEXT NOT NULL
);
"#;

pub async fn init(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(SCHEMA).execute(pool).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    if count == 0 {
        seed(pool).await?;
    }
    Ok(())
}

/// 演示账号（密码均为 123456）
async fn seed(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let users: &[(&str, &str, &str, &str)] = &[
        ("secretary", "secretary", "王秘书", "secretary@example.edu"),
        ("advisor1", "advisor", "张教授", "advisor1@example.edu"),
        ("advisor2", "advisor", "刘教授", "advisor2@example.edu"),
        ("student1", "student", "李同学", "student1@example.edu"),
        ("student2", "student", "王同学", "student2@example.edu"),
        ("reviewer1", "reviewer", "赵评阅", "reviewer1@example.edu"),
        ("reviewer2", "reviewer", "钱评阅", "reviewer2@example.edu"),
        ("reviewer3", "reviewer", "孙评阅", "reviewer3@example.edu"),
    ];
    for (username, role, display_name, email) in users {
        let hash = hash_password("123456", username);
        sqlx::query(
            "INSERT INTO users (username, password_hash, role, display_name, email)
             VALUES (?,?,?,?,?)",
        )
        .bind(username)
        .bind(format!("{username}${hash}"))
        .bind(role)
        .bind(display_name)
        .bind(email)
        .execute(pool)
        .await?;
    }
    Ok(())
}
