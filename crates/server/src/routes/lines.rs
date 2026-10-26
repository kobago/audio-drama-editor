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
    portrait_id: Option<i64>,
    /// 採用中のテイク (一覧表示用。テイクの詳細は /lines/{id}/takes)
    adopted_take_id: Option<i64>,
    adopted_take_path: Option<String>,
    adopted_duration_ms: Option<i64>,
    take_count: i64,
}

/// 採用テイクとテイク数を付けて行を読む SELECT。WHERE 以降を引数で足す。
macro_rules! select_line {
    ($tail:literal) => {
        concat!(
            "SELECT l.*,
    a.id AS adopted_take_id, a.path AS adopted_take_path, a.duration_ms AS adopted_duration_ms,
    (SELECT COUNT(*) FROM takes c WHERE c.line_id = l.id) AS take_count
  FROM lines l LEFT JOIN takes a ON a.line_id = l.id AND a.adopted = 1",
            " ",
            $tail
        )
    };
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
    /// 省略時: 作成なら末尾、更新なら現在の位置のまま
    sort_order: Option<i64>,
    portrait_id: Option<i64>,
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
    let rows = sqlx::query_as(select_line!(
        "WHERE l.scene_id = ? ORDER BY l.sort_order, l.id"
    ))
    .bind(scene_id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

async fn fetch_line(db: &sqlx::SqlitePool, id: i64) -> AppResult<Line> {
    Ok(sqlx::query_as(select_line!("WHERE l.id = ?"))
        .bind(id)
        .fetch_one(db)
        .await?)
}

async fn show(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<Json<Line>> {
    Ok(Json(fetch_line(&s.db, id).await?))
}

async fn create(
    State(s): State<AppState>,
    Path(scene_id): Path<i64>,
    Json(i): Json<LineInput>,
) -> AppResult<(StatusCode, Json<Line>)> {
    i.validate()?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO lines (scene_id, kind, character_id, text, tts_text, length_class, performance, caption_override,
            pause_before_ms, pause_after_ms, sort_order, portrait_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
            COALESCE(?, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM lines WHERE scene_id = ?)), ?)
         RETURNING id",
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
    .bind(scene_id)
    .bind(i.portrait_id)
    .fetch_one(&s.db)
    .await?;
    Ok((StatusCode::CREATED, Json(fetch_line(&s.db, id).await?)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(i): Json<LineInput>,
) -> AppResult<Json<Line>> {
    i.validate()?;
    sqlx::query(
        "UPDATE lines SET kind = ?, character_id = ?, text = ?, tts_text = ?, length_class = ?, performance = ?,
            caption_override = ?, pause_before_ms = ?, pause_after_ms = ?, sort_order = COALESCE(?, sort_order),
            portrait_id = ?
         WHERE id = ?",
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
    .bind(i.portrait_id)
    .bind(id)
    .execute(&s.db)
    .await?;
    Ok(Json(fetch_line(&s.db, id).await?))
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
