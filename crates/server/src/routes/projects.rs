use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::AppResult;

#[derive(Serialize, sqlx::FromRow)]
pub struct Project {
    id: i64,
    title: String,
    synopsis: String,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
pub struct ProjectInput {
    title: String,
    #[serde(default)]
    synopsis: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list).post(create))
        .route("/projects/{id}", get(show).put(update).delete(remove))
}

async fn list(State(s): State<AppState>) -> AppResult<Json<Vec<Project>>> {
    let rows = sqlx::query_as("SELECT * FROM projects ORDER BY updated_at DESC")
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

async fn show(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<Json<Project>> {
    let row = sqlx::query_as("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

async fn create(
    State(s): State<AppState>,
    Json(input): Json<ProjectInput>,
) -> AppResult<(StatusCode, Json<Project>)> {
    let row = sqlx::query_as("INSERT INTO projects (title, synopsis) VALUES (?, ?) RETURNING *")
        .bind(input.title)
        .bind(input.synopsis)
        .fetch_one(&s.db)
        .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<ProjectInput>,
) -> AppResult<Json<Project>> {
    let row = sqlx::query_as(
        "UPDATE projects SET title = ?, synopsis = ?, updated_at = datetime('now') WHERE id = ? RETURNING *",
    )
    .bind(input.title)
    .bind(input.synopsis)
    .bind(id)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

async fn remove(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
