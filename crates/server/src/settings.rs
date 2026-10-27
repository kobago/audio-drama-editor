//! 画面から変えられる設定。値は「保存した値 → 環境変数 → 既定値」の順に解決する。
//!
//! 待ち受けアドレスやデータの置き場所のように起動前に決まっている必要がある項目は、
//! ここには置かず環境変数と起動オプションで決める (main.rs)。

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::AppState;
use crate::error::{AppError, AppResult};

pub const DEFAULT_LLAMA_SERVER_URL: &str = "http://127.0.0.1:8080";
pub const DEFAULT_OPENCODE_BASE_URL: &str = "https://opencode.ai/zen/go/v1";

const LLAMA_SERVER_URL: &str = "llama_server_url";
const OPENCODE_BASE_URL: &str = "opencode_base_url";
const OPENCODE_MODEL: &str = "opencode_model";
const OPENCODE_API_KEY: &str = "opencode_api_key";
const LLM_PROVIDER: &str = "llm_provider";

/// 設定画面の初期値になる環境変数の値。
#[derive(Debug, Clone, Default)]
pub struct EnvDefaults {
    pub llama_server_url: Option<String>,
    pub opencode_base_url: Option<String>,
    pub opencode_api_key: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Saved,
    Env,
    Default,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Resolved {
    pub value: String,
    pub source: Source,
}

async fn saved(db: &SqlitePool, key: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
}

async fn save(db: &SqlitePool, key: &str, value: &str) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(db)
        .await?;
    Ok(())
}

async fn clear(db: &SqlitePool, key: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(db)
        .await?;
    Ok(())
}

async fn resolve(
    db: &SqlitePool,
    key: &str,
    env: Option<&str>,
    default: &str,
) -> sqlx::Result<Resolved> {
    Ok(match (saved(db, key).await?, env) {
        (Some(value), _) => Resolved {
            value,
            source: Source::Saved,
        },
        (None, Some(v)) => Resolved {
            value: v.to_owned(),
            source: Source::Env,
        },
        (None, None) => Resolved {
            value: default.to_owned(),
            source: Source::Default,
        },
    })
}

/// LLM の接続先。リクエストのたびに解決する (画面での変更をすぐ反映するため)。
#[derive(Debug, Clone)]
pub struct LlmSettings {
    /// セリフの生成に使う接続先
    pub provider: Provider,
    pub llama_server_url: String,
    pub opencode_base_url: String,
    pub opencode_model: String,
    pub opencode_api_key: Option<String>,
}

pub async fn llm(state: &AppState) -> sqlx::Result<LlmSettings> {
    let env = &state.env;
    Ok(LlmSettings {
        provider: provider(&state.db).await?,
        llama_server_url: resolve(
            &state.db,
            LLAMA_SERVER_URL,
            env.llama_server_url.as_deref(),
            DEFAULT_LLAMA_SERVER_URL,
        )
        .await?
        .value,
        opencode_base_url: resolve(
            &state.db,
            OPENCODE_BASE_URL,
            env.opencode_base_url.as_deref(),
            DEFAULT_OPENCODE_BASE_URL,
        )
        .await?
        .value,
        opencode_model: resolve(&state.db, OPENCODE_MODEL, None, "").await?.value,
        opencode_api_key: match saved(&state.db, OPENCODE_API_KEY).await? {
            Some(k) => Some(k),
            None => env.opencode_api_key.clone(),
        },
    })
}

async fn provider(db: &SqlitePool) -> sqlx::Result<Provider> {
    Ok(match saved(db, LLM_PROVIDER).await?.as_deref() {
        Some("llama_server") => Provider::LlamaServer,
        _ => Provider::Opencode,
    })
}

/// API キーそのものは返さず、どこから来たかだけを返す。
#[derive(Serialize)]
pub struct SecretView {
    source: Option<Source>,
}

#[derive(Serialize)]
pub struct SettingsView {
    llm_provider: Provider,
    llama_server_url: Resolved,
    opencode_base_url: Resolved,
    opencode_model: Resolved,
    opencode_api_key: SecretView,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(show).put(update))
        .route("/settings/opencode-api-key", put(put_api_key))
        .route("/settings/check", post(check))
}

async fn show(State(s): State<AppState>) -> AppResult<Json<SettingsView>> {
    let env = &s.env;
    let key_source = if saved(&s.db, OPENCODE_API_KEY).await?.is_some() {
        Some(Source::Saved)
    } else if env.opencode_api_key.is_some() {
        Some(Source::Env)
    } else {
        None
    };
    Ok(Json(SettingsView {
        llm_provider: provider(&s.db).await?,
        llama_server_url: resolve(
            &s.db,
            LLAMA_SERVER_URL,
            env.llama_server_url.as_deref(),
            DEFAULT_LLAMA_SERVER_URL,
        )
        .await?,
        opencode_base_url: resolve(
            &s.db,
            OPENCODE_BASE_URL,
            env.opencode_base_url.as_deref(),
            DEFAULT_OPENCODE_BASE_URL,
        )
        .await?,
        opencode_model: resolve(&s.db, OPENCODE_MODEL, None, "").await?,
        opencode_api_key: SecretView { source: key_source },
    }))
}

#[derive(Deserialize)]
pub struct SettingsInput {
    /// 省略時は変えない
    llm_provider: Option<Provider>,
    llama_server_url: String,
    opencode_base_url: String,
    opencode_model: String,
}

fn normalize_url(url: &str) -> AppResult<String> {
    let url = url.trim().trim_end_matches('/');
    if !(url.starts_with("https://") || url.starts_with("http://"))
        || reqwest::Url::parse(url).is_err()
    {
        return Err(AppError::BadRequest(format!("invalid URL: {url}")));
    }
    Ok(url.to_owned())
}

async fn update(
    State(s): State<AppState>,
    Json(input): Json<SettingsInput>,
) -> AppResult<StatusCode> {
    let llama = normalize_url(&input.llama_server_url)?;
    let opencode = normalize_url(&input.opencode_base_url)?;
    if let Some(p) = input.llm_provider {
        let value = match p {
            Provider::LlamaServer => "llama_server",
            Provider::Opencode => "opencode",
        };
        save(&s.db, LLM_PROVIDER, value).await?;
    }
    save(&s.db, LLAMA_SERVER_URL, &llama).await?;
    save(&s.db, OPENCODE_BASE_URL, &opencode).await?;
    match input.opencode_model.trim() {
        "" => clear(&s.db, OPENCODE_MODEL).await?,
        model => save(&s.db, OPENCODE_MODEL, model).await?,
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ApiKeyInput {
    key: String,
}

/// 空なら保存したキーを消す (環境変数があればそちらに戻る)。
async fn put_api_key(
    State(s): State<AppState>,
    Json(input): Json<ApiKeyInput>,
) -> AppResult<StatusCode> {
    let key = input.key.trim();
    if key.is_empty() {
        clear(&s.db, OPENCODE_API_KEY).await?;
    } else if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::BadRequest(
            "the API key must not contain spaces".into(),
        ));
    } else {
        save(&s.db, OPENCODE_API_KEY, key).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    LlamaServer,
    Opencode,
}

#[derive(Deserialize)]
pub struct CheckInput {
    provider: Provider,
}

#[derive(Serialize)]
pub struct CheckResult {
    ok: bool,
    /// 接続先が返したモデル ID
    models: Vec<String>,
    error: Option<String>,
}

/// 接続先の `/models` を呼んで、つながるか (と使えるモデル) を確かめる。
async fn check(
    State(s): State<AppState>,
    Json(input): Json<CheckInput>,
) -> AppResult<Json<CheckResult>> {
    let llm = llm(&s).await?;
    let mut req = match input.provider {
        Provider::LlamaServer => s.http.get(format!("{}/v1/models", llm.llama_server_url)),
        Provider::Opencode => {
            let Some(key) = &llm.opencode_api_key else {
                return Ok(Json(CheckResult {
                    ok: false,
                    models: vec![],
                    error: Some("API キーが保存されていません".into()),
                }));
            };
            s.http
                .get(format!("{}/models", llm.opencode_base_url))
                .bearer_auth(key)
        }
    };
    req = req.timeout(std::time::Duration::from_secs(10));
    let result = async {
        let res = req.send().await?;
        let status = res.status();
        if !status.is_success() {
            anyhow::bail!("HTTP {status}");
        }
        let body: serde_json::Value = res.json().await?;
        let models = body["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["id"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        Ok::<_, anyhow::Error>(models)
    }
    .await;
    Ok(Json(match result {
        Ok(models) => CheckResult {
            ok: true,
            models,
            error: None,
        },
        Err(e) => CheckResult {
            ok: false,
            models: vec![],
            error: Some(format!("{e:#}")),
        },
    }))
}
