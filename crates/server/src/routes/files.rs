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
