use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::PathBuf;
use thesis_review_server::{build_app, schema, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    let data_dir = PathBuf::from(data_dir);
    let upload_dir = data_dir.join("uploads");
    tokio::fs::create_dir_all(&upload_dir).await?;

    let db_path = data_dir.join("app.db");
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    schema::migrate(&pool).await?;
    schema::seed(&pool).await?;

    let state = AppState::new(pool, upload_dir);
    let web_dist = std::env::var("WEB_DIST").unwrap_or_else(|_| "./web/dist".to_string());
    let app = build_app(state, Some(PathBuf::from(web_dist)));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!("论文匿名送审系统已启动: http://localhost:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}
