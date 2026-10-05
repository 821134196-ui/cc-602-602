use sqlx::SqlitePool;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS users (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  username TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  role TEXT NOT NULL CHECK(role IN ('student','advisor','secretary','reviewer')),
  display_name TEXT NOT NULL,
  advisor_id INTEGER REFERENCES users(id)
);

CREATE TABLE IF NOT EXISTS theses (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  student_id INTEGER NOT NULL REFERENCES users(id),
  advisor_id INTEGER NOT NULL REFERENCES users(id),
  title TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft',
  current_version_id INTEGER,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS versions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  thesis_id INTEGER NOT NULL REFERENCES theses(id),
  seq INTEGER NOT NULL,
  orig_filename TEXT NOT NULL,
  stored_name TEXT NOT NULL,
  size INTEGER NOT NULL,
  uploaded_at TEXT NOT NULL
);

-- 一轮送审：秘书冻结某个版本后创建，后续重传不影响该轮冻结版本
CREATE TABLE IF NOT EXISTS rounds (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  thesis_id INTEGER NOT NULL REFERENCES theses(id),
  version_id INTEGER NOT NULL REFERENCES versions(id),
  round_no INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'checking',
  anonymity_comment TEXT,
  checked_by INTEGER,
  checked_at TEXT,
  created_at TEXT NOT NULL,
  released_at TEXT
);

CREATE TABLE IF NOT EXISTS assignments (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  round_id INTEGER NOT NULL REFERENCES rounds(id),
  reviewer_id INTEGER NOT NULL REFERENCES users(id),
  alias TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(round_id, reviewer_id)
);

CREATE TABLE IF NOT EXISTS reports (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  assignment_id INTEGER NOT NULL UNIQUE REFERENCES assignments(id),
  score INTEGER,
  recommendation TEXT,
  comment TEXT NOT NULL,
  submitted_at TEXT NOT NULL
);

-- 模拟邮件：所有“发出”的邮件落库，便于演示与查验
CREATE TABLE IF NOT EXISTS emails (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  to_user_id INTEGER NOT NULL,
  subject TEXT NOT NULL,
  body TEXT NOT NULL,
  sent_at TEXT NOT NULL
);
"#;

pub async fn migrate(db: &SqlitePool) -> Result<(), sqlx::Error> {
    for stmt in SCHEMA.split(';') {
        let s = stmt.trim();
        if !s.is_empty() {
            sqlx::query(s).execute(db).await?;
        }
    }
    Ok(())
}

/// 首次启动写入演示账号（所有账号密码均为 password123）
pub async fn seed(db: &SqlitePool) -> Result<(), sqlx::Error> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(db)
        .await?;
    if count.0 > 0 {
        return Ok(());
    }
    let hash = crate::auth::hash_password("password123");
    let users: &[(&str, &str, &str, Option<&str>)] = &[
        ("secretary", "secretary", "王秘书", None),
        ("advisor1", "advisor", "张明远 导师", None),
        ("advisor2", "advisor", "李慧 导师", None),
        ("student1", "student", "陈晨", Some("advisor1")),
        ("student2", "student", "刘洋", Some("advisor2")),
        ("reviewer1", "reviewer", "赵评审", None),
        ("reviewer2", "reviewer", "钱评审", None),
        ("reviewer3", "reviewer", "孙评审", None),
    ];
    for (username, role, display, advisor) in users {
        let advisor_id: Option<i64> = match advisor {
            Some(a) => {
                sqlx::query_as("SELECT id FROM users WHERE username = ?")
                    .bind(a)
                    .fetch_optional(db)
                    .await?
                    .map(|(id,): (i64,)| id)
            }
            None => None,
        };
        sqlx::query(
            "INSERT INTO users (username, password_hash, role, display_name, advisor_id) VALUES (?,?,?,?,?)",
        )
        .bind(username)
        .bind(&hash)
        .bind(role)
        .bind(display)
        .bind(advisor_id)
        .execute(db)
        .await?;
    }
    Ok(())
}
