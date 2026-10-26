//! audio-drama-editor のサーバー: REST API、音声生成ワーカー、埋め込んだ画面の配信。

mod error;
pub mod rag;
mod recall;
mod routes;
pub mod settings;
mod tts;
mod writer;

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use axum::Router;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// 起動時に決まる設定。環境変数と起動オプションから作る (main.rs)。
#[derive(Debug, Clone)]
pub struct Config {
    /// データと設定の置き場所 (DB と media/)
    pub home: PathBuf,
    pub tts: TtsConfig,
    /// 記憶の検索 (EmbeddingGemma 2 + d1-3B)。false ならモデルを読み込まない
    pub rag: bool,
    /// 設定画面の初期値になる環境変数の値 (画面で保存した値が優先)
    pub env: settings::EnvDefaults,
}

#[derive(Debug, Clone)]
pub struct TtsConfig {
    pub enabled: bool,
    pub checkpoint: String,
    pub codec: String,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            checkpoint: irodori::DEFAULT_CHECKPOINT.into(),
            codec: irodori::DEFAULT_CODEC.into(),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    /// アップロード画像・生成音声の保存先。`/media/` で配信する。
    pub media_dir: PathBuf,
    pub env: Arc<settings::EnvDefaults>,
    pub http: reqwest::Client,
    pub(crate) tts: Arc<tts::TtsEngine>,
    pub(crate) jobs: routes::tts::Jobs,
    pub(crate) rag: Arc<rag::RagEngine>,
}

impl AppState {
    pub async fn open(config: Config) -> anyhow::Result<Self> {
        let media_dir = config.home.join("media");
        tokio::fs::create_dir_all(&media_dir).await?;
        let db_path = config.home.join("audio-drama-editor.db");
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
            .create_if_missing(true)
            .foreign_keys(true);
        let db = SqlitePool::connect_with(opts).await?;
        sqlx::migrate!("./migrations").run(&db).await?;
        routes::generate::fail_interrupted(&db).await?;

        let tts = if config.tts.enabled {
            tts::TtsEngine::spawn(config.tts.checkpoint, config.tts.codec)
        } else {
            tts::TtsEngine::disabled()
        };
        Ok(Self {
            db,
            media_dir,
            env: Arc::new(config.env),
            http: reqwest::Client::builder()
                .user_agent(llm::USER_AGENT)
                .build()?,
            tts: Arc::new(tts),
            jobs: Default::default(),
            rag: Arc::new(if config.rag {
                rag::RagEngine::spawn()
            } else {
                rag::RagEngine::disabled()
            }),
        })
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .nest("/api", routes::router())
        .nest_service("/media", ServeDir::new(&state.media_dir))
        .fallback(spa)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(rust_embed::RustEmbed)]
#[folder = "../../frontend/dist/"]
#[allow_missing = true]
struct Spa;

/// 埋め込んだ画面を返す。知らないパスは index.html (クライアント側のルーティングに任せる)。
async fn spa(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") || path.starts_with("media/") {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({ "error": "not found" })),
        )
            .into_response();
    }
    let (file, cache) = match Spa::get(path).filter(|_| !path.is_empty()) {
        Some(f) if path.starts_with("assets/") => (f, "public, max-age=31536000, immutable"),
        Some(f) => (f, "no-cache"),
        None => match Spa::get("index.html") {
            Some(f) => (f, "no-cache"),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    "frontend is not built (run `make frontend`)",
                )
                    .into_response();
            }
        },
    };
    let headers = [
        (header::CONTENT_TYPE, file.metadata.mimetype().to_string()),
        (header::CACHE_CONTROL, cache.to_string()),
    ];
    (headers, file.data).into_response()
}
