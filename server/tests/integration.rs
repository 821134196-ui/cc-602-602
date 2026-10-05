use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use thesis_review_server::{build_app, schema, AppState};
use tower::ServiceExt;

struct TestApp {
    app: Router,
    _tmp: TempDir,
}

async fn setup() -> TestApp {
    let tmp = tempfile::tempdir().unwrap();
    let upload_dir = tmp.path().join("uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    schema::migrate(&pool).await.unwrap();
    schema::seed(&pool).await.unwrap();
    let state = AppState::new(pool, upload_dir);
    TestApp {
        app: build_app(state, None),
        _tmp: tmp,
    }
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: Body,
    content_type: Option<&str>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    if let Some(ct) = content_type {
        builder = builder.header(header::CONTENT_TYPE, ct);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}

async fn get(app: &Router, path: &str, cookie: Option<&str>) -> (StatusCode, Vec<u8>) {
    let (s, _, b) = request(app, "GET", path, cookie, Body::empty(), None).await;
    (s, b)
}

async fn post_json(
    app: &Router,
    path: &str,
    cookie: Option<&str>,
    json: &str,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    request(
        app,
        "POST",
        path,
        cookie,
        Body::from(json.to_string()),
        Some("application/json"),
    )
    .await
}

async fn login(app: &Router, username: &str) -> String {
    let (status, headers, body) = post_json(
        app,
        "/api/login",
        None,
        &format!(r#"{{"username":"{username}","password":"password123"}}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login failed: {:?}", body);
    let set_cookie = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    set_cookie.split(';').next().unwrap().to_string()
}

fn multipart(filename: &str, content: &[u8]) -> (String, Body) {
    let boundary = "XTESTBOUNDARY";
    let mut data = Vec::new();
    data.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    data.extend_from_slice(content);
    data.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (
        format!("multipart/form-data; boundary={boundary}"),
        Body::from(data),
    )
}

async fn upload(app: &Router, cookie: &str, thesis_id: i64, filename: &str, content: &[u8]) -> StatusCode {
    let (ct, body) = multipart(filename, content);
    let (s, _, _) = request(
        app,
        "POST",
        &format!("/api/theses/{thesis_id}/upload"),
        Some(cookie),
        body,
        Some(&ct),
    )
    .await;
    s
}

/// 跑通完整流程直到分配评阅人，返回 (thesis_id, round_id, v1_version_id)
async fn flow_until_assigned(app: &Router) -> (i64, i64, i64, String, String, String) {
    let student = login(app, "student1").await;
    let advisor = login(app, "advisor1").await;
    let secretary = login(app, "secretary").await;

    let (s, _, b) = post_json(app, "/api/theses", Some(&student), r#"{"title":"论匿名评审系统"}"#).await;
    assert_eq!(s, StatusCode::OK);
    let thesis_id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"]
        .as_i64()
        .unwrap();

    let s = upload(app, &student, thesis_id, "thesis.txt", b"VERSION-1 anonymous content").await;
    assert_eq!(s, StatusCode::OK);

    let (s, _, _) = post_json(app, &format!("/api/theses/{thesis_id}/confirm"), Some(&advisor), "{}").await;
    assert_eq!(s, StatusCode::OK);

    let (s, _, b) = post_json(app, &format!("/api/theses/{thesis_id}/rounds"), Some(&secretary), "{}").await;
    assert_eq!(s, StatusCode::OK);
    let round_id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["round_id"]
        .as_i64()
        .unwrap();

    let (s, _, _) = post_json(
        app,
        &format!("/api/rounds/{round_id}/check"),
        Some(&secretary),
        r#"{"passed":true,"comment":"匿名合格"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let (s, _, _) = post_json(
        app,
        &format!("/api/rounds/{round_id}/assign"),
        Some(&secretary),
        r#"{"reviewer_ids":[6,7]}"#,
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // v1 的版本 id
    let (s, b) = get(app, &format!("/api/theses/{thesis_id}"), Some(&student)).await;
    assert_eq!(s, StatusCode::OK);
    let detail: serde_json::Value = serde_json::from_slice(&b).unwrap();
    let v1 = detail["review_version"]["version_id"].as_i64().unwrap();
    (thesis_id, round_id, v1, student, advisor, secretary)
}

async fn submit_report(app: &Router, reviewer: &str, assignment_id: i64, comment: &str) -> StatusCode {
    let cookie = login(app, reviewer).await;
    let (s, _, _) = post_json(
        app,
        &format!("/api/assignments/{assignment_id}/report"),
        Some(&cookie),
        &format!(r#"{{"score":85,"recommendation":"同意答辩","comment":"{comment}"}}"#),
    )
    .await;
    s
}

/// 获取某轮次的 assignment id 列表（秘书视角）
async fn assignment_ids(app: &Router, secretary: &str, thesis_id: i64) -> Vec<i64> {
    let (s, b) = get(app, &format!("/api/theses/{thesis_id}"), Some(secretary)).await;
    assert_eq!(s, StatusCode::OK);
    let detail: serde_json::Value = serde_json::from_slice(&b).unwrap();
    detail["rounds"][0]["assignments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["assignment_id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn anonymity_not_leaked_to_student_or_advisor() {
    let t = setup().await;
    let (thesis_id, _round_id, _v1, student, advisor, secretary) = flow_until_assigned(&t.app).await;

    let ids = assignment_ids(&t.app, &secretary, thesis_id).await;
    assert_eq!(submit_report(&t.app, "reviewer1", ids[0], "选题有价值").await, StatusCode::OK);
    assert_eq!(submit_report(&t.app, "reviewer2", ids[1], "结构需调整").await, StatusCode::OK);

    let (s, _, rb) = post_json(&t.app, &format!("/api/rounds/{}/release", 1), Some(&secretary), "{}").await;
    assert_eq!(s, StatusCode::OK, "release failed: {}", String::from_utf8_lossy(&rb));

    // 学生视角：能看到意见与别名，但绝不能出现评阅人任何身份信息
    let (s, b) = get(&t.app, &format!("/api/theses/{thesis_id}"), Some(&student)).await;
    assert_eq!(s, StatusCode::OK);
    let text = String::from_utf8(b.clone()).unwrap();
    assert!(text.contains("评阅人甲"), "应包含匿名别名: {text}");
    assert!(text.contains("选题有价值"), "应包含已开放的评阅意见");
    for forbidden in ["赵评审", "钱评审", "孙评审", "reviewer1", "reviewer2", "reviewer3", "reviewer_id", "reviewer_name"] {
        assert!(!text.contains(forbidden), "学生视角泄露了 {forbidden}: {text}");
    }

    // 导师视角：同样不能出现评阅人身份
    let (s, b) = get(&t.app, &format!("/api/theses/{thesis_id}"), Some(&advisor)).await;
    assert_eq!(s, StatusCode::OK);
    let text = String::from_utf8(b).unwrap();
    assert!(text.contains("评阅人甲"));
    for forbidden in ["赵评审", "钱评审", "reviewer1", "reviewer2", "reviewer_id", "reviewer_name"] {
        assert!(!text.contains(forbidden), "导师视角泄露了 {forbidden}: {text}");
    }

    // 学生/导师无权访问评阅人名单接口
    let (s, _) = get(&t.app, "/api/secretary/reviewers", Some(&student)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = get(&t.app, "/api/secretary/reviewers", Some(&advisor)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // 评阅人视角不包含学生身份
    let reviewer = login(&t.app, "reviewer1").await;
    let (s, b) = get(&t.app, "/api/reviewer/assignments", Some(&reviewer)).await;
    assert_eq!(s, StatusCode::OK);
    let text = String::from_utf8(b).unwrap();
    for forbidden in ["陈晨", "student1", "student_id"] {
        assert!(!text.contains(forbidden), "评阅人视角泄露了 {forbidden}: {text}");
    }
    // 评阅人不能访问论文详情接口
    let (s, _) = get(&t.app, &format!("/api/theses/{thesis_id}"), Some(&reviewer)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn reupload_keeps_frozen_review_version() {
    let t = setup().await;
    let (thesis_id, _round_id, v1, student, _advisor, secretary) = flow_until_assigned(&t.app).await;

    // 送审中学生重传新稿
    let s = upload(&t.app, &student, thesis_id, "thesis-v2.txt", b"VERSION-2 rewritten").await;
    assert_eq!(s, StatusCode::OK);

    // 学生详情：最新上传稿是 v2，实际送审稿仍是 v1
    let (s, b) = get(&t.app, &format!("/api/theses/{thesis_id}"), Some(&student)).await;
    assert_eq!(s, StatusCode::OK);
    let detail: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(detail["versions"][0]["seq"].as_i64().unwrap(), 2, "最新上传稿应为第 2 版");
    assert_eq!(detail["review_version"]["seq"].as_i64().unwrap(), 1, "实际送审稿仍应冻结在第 1 版");
    assert_eq!(detail["review_version"]["version_id"].as_i64().unwrap(), v1);

    // 评阅人下载到的仍是冻结的 v1 内容
    let reviewer = login(&t.app, "reviewer1").await;
    let (s, b) = get(&t.app, &format!("/api/files/{v1}/download"), Some(&reviewer)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b, b"VERSION-1 anonymous content", "评阅人必须拿到原始送审稿");

    // 评阅人不能下载学生新上传的 v2
    let v2 = detail["versions"][0]["id"].as_i64().unwrap();
    let (s, _) = get(&t.app, &format!("/api/files/{v2}/download"), Some(&reviewer)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "评阅人不应下载到未送审的新稿");

    // 秘书决定以新稿重新送审：旧轮次作废，评阅人随即失去旧稿访问权
    let (s, _, _) = post_json(&t.app, &format!("/api/theses/{thesis_id}/rounds"), Some(&secretary), "{}").await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = get(&t.app, &format!("/api/files/{v1}/download"), Some(&reviewer)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "轮次作废后旧稿下载应被回收");
}

#[tokio::test]
async fn unauthorized_downloads_are_rejected() {
    let t = setup().await;
    let (thesis_id, _round_id, v1, _student, _advisor, _secretary) = flow_until_assigned(&t.app).await;

    // 未登录
    let (s, _) = get(&t.app, &format!("/api/files/{v1}/download"), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // 其他学生（猜测版本 id）
    let student2 = login(&t.app, "student2").await;
    let (s, _) = get(&t.app, &format!("/api/files/{v1}/download"), Some(&student2)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // 未被分配的评阅人
    let reviewer3 = login(&t.app, "reviewer3").await;
    let (s, _) = get(&t.app, &format!("/api/files/{v1}/download"), Some(&reviewer3)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // 非该论文的导师
    let advisor2 = login(&t.app, "advisor2").await;
    let (s, _) = get(&t.app, &format!("/api/files/{v1}/download"), Some(&advisor2)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // 不存在的版本 id
    let (s, _) = get(&t.app, "/api/files/9999/download", Some(&student2)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // 本人 / 导师 / 秘书 / 被分配评阅人可以下载
    let student1 = login(&t.app, "student1").await;
    assert_eq!(get(&t.app, &format!("/api/files/{v1}/download"), Some(&student1)).await.0, StatusCode::OK);
    let advisor1 = login(&t.app, "advisor1").await;
    assert_eq!(get(&t.app, &format!("/api/files/{v1}/download"), Some(&advisor1)).await.0, StatusCode::OK);
    let secretary = login(&t.app, "secretary").await;
    assert_eq!(get(&t.app, &format!("/api/files/{v1}/download"), Some(&secretary)).await.0, StatusCode::OK);
    let reviewer1 = login(&t.app, "reviewer1").await;
    assert_eq!(get(&t.app, &format!("/api/files/{v1}/download"), Some(&reviewer1)).await.0, StatusCode::OK);

    // 评阅人不能替他人提交评阅意见
    let ids = assignment_ids(&t.app, &secretary, thesis_id).await;
    let reviewer2 = login(&t.app, "reviewer2").await;
    let (s, _, _) = post_json(
        &t.app,
        &format!("/api/assignments/{}/report", ids[0]),
        Some(&reviewer2),
        r#"{"score":10,"comment":"越权提交"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}
