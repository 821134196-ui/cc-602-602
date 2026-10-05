use axum::{
    body::{to_bytes, Body, Bytes},
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::util::ServiceExt;
use uuid::Uuid;

use thesis_review::{build_app, db, AppState};

// ---------- 测试辅助 ----------

async fn setup_app() -> Router {
    let dir = std::env::temp_dir().join(format!("thesis-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("uploads")).unwrap();
    let pool = db::connect(&dir.join("test.db")).await.unwrap();
    db::init(&pool).await.unwrap();
    build_app(
        AppState {
            pool,
            upload_dir: dir.join("uploads"),
        },
        dir.join("dist"),
    )
}

async fn call(app: &Router, req: Request<Body>) -> (StatusCode, Bytes) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 50 * 1024 * 1024).await.unwrap();
    (status, bytes)
}

async fn json(
    app: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(path);
    if let Some(t) = token {
        b = b.header("Authorization", format!("Bearer {t}"));
    }
    let req = match body {
        Some(v) => b
            .header("Content-Type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    };
    let (status, bytes) = call(app, req).await;
    let v: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("响应应为 JSON")
    };
    (status, v)
}

async fn login(app: &Router, username: &str) -> (String, i64) {
    let (status, v) = json(
        app,
        "POST",
        "/api/login",
        None,
        Some(json!({ "username": username, "password": "123456" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "登录失败: {username}");
    (
        v["token"].as_str().unwrap().to_string(),
        v["user"]["id"].as_i64().unwrap(),
    )
}

async fn upload(
    app: &Router,
    token: &str,
    thesis_id: i64,
    filename: &str,
    content: &[u8],
) -> (StatusCode, Value) {
    let boundary = "XTESTBOUNDARY";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/pdf\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/student/theses/{thesis_id}/upload"))
        .header("Authorization", format!("Bearer {token}"))
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let (status, bytes) = call(app, req).await;
    let v: Value = serde_json::from_slice(&bytes).expect("上传响应应为 JSON");
    (status, v)
}

async fn download(app: &Router, token: Option<&str>, version_id: i64) -> (StatusCode, Bytes) {
    let mut b = Request::builder()
        .method("GET")
        .uri(format!("/api/files/versions/{version_id}/download"));
    if let Some(t) = token {
        b = b.header("Authorization", format!("Bearer {t}"));
    }
    call(app, b.body(Body::empty()).unwrap()).await
}

/// 走通到“已分配评阅人”的标准流程，返回各 token 与关键 id
struct Flow {
    sec: String,
    adv: String,
    stu: String,
    rev1: String,
    rev1_id: i64,
    rev2_id: i64,
    rev3: String,
    thesis_id: i64,
    v1_id: i64,
    freeze_id: i64,
}

async fn flow_until_assigned(app: &Router) -> Flow {
    let (sec, _) = login(app, "secretary").await;
    let (adv, adv_id) = login(app, "advisor1").await;
    let (stu, _) = login(app, "student1").await;
    let (rev1, rev1_id) = login(app, "reviewer1").await;
    let (_, rev2_id) = login(app, "reviewer2").await;
    let (rev3, _) = login(app, "reviewer3").await;

    let (s, v) = json(
        app,
        "POST",
        "/api/student/theses",
        Some(&stu),
        Some(json!({ "title": "测试论文", "advisor_id": adv_id })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let thesis_id = v["id"].as_i64().unwrap();

    let (s, v) = upload(app, &stu, thesis_id, "thesis.pdf", b"VERSION-1-CONTENT").await;
    assert_eq!(s, StatusCode::OK);
    let v1_id = v["id"].as_i64().unwrap();

    let (s, _) = json(
        app,
        "POST",
        &format!("/api/advisor/theses/{thesis_id}/confirm"),
        Some(&adv),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let (s, v) = json(
        app,
        "POST",
        &format!("/api/secretary/theses/{thesis_id}/freeze"),
        Some(&sec),
        Some(json!({ "version_id": v1_id, "anonymity_note": "已检查，无作者与导师身份信息" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let freeze_id = v["id"].as_i64().unwrap();

    let (s, _) = json(
        app,
        "POST",
        &format!("/api/secretary/freezes/{freeze_id}/assign"),
        Some(&sec),
        Some(json!({ "reviewer_ids": [rev1_id, rev2_id] })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    Flow {
        sec,
        adv,
        stu,
        rev1,
        rev1_id,
        rev2_id,
        rev3,
        thesis_id,
        v1_id,
        freeze_id,
    }
}

async fn submit_review(app: &Router, rev_token: &str, comment: &str) -> StatusCode {
    let (s, v) = json(app, "GET", "/api/reviewer/assignments", Some(rev_token), None).await;
    assert_eq!(s, StatusCode::OK);
    let assignment_id = v[0]["assignment_id"].as_i64().unwrap();
    let (s, _) = json(
        app,
        "POST",
        &format!("/api/reviewer/assignments/{assignment_id}/review"),
        Some(rev_token),
        Some(json!({ "comment": comment, "recommendation": "pass", "score": 88 })),
    )
    .await;
    s
}

// ---------- 测试 1：匿名性 ----------

#[tokio::test]
async fn anonymity_not_leaked_to_student_or_advisor() {
    let app = setup_app().await;
    let f = flow_until_assigned(&app).await;

    // 评阅人 1 提交意见
    let s = submit_review(&app, &f.rev1, "评阅人一的意见").await;
    assert_eq!(s, StatusCode::OK);

    // 另一位未提交时，秘书不能开放意见
    let (s, _) = json(
        &app,
        "POST",
        &format!("/api/secretary/freezes/{}/release", f.freeze_id),
        Some(&f.sec),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "未收齐意见时不允许开放");

    // 开放前学生看不到任何评阅内容
    let (s, v) = json(&app, "GET", "/api/student/theses", Some(&f.stu), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v[0]["reviews_released"], false);
    assert_eq!(v[0]["reviews"].as_array().unwrap().len(), 0);

    // 评阅人 2 提交，秘书开放
    let (rev2, _) = login(&app, "reviewer2").await;
    let s = submit_review(&app, &rev2, "评阅人二的意见").await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = json(
        &app,
        "POST",
        &format!("/api/secretary/freezes/{}/release", f.freeze_id),
        Some(&f.sec),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // 学生视角：意见可见但完全匿名
    let (s, v) = json(&app, "GET", "/api/student/theses", Some(&f.stu), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v[0]["reviews_released"], true);
    assert_eq!(v[0]["reviews"].as_array().unwrap().len(), 2);
    let text = v.to_string();
    for forbidden in [
        "reviewer1", "reviewer2", "reviewer3",
        "赵评阅", "钱评阅", "孙评阅",
        "reviewer_id", "assignment_id",
    ] {
        assert!(!text.contains(forbidden), "学生响应泄露了评阅人身份: {forbidden}");
    }

    // 导师视角：同样匿名
    let (s, v) = json(&app, "GET", "/api/advisor/theses", Some(&f.adv), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v[0]["reviews"].as_array().unwrap().len(), 2);
    let text = v.to_string();
    for forbidden in [
        "reviewer1", "reviewer2", "reviewer3",
        "赵评阅", "钱评阅", "孙评阅",
        "reviewer_id", "assignment_id",
    ] {
        assert!(!text.contains(forbidden), "导师响应泄露了评阅人身份: {forbidden}");
    }
}

// ---------- 测试 2：重传保留送审稿 ----------

#[tokio::test]
async fn reupload_preserves_frozen_review_version() {
    let app = setup_app().await;
    let f = flow_until_assigned(&app).await;

    // 学生在送审后重传 v2
    let (s, v) = upload(&app, &f.stu, f.thesis_id, "thesis-v2.pdf", b"VERSION-2-CONTENT").await;
    assert_eq!(s, StatusCode::OK);
    let v2_id = v["id"].as_i64().unwrap();
    assert_ne!(v2_id, f.v1_id);

    // 学生页面：最新上传稿 = v2，实际送审稿 = v1
    let (s, v) = json(&app, "GET", "/api/student/theses", Some(&f.stu), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v[0]["latest_version_id"], v2_id, "最新上传稿应为 v2");
    assert_eq!(v[0]["review_version_id"], f.v1_id, "实际送审稿应仍为 v1");

    // 评阅人下载送审稿，拿到的仍是 v1 的内容
    let (status, bytes) = download(&app, Some(&f.rev1), f.v1_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&bytes[..], b"VERSION-1-CONTENT", "评阅人应拿到冻结时的 v1");

    // 评阅人不能下载学生新上传的 v2
    let (status, _) = download(&app, Some(&f.rev1), v2_id).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "评阅人不应看到未送审的新版本");

    // 秘书决定以 v2 重新送审：冻结 v2 并分配给 reviewer3
    let (_, rev3_id) = login(&app, "reviewer3").await;
    let (s, v) = json(
        &app,
        "POST",
        &format!("/api/secretary/theses/{}/freeze", f.thesis_id),
        Some(&f.sec),
        Some(json!({ "version_id": v2_id, "anonymity_note": "v2 匿名检查通过" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let freeze2_id = v["id"].as_i64().unwrap();
    let (s, _) = json(
        &app,
        "POST",
        &format!("/api/secretary/freezes/{freeze2_id}/assign"),
        Some(&f.sec),
        Some(json!({ "reviewer_ids": [rev3_id] })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // reviewer3 下载到 v2 内容
    let (status, bytes) = download(&app, Some(&f.rev3), v2_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&bytes[..], b"VERSION-2-CONTENT");

    // reviewer1 未被分配到第二轮，仍只能下载 v1
    let (status, _) = download(&app, Some(&f.rev1), v2_id).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, bytes) = download(&app, Some(&f.rev1), f.v1_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&bytes[..], b"VERSION-1-CONTENT");

    // 秘书视角能看到两轮冻结与分配记录
    let (s, v) = json(&app, "GET", "/api/secretary/theses", Some(&f.sec), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v[0]["freezes"].as_array().unwrap().len(), 2, "应有两轮送审记录");
}

// ---------- 测试 3：越权下载被拒 ----------

#[tokio::test]
async fn unauthorized_download_rejected() {
    let app = setup_app().await;
    let f = flow_until_assigned(&app).await;

    let (_, _) = login(&app, "student2").await;
    let (stu2, _) = login(&app, "student2").await;
    let (adv2, _) = login(&app, "advisor2").await;
    let (rev3, _) = login(&app, "reviewer3").await;

    // 其他学生下载 → 403
    let (status, _) = download(&app, Some(&stu2), f.v1_id).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "其他学生不应能下载");

    // 非指导教师下载 → 403
    let (status, _) = download(&app, Some(&adv2), f.v1_id).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "非指导教师不应能下载");

    // 未被分配的评阅人下载 → 403
    let (status, _) = download(&app, Some(&rev3), f.v1_id).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "未分配的评阅人不应能下载");

    // 未登录 → 401
    let (status, _) = download(&app, None, f.v1_id).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 猜测不存在的版本号 → 404
    let (status, _) = download(&app, Some(&f.sec), 99999).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 本人 / 导师 / 秘书 / 被分配评阅人可以下载
    for token in [&f.stu, &f.adv, &f.sec, &f.rev1] {
        let (status, _) = download(&app, Some(token), f.v1_id).await;
        assert_eq!(status, StatusCode::OK, "有权限的角色应能下载");
    }
}

// ---------- 测试 4：角色与分配范围隔离 ----------

#[tokio::test]
async fn role_and_assignment_scoping() {
    let app = setup_app().await;
    let f = flow_until_assigned(&app).await;
    let (stu2, _) = login(&app, "student2").await;

    // 导师不能访问秘书接口（含评阅人名单）
    for path in ["/api/secretary/theses", "/api/secretary/reviewers"] {
        let (s, _) = json(&app, "GET", path, Some(&f.adv), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "导师不应访问 {path}");
    }
    // 学生不能访问评阅人接口
    let (s, _) = json(&app, "GET", "/api/reviewer/assignments", Some(&f.stu), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // 评阅人不能访问学生/秘书接口
    let (s, _) = json(&app, "GET", "/api/student/theses", Some(&f.rev1), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = json(&app, "GET", "/api/secretary/theses", Some(&f.rev1), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // 未登录一律 401
    let (s, _) = json(&app, "GET", "/api/secretary/theses", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // 评阅人只能看到自己的任务，且看不到学生/导师身份
    let (s, v) = json(&app, "GET", "/api/reviewer/assignments", Some(&f.rev1), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 1);
    let text = v.to_string();
    for forbidden in ["李同学", "王同学", "张教授", "student", "advisor"] {
        assert!(!text.contains(forbidden), "评阅人响应泄露了身份: {forbidden}");
    }

    // reviewer3 没有任务
    let (s, v) = json(&app, "GET", "/api/reviewer/assignments", Some(&f.rev3), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 0);

    // reviewer1 不能替别人的任务提交意见：找到 reviewer2 的任务 id
    let (rev2, _) = login(&app, "reviewer2").await;
    let (_, v) = json(&app, "GET", "/api/reviewer/assignments", Some(&rev2), None).await;
    let other_assignment = v[0]["assignment_id"].as_i64().unwrap();
    let (s, _) = json(
        &app,
        "POST",
        &format!("/api/reviewer/assignments/{other_assignment}/review"),
        Some(&f.rev1),
        Some(json!({ "comment": "越权提交", "recommendation": "pass", "score": 100 })),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "不能替他人的任务提交意见");

    // 其他学生看不到这篇论文
    let (s, v) = json(&app, "GET", "/api/student/theses", Some(&stu2), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 0);
}
