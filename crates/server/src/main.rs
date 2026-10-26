mod error;
mod routes;

use std::path::PathBuf;
use std::str::FromStr;

use axum::Router;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    /// アップロード画像・生成音声の保存先
    pub data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let data_dir = PathBuf::from(std::env::var("ADE_DATA_DIR").unwrap_or_else(|_| "data".into()));
    tokio::fs::create_dir_all(data_dir.join("files")).await?;

    let opts =
        SqliteConnectOptions::from_str(&format!("sqlite://{}", data_dir.join("app.db").display()))?
            .create_if_missing(true)
            .foreign_keys(true);
    let db = SqlitePool::connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&db).await?;

    let state = AppState {
        db,
        data_dir: data_dir.clone(),
    };
    let app = Router::new()
        .nest("/api", routes::router())
        .nest_service("/files", ServeDir::new(data_dir.join("files")))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = std::env::var("ADE_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
