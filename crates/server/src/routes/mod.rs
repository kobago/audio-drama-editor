mod characters;
mod files;
mod lines;
mod projects;
mod scenes;

use axum::Router;
use axum::routing::get;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(projects::router())
        .merge(characters::router())
        .merge(scenes::router())
        .merge(lines::router())
        .merge(files::router())
}
