use std::path::PathBuf;
use thesis_review::{build_app, db, AppState};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let upload_dir = PathBuf::from(&data_dir).join("uploads");
    std::fs::create_dir_all(&upload_dir).expect("无法创建上传目录");

    let db_path = PathBuf::from(&data_dir).join("thesis.db");
    let pool = db::connect(&db_path).await.expect("无法连接数据库");
    db::init(&pool).await.expect("数据库初始化失败");

    let state = AppState {
        pool,
        upload_dir,
    };
    let dist = std::env::var("FRONTEND_DIST").unwrap_or_else(|_| "frontend/dist".to_string());
    let app = build_app(state, PathBuf::from(dist));

    let bind = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .expect("端口绑定失败");
    println!("服务器已启动: http://localhost:3000 （演示账号见 README.md）");
    axum::serve(listener, app).await.expect("服务器运行失败");
}
