use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json as SqlJson;

use crate::AppState;
use crate::error::{AppError, AppResult};

#[derive(Serialize, sqlx::FromRow)]
pub struct Line {
    id: i64,
    scene_id: i64,
    kind: String,
    character_id: Option<i64>,
    text: String,
    tts_text: Option<String>,
    length_class: Option<String>,
    performance: SqlJson<Value>,
    caption_override: Option<String>,
    pause_before_ms: i64,
    pause_after_ms: i64,
    sort_order: i64,
}

#[derive(Deserialize)]
pub struct LineInput {
    #[serde(default = "dialogue")]
    kind: String,
    character_id: Option<i64>,
    #[serde(default)]
    text: String,
    tts_text: Option<String>,
    length_class: Option<String>,
    #[serde(default = "empty_object")]
    performance: Value,
    caption_override: Option<String>,
    #[serde(default)]
    pause_before_ms: i64,
    #[serde(default = "default_pause_after")]
    pause_after_ms: i64,
    #[serde(default)]
    sort_order: i64,
}

fn dialogue() -> String {
    "dialogue".into()
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

fn default_pause_after() -> i64 {
    300
}

impl LineInput {
    fn validate(&self) -> AppResult<()> {
        if !matches!(self.kind.as_str(), "dialogue" | "direction") {
            return Err(AppError::BadRequest(format!("invalid kind: {}", self.kind)));
        }
        if let Some(c) = &self.length_class
            && !matches!(c.as_str(), "S" | "M" | "L")
        {
            return Err(AppError::BadRequest(format!("invalid length_class: {c}")));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct ReorderInput {
    /// 新しい並び順の line id
    ids: Vec<i64>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/scenes/{id}/lines", get(list).post(create))
        .route("/scenes/{id}/lines/order", put(reorder))
        .route("/lines/{id}", get(show).put(update).delete(remove))
}

async fn list(State(s): State<AppState>, Path(scene_id): Path<i64>) -> AppResult<Json<Vec<Line>>> {
    let rows = sqlx::query_as("SELECT * FROM lines WHERE scene_id = ? ORDER BY sort_order, id")
        .bind(scene_id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

async fn show(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<Json<Line>> {
    let row = sqlx::query_as("SELECT * FROM lines WHERE id = ?")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

async fn create(
    State(s): State<AppState>,
    Path(scene_id): Path<i64>,
    Json(i): Json<LineInput>,
) -> AppResult<(StatusCode, Json<Line>)> {
    i.validate()?;
    let row = sqlx::query_as(
        "INSERT INTO lines (scene_id, kind, character_id, text, tts_text, length_class, performance, caption_override,
            pause_before_ms, pause_after_ms, sort_order)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(scene_id)
    .bind(i.kind)
    .bind(i.character_id)
    .bind(i.text)
    .bind(i.tts_text)
    .bind(i.length_class)
    .bind(SqlJson(i.performance))
    .bind(i.caption_override)
    .bind(i.pause_before_ms)
    .bind(i.pause_after_ms)
    .bind(i.sort_order)
    .fetch_one(&s.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(i): Json<LineInput>,
) -> AppResult<Json<Line>> {
    i.validate()?;
    let row = sqlx::query_as(
        "UPDATE lines SET kind = ?, character_id = ?, text = ?, tts_text = ?, length_class = ?, performance = ?,
            caption_override = ?, pause_before_ms = ?, pause_after_ms = ?, sort_order = ?
         WHERE id = ? RETURNING *",
    )
    .bind(i.kind)
    .bind(i.character_id)
    .bind(i.text)
    .bind(i.tts_text)
    .bind(i.length_class)
    .bind(SqlJson(i.performance))
    .bind(i.caption_override)
    .bind(i.pause_before_ms)
    .bind(i.pause_after_ms)
    .bind(i.sort_order)
    .bind(id)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

async fn reorder(
    State(s): State<AppState>,
    Path(scene_id): Path<i64>,
    Json(i): Json<ReorderInput>,
) -> AppResult<StatusCode> {
    let mut tx = s.db.begin().await?;
    for (order, id) in i.ids.iter().enumerate() {
        sqlx::query("UPDATE lines SET sort_order = ? WHERE id = ? AND scene_id = ?")
            .bind(order as i64)
            .bind(id)
            .bind(scene_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM lines WHERE id = ?")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
