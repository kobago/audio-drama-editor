use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json as SqlJson;

use crate::AppState;
use crate::error::AppResult;

#[derive(Serialize, sqlx::FromRow)]
pub struct Character {
    id: i64,
    project_id: i64,
    name: String,
    reading: String,
    profile: String,
    speech_style: String,
    sample_lines: SqlJson<Value>,
    lora_id: Option<i64>,
    lora_scale: f64,
    voice_ref_path: Option<String>,
    voice_caption: String,
    voice_seed: Option<i64>,
    sort_order: i64,
    created_at: String,
}

#[derive(Deserialize)]
pub struct CharacterInput {
    name: String,
    #[serde(default)]
    reading: String,
    #[serde(default)]
    profile: String,
    #[serde(default)]
    speech_style: String,
    #[serde(default = "empty_array")]
    sample_lines: Value,
    lora_id: Option<i64>,
    #[serde(default = "one")]
    lora_scale: f64,
    voice_ref_path: Option<String>,
    #[serde(default)]
    voice_caption: String,
    voice_seed: Option<i64>,
    #[serde(default)]
    sort_order: i64,
}

fn empty_array() -> Value {
    Value::Array(vec![])
}

fn one() -> f64 {
    1.0
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Fact {
    id: i64,
    character_id: i64,
    text: String,
}

#[derive(Deserialize)]
pub struct FactInput {
    text: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Portrait {
    id: i64,
    character_id: i64,
    label: String,
    path: String,
}

#[derive(Deserialize)]
pub struct PortraitInput {
    #[serde(default)]
    label: String,
    path: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/{id}/characters", get(list).post(create))
        .route("/characters/{id}", get(show).put(update).delete(remove))
        .route("/characters/{id}/facts", get(list_facts).post(create_fact))
        .route("/facts/{id}", delete(remove_fact))
        .route(
            "/characters/{id}/portraits",
            get(list_portraits).post(create_portrait),
        )
        .route("/portraits/{id}", delete(remove_portrait))
}

async fn list(
    State(s): State<AppState>,
    Path(project_id): Path<i64>,
) -> AppResult<Json<Vec<Character>>> {
    let rows =
        sqlx::query_as("SELECT * FROM characters WHERE project_id = ? ORDER BY sort_order, id")
            .bind(project_id)
            .fetch_all(&s.db)
            .await?;
    Ok(Json(rows))
}

async fn show(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<Json<Character>> {
    let row = sqlx::query_as("SELECT * FROM characters WHERE id = ?")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

async fn create(
    State(s): State<AppState>,
    Path(project_id): Path<i64>,
    Json(i): Json<CharacterInput>,
) -> AppResult<(StatusCode, Json<Character>)> {
    let row = sqlx::query_as(
        "INSERT INTO characters (project_id, name, reading, profile, speech_style, sample_lines, lora_id, lora_scale,
            voice_ref_path, voice_caption, voice_seed, sort_order)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(project_id)
    .bind(i.name)
    .bind(i.reading)
    .bind(i.profile)
    .bind(i.speech_style)
    .bind(SqlJson(i.sample_lines))
    .bind(i.lora_id)
    .bind(i.lora_scale)
    .bind(i.voice_ref_path)
    .bind(i.voice_caption)
    .bind(i.voice_seed)
    .bind(i.sort_order)
    .fetch_one(&s.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(i): Json<CharacterInput>,
) -> AppResult<Json<Character>> {
    let row = sqlx::query_as(
        "UPDATE characters SET name = ?, reading = ?, profile = ?, speech_style = ?, sample_lines = ?, lora_id = ?,
            lora_scale = ?, voice_ref_path = ?, voice_caption = ?, voice_seed = ?, sort_order = ?
         WHERE id = ? RETURNING *",
    )
    .bind(i.name)
    .bind(i.reading)
    .bind(i.profile)
    .bind(i.speech_style)
    .bind(SqlJson(i.sample_lines))
    .bind(i.lora_id)
    .bind(i.lora_scale)
    .bind(i.voice_ref_path)
    .bind(i.voice_caption)
    .bind(i.voice_seed)
    .bind(i.sort_order)
    .bind(id)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

async fn remove(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM characters WHERE id = ?")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_facts(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<Json<Vec<Fact>>> {
    let rows = sqlx::query_as(
        "SELECT id, character_id, text FROM character_facts WHERE character_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

async fn create_fact(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(i): Json<FactInput>,
) -> AppResult<(StatusCode, Json<Fact>)> {
    let row = sqlx::query_as("INSERT INTO character_facts (character_id, text) VALUES (?, ?) RETURNING id, character_id, text")
        .bind(id)
        .bind(i.text)
        .fetch_one(&s.db)
        .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn remove_fact(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM character_facts WHERE id = ?")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_portraits(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<Portrait>>> {
    let rows =
        sqlx::query_as("SELECT * FROM character_portraits WHERE character_id = ? ORDER BY id")
            .bind(id)
            .fetch_all(&s.db)
            .await?;
    Ok(Json(rows))
}

async fn create_portrait(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(i): Json<PortraitInput>,
) -> AppResult<(StatusCode, Json<Portrait>)> {
    let row = sqlx::query_as(
        "INSERT INTO character_portraits (character_id, label, path) VALUES (?, ?, ?) RETURNING *",
    )
    .bind(id)
    .bind(i.label)
    .bind(i.path)
    .fetch_one(&s.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn remove_portrait(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM character_portraits WHERE id = ?")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
