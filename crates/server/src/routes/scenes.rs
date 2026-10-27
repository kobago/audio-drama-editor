use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::AppResult;

#[derive(Serialize, sqlx::FromRow)]
pub struct Scene {
    id: i64,
    project_id: i64,
    title: String,
    setting: String,
    summary: String,
    background_path: Option<String>,
    sort_order: i64,
}

#[derive(Deserialize)]
pub struct SceneInput {
    #[serde(default)]
    title: String,
    #[serde(default)]
    setting: String,
    #[serde(default)]
    summary: String,
    background_path: Option<String>,
    #[serde(default)]
    sort_order: i64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/{id}/scenes", get(list).post(create))
        .route("/scenes/{id}", get(show).put(update).delete(remove))
}

async fn list(
    State(s): State<AppState>,
    Path(project_id): Path<i64>,
) -> AppResult<Json<Vec<Scene>>> {
    let rows = sqlx::query_as("SELECT * FROM scenes WHERE project_id = ? ORDER BY sort_order, id")
        .bind(project_id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

async fn show(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<Json<Scene>> {
    let row = sqlx::query_as("SELECT * FROM scenes WHERE id = ?")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

async fn create(
    State(s): State<AppState>,
    Path(project_id): Path<i64>,
    Json(i): Json<SceneInput>,
) -> AppResult<(StatusCode, Json<Scene>)> {
    let row = sqlx::query_as(
        "INSERT INTO scenes (project_id, title, setting, summary, background_path, sort_order)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(project_id)
    .bind(i.title)
    .bind(i.setting)
    .bind(i.summary)
    .bind(i.background_path)
    .bind(i.sort_order)
    .fetch_one(&s.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(i): Json<SceneInput>,
) -> AppResult<Json<Scene>> {
    let row = sqlx::query_as(
        "UPDATE scenes SET title = ?, setting = ?, summary = ?, background_path = ?, sort_order = ?
         WHERE id = ? RETURNING *",
    )
    .bind(i.title)
    .bind(i.setting)
    .bind(i.summary)
    .bind(i.background_path)
    .bind(i.sort_order)
    .bind(id)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

/// シーンを消す。セリフとテイクは一緒に消え、背景画像とテイクの音声ファイルもここで消す。
async fn remove(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    let mut tx = s.db.begin().await?;
    let files: Vec<String> = sqlx::query_scalar(
        "SELECT background_path FROM scenes WHERE id = ?1 AND background_path IS NOT NULL
         UNION ALL SELECT t.path FROM takes t JOIN lines l ON l.id = t.line_id WHERE l.scene_id = ?1",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let done = sqlx::query("DELETE FROM scenes WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound.into());
    }
    tx.commit().await?;
    super::files::remove_unused(&s, files).await;
    Ok(StatusCode::NO_CONTENT)
}
