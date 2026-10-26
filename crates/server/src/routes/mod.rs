mod characters;
mod files;
pub(crate) mod generate;
mod lines;
mod memories;
mod projects;
mod scenes;
pub mod tts;

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
        .merge(memories::router())
        .merge(generate::router())
        .merge(files::router())
        .merge(tts::router())
        .merge(crate::settings::router())
}
