use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum Role {
    Student,
    Advisor,
    Secretary,
    Reviewer,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: Role,
    pub display_name: String,
    pub email: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Thesis {
    pub id: i64,
    pub student_id: i64,
    pub advisor_id: i64,
    pub title: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Version {
    pub id: i64,
    pub thesis_id: i64,
    pub version_no: i64,
    pub filename: String,
    pub stored_name: String,
    pub size_bytes: i64,
    pub uploaded_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Freeze {
    pub id: i64,
    pub thesis_id: i64,
    pub version_id: i64,
    pub frozen_by: i64,
    pub frozen_at: String,
    pub anonymity_note: Option<String>,
    pub released_at: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Assignment {
    pub id: i64,
    pub freeze_id: i64,
    pub reviewer_id: i64,
    pub assigned_by: i64,
    pub assigned_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Review {
    pub id: i64,
    pub assignment_id: i64,
    pub comment: String,
    pub recommendation: String,
    pub score: Option<i64>,
    pub submitted_at: String,
}

// 论文状态机：uploaded -> advisor_confirmed -> frozen -> in_review -> released
pub mod status {
    pub const UPLOADED: &str = "uploaded";
    pub const ADVISOR_CONFIRMED: &str = "advisor_confirmed";
    pub const FROZEN: &str = "frozen";
    pub const IN_REVIEW: &str = "in_review";
    pub const RELEASED: &str = "released";
}
