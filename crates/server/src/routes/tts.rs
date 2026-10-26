//! 音声生成ジョブとテイク。
//!
//! 「音声生成」ボタンで選択したセリフを `POST /tts/jobs` に送ると、セリフごとにジョブを作って
//! TTS ワーカーのキューに積む。進捗は `GET /tts/events` (SSE) で通知し、完了したらテイクとして保存する。

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json as SqlJson;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::tts::{EngineStatus, SynthesisInput, SynthesisParams, compose_caption};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    id: String,
    line_id: i64,
    state: JobState,
    take_id: Option<i64>,
    error: Option<String>,
}

/// 実行中・最近のジョブと、その更新の配信。
pub struct JobBoard {
    jobs: Mutex<HashMap<String, Job>>,
    events: broadcast::Sender<Job>,
}

impl Default for JobBoard {
    fn default() -> Self {
        Self {
            jobs: Mutex::default(),
            events: broadcast::channel(256).0,
        }
    }
}

impl JobBoard {
    fn update(&self, id: &str, f: impl FnOnce(&mut Job)) {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.get_mut(id) {
            f(job);
            let _ = self.events.send(job.clone());
        }
    }

    fn insert(&self, job: Job) {
        let mut jobs = self.jobs.lock().unwrap();
        // 完了済みのジョブは溜めすぎない
        if jobs.len() > 500 {
            jobs.retain(|_, j| matches!(j.state, JobState::Queued | JobState::Running));
        }
        jobs.insert(job.id.clone(), job.clone());
        let _ = self.events.send(job);
    }
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Take {
    id: i64,
    line_id: i64,
    path: String,
    seed: Option<i64>,
    caption: String,
    params: SqlJson<Value>,
    adopted: bool,
    created_at: String,
    duration_ms: i64,
}

#[derive(sqlx::FromRow)]
struct LineSource {
    kind: String,
    text: String,
    tts_text: Option<String>,
    performance: SqlJson<Value>,
    caption_override: Option<String>,
    voice_ref_path: Option<String>,
    voice_caption: Option<String>,
}

impl LineSource {
    async fn load(db: &sqlx::SqlitePool, line_id: i64) -> AppResult<Self> {
        Ok(sqlx::query_as(
            "SELECT l.kind, l.text, l.tts_text, l.performance, l.caption_override, c.voice_ref_path, c.voice_caption
             FROM lines l LEFT JOIN characters c ON c.id = l.character_id WHERE l.id = ?",
        )
        .bind(line_id)
        .fetch_one(db)
        .await?)
    }

    fn caption(&self) -> (String, &'static str) {
        match self
            .caption_override
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(c) => (c.to_owned(), "override"),
            None => (
                compose_caption(
                    self.voice_caption.as_deref().unwrap_or(""),
                    &self.performance,
                ),
                "composed",
            ),
        }
    }

    fn speech_text(&self) -> &str {
        self.tts_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.text)
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tts/status", get(status))
        .route("/tts/jobs", get(list_jobs).post(create_jobs))
        .route("/tts/events", get(events))
        .route("/lines/{id}/caption", get(caption))
        .route("/lines/{id}/takes", get(list_takes))
        .route("/takes/{id}/adopt", put(adopt))
        .route("/takes/{id}", delete(remove_take))
}

#[derive(Serialize)]
struct StatusResponse {
    engine: EngineStatus,
    jobs: Vec<Job>,
}

async fn status(State(s): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        engine: s.tts.status(),
        jobs: active_jobs(&s.jobs),
    })
}

fn active_jobs(board: &JobBoard) -> Vec<Job> {
    board
        .jobs
        .lock()
        .unwrap()
        .values()
        .filter(|j| matches!(j.state, JobState::Queued | JobState::Running))
        .cloned()
        .collect()
}

async fn list_jobs(State(s): State<AppState>) -> Json<Vec<Job>> {
    Json(s.jobs.jobs.lock().unwrap().values().cloned().collect())
}

#[derive(Deserialize)]
struct CreateJobs {
    line_ids: Vec<i64>,
    /// 指定しなければセリフごとにランダム
    seed: Option<u64>,
    #[serde(default)]
    params: Option<SynthesisParams>,
}

async fn create_jobs(
    State(s): State<AppState>,
    Json(req): Json<CreateJobs>,
) -> AppResult<(StatusCode, Json<Vec<Job>>)> {
    if let EngineStatus::Disabled | EngineStatus::Failed { .. } = s.tts.status() {
        return Err(AppError::Unavailable("TTS engine is not available".into()));
    }
    // 先に全セリフを検証してから積む
    let mut sources = Vec::with_capacity(req.line_ids.len());
    for &line_id in &req.line_ids {
        let src = LineSource::load(&s.db, line_id).await?;
        if src.kind != "dialogue" {
            return Err(AppError::BadRequest(format!(
                "line {line_id} is not dialogue"
            )));
        }
        if src.speech_text().trim().is_empty() {
            return Err(AppError::BadRequest(format!("line {line_id} has no text")));
        }
        sources.push((line_id, src));
    }

    let params = req.params.unwrap_or_default();
    let mut jobs = Vec::new();
    for (line_id, src) in sources {
        let job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            line_id,
            state: JobState::Queued,
            take_id: None,
            error: None,
        };
        s.jobs.insert(job.clone());
        jobs.push(job.clone());

        let (caption, _) = src.caption();
        let input = SynthesisInput {
            text: src.speech_text().to_owned(),
            caption,
            reference: src.voice_ref_path.as_ref().map(|p| s.media_dir.join(p)),
            seed: req.seed.unwrap_or_else(random_seed),
            params: params.clone(),
        };
        let state = s.clone();
        tokio::spawn(async move { run_job(state, job.id, line_id, input).await });
    }
    Ok((StatusCode::ACCEPTED, Json(jobs)))
}

fn random_seed() -> u64 {
    // JSON / SQLite で扱いやすいよう 53 bit に収める
    uuid::Uuid::new_v4().as_u64_pair().0 >> 11
}

async fn run_job(s: AppState, job_id: String, line_id: i64, input: SynthesisInput) {
    let board = s.jobs.clone();
    let started = job_id.clone();
    let result = s
        .tts
        .synthesize(input.clone(), move || {
            board.update(&started, |j| j.state = JobState::Running)
        })
        .await;
    let outcome = match result {
        Ok(audio) => save_take(&s, line_id, &input, audio)
            .await
            .map_err(|e| format!("{e:#}")),
        Err(e) => Err(format!("{e:#}")),
    };
    s.jobs.update(&job_id, |j| match outcome {
        Ok(take_id) => {
            j.state = JobState::Done;
            j.take_id = Some(take_id);
        }
        Err(e) => {
            tracing::error!("tts job {} for line {line_id} failed: {e}", j.id);
            j.state = JobState::Failed;
            j.error = Some(e);
        }
    });
}

async fn save_take(
    s: &AppState,
    line_id: i64,
    input: &SynthesisInput,
    audio: crate::tts::Synthesized,
) -> anyhow::Result<i64> {
    let rel = format!("takes/{}.wav", uuid::Uuid::new_v4());
    let path = s.media_dir.join(&rel);
    tokio::fs::create_dir_all(path.parent().unwrap()).await?;
    let (samples, sr) = (audio.samples, audio.sample_rate);
    let duration_ms = (samples.len() as u64 * 1000 / sr as u64) as i64;
    let write_path = path.clone();
    tokio::task::spawn_blocking(move || irodori::audio::write_wav(&write_path, &samples, sr))
        .await??;

    let mut tx = s.db.begin().await?;
    let has_adopted: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM takes WHERE line_id = ? AND adopted = 1)")
            .bind(line_id)
            .fetch_one(&mut *tx)
            .await?;
    let id = sqlx::query_scalar(
        "INSERT INTO takes (line_id, path, seed, caption, params, adopted, duration_ms) VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(line_id)
    .bind(&rel)
    .bind(input.seed as i64)
    .bind(&input.caption)
    .bind(SqlJson(&input.params))
    .bind(!has_adopted)
    .bind(duration_ms)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

async fn events(
    State(s): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(s.jobs.events.subscribe())
        .filter_map(|job| job.ok())
        .map(|job| {
            Ok(Event::default()
                .event("job")
                .json_data(job)
                .expect("serializable"))
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Serialize)]
struct CaptionPreview {
    caption: String,
    /// "override" (手動指定) | "composed" (キャラの声 + 演技メタデータから生成)
    source: &'static str,
    text: String,
}

async fn caption(
    State(s): State<AppState>,
    Path(line_id): Path<i64>,
) -> AppResult<Json<CaptionPreview>> {
    let src = LineSource::load(&s.db, line_id).await?;
    let (caption, source) = src.caption();
    Ok(Json(CaptionPreview {
        caption,
        source,
        text: src.speech_text().to_owned(),
    }))
}

async fn list_takes(
    State(s): State<AppState>,
    Path(line_id): Path<i64>,
) -> AppResult<Json<Vec<Take>>> {
    let rows = sqlx::query_as("SELECT * FROM takes WHERE line_id = ? ORDER BY id DESC")
        .bind(line_id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

async fn adopt(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<Json<Take>> {
    let mut tx = s.db.begin().await?;
    let line_id: i64 = sqlx::query_scalar("SELECT line_id FROM takes WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("UPDATE takes SET adopted = (id = ?) WHERE line_id = ?")
        .bind(id)
        .bind(line_id)
        .execute(&mut *tx)
        .await?;
    let take = sqlx::query_as("SELECT * FROM takes WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(take))
}

async fn remove_take(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    let path: String = sqlx::query_scalar("DELETE FROM takes WHERE id = ? RETURNING path")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    if let Err(e) = tokio::fs::remove_file(s.media_dir.join(&path)).await {
        tracing::warn!("failed to remove {path}: {e}");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// AppState に持たせる共有オブジェクト。
pub type Jobs = Arc<JobBoard>;
