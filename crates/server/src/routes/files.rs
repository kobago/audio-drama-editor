use std::path::{Component, Path};

use axum::extract::DefaultBodyLimit;
use axum::extract::{Multipart, State};
use axum::routing::post;
use axum::{Json, Router};
use serde::Serialize;

use crate::AppState;
use crate::error::{AppError, AppResult};

const ALLOWED: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "wav", "flac", "mp3", "ogg",
];

#[derive(Serialize)]
pub struct Uploaded {
    /// `/media/` 配下の相対パス。DB にはこの値を保存する。
    path: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/files", post(upload))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
}

async fn upload(State(s): State<AppState>, mut multipart: Multipart) -> AppResult<Json<Uploaded>> {
    let field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest("no file".into()))?;
    let ext = field
        .file_name()
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| ALLOWED.contains(&e.as_str()))
        .ok_or_else(|| AppError::BadRequest("unsupported file type".into()))?;
    let bytes = field
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let name = format!("{}.{ext}", uuid::Uuid::new_v4());
    tokio::fs::write(s.media_dir.join(&name), &bytes)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(Json(Uploaded { path: name }))
}

/// 消した行が指していた `/media/` 配下のファイルを消す。
/// ほかの行がまだ指しているファイルと、`/media/` の外を指すパスは残す。
pub(crate) async fn remove_unused(s: &AppState, paths: Vec<String>) {
    for path in paths {
        if !Path::new(&path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            tracing::warn!("not removing {path}: outside the media directory");
            continue;
        }
        let used = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM takes WHERE path = ?1
                UNION ALL SELECT 1 FROM character_portraits WHERE path = ?1
                UNION ALL SELECT 1 FROM characters WHERE voice_ref_path = ?1
                UNION ALL SELECT 1 FROM scenes WHERE background_path = ?1
            )",
        )
        .bind(&path)
        .fetch_one(&s.db)
        .await;
        match used {
            Ok(false) => {}
            Ok(true) => continue,
            Err(e) => {
                tracing::warn!("not removing {path}: {e}");
                continue;
            }
        }
        if let Err(e) = tokio::fs::remove_file(s.media_dir.join(&path)).await {
            tracing::warn!("failed to remove {path}: {e}");
        }
    }
}
