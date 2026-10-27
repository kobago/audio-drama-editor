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

/// 作品を消す。キャスト・シーン・セリフ・テイク・記憶は DB の外部キーで一緒に消え、
/// それらが指していた画像と音声のファイルもここで消す。
async fn remove(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    let mut tx = s.db.begin().await?;
    let files: Vec<String> = sqlx::query_scalar(
        "SELECT background_path FROM scenes WHERE project_id = ?1 AND background_path IS NOT NULL
         UNION ALL SELECT voice_ref_path FROM characters WHERE project_id = ?1 AND voice_ref_path IS NOT NULL
         UNION ALL SELECT p.path FROM character_portraits p
             JOIN characters c ON c.id = p.character_id WHERE c.project_id = ?1
         UNION ALL SELECT t.path FROM takes t
             JOIN lines l ON l.id = t.line_id JOIN scenes sc ON sc.id = l.scene_id WHERE sc.project_id = ?1",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let done = sqlx::query("DELETE FROM projects WHERE id = ?")
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
