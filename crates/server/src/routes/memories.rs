//! キャラクターの記憶と、セリフごとの記憶の検索。

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::types::Json as SqlJson;

use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::rag::{self, RagStatus};
use crate::recall::{self, Candidate, Context, Mark, MemoryDoc, Timing};

const KINDS: [&str; 5] = [
    "experience",
    "recollection",
    "knowledge",
    "relation",
    "habit",
];
/// 判断モデルに見せる直前のやりとりの行数
const HISTORY: i64 = 6;
/// 判断モデルに見せる人物の説明の長さ (文字)
const PROFILE_CHARS: usize = 60;

#[derive(Serialize, sqlx::FromRow)]
pub struct Memory {
    id: i64,
    character_id: i64,
    kind: String,
    period: String,
    people: SqlJson<Vec<String>>,
    text: String,
    /// 埋め込み済みか (検索の前に足りない分を作る)
    embedded: bool,
    created_at: String,
}

macro_rules! select_memory {
    ($tail:literal) => {
        concat!(
            "SELECT id, character_id, kind, period, people, text, embedding IS NOT NULL AS embedded, created_at
  FROM character_memories",
            " ",
            $tail
        )
    };
}

#[derive(Deserialize)]
pub struct MemoryInput {
    #[serde(default = "knowledge")]
    kind: String,
    #[serde(default)]
    period: String,
    #[serde(default)]
    people: Vec<String>,
    text: String,
}

fn knowledge() -> String {
    "knowledge".into()
}

impl MemoryInput {
    fn validate(mut self) -> AppResult<Self> {
        if !KINDS.contains(&self.kind.as_str()) {
            return Err(AppError::BadRequest(format!("unknown kind: {}", self.kind)));
        }
        self.text = self.text.trim().to_owned();
        if self.text.is_empty() {
            return Err(AppError::BadRequest("text is empty".into()));
        }
        self.period = self.period.trim().to_owned();
        self.people = self
            .people
            .into_iter()
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect();
        Ok(self)
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/characters/{id}/memories", get(list).post(create))
        .route("/memories/{id}", put(update).delete(remove))
        .route("/lines/{id}/recall", get(show_recall).post(run_recall))
        .route("/lines/{id}/memory-marks/{memory_id}", put(set_mark))
        .route("/rag/status", get(status))
}

async fn status(State(s): State<AppState>) -> Json<RagStatus> {
    Json(s.rag.status())
}

async fn list(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<Json<Vec<Memory>>> {
    let rows = sqlx::query_as(select_memory!("WHERE character_id = ? ORDER BY id"))
        .bind(id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

async fn create(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(i): Json<MemoryInput>,
) -> AppResult<(StatusCode, Json<Memory>)> {
    let i = i.validate()?;
    let (new_id,): (i64,) = sqlx::query_as(
        "INSERT INTO character_memories (character_id, kind, period, people, text) VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(id)
    .bind(i.kind)
    .bind(i.period)
    .bind(SqlJson(i.people))
    .bind(i.text)
    .fetch_one(&s.db)
    .await?;
    let row = sqlx::query_as(select_memory!("WHERE id = ?"))
        .bind(new_id)
        .fetch_one(&s.db)
        .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(i): Json<MemoryInput>,
) -> AppResult<Json<Memory>> {
    let i = i.validate()?;
    // 埋め込みは検索の前に、文書が変わっていれば作り直す (embedding_key で判断)
    sqlx::query("UPDATE character_memories SET kind = ?, period = ?, people = ?, text = ? WHERE id = ? RETURNING id")
        .bind(i.kind)
        .bind(i.period)
        .bind(SqlJson(i.people))
        .bind(i.text)
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    let row = sqlx::query_as(select_memory!("WHERE id = ?"))
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

async fn remove(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM character_memories WHERE id = ?")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- セリフごとの記憶 ---

#[derive(Serialize)]
pub struct RecallView {
    /// 最後の検索。まだ検索していなければ null
    recall: Option<RecallMeta>,
    /// 候補 (点数の高い順) と、候補に入っていない印付きの記憶
    items: Vec<RecallItem>,
}

#[derive(Serialize)]
struct RecallMeta {
    intent: String,
    created_at: String,
    timing: Timing,
}

#[derive(Serialize)]
struct RecallItem {
    memory: Memory,
    /// 検索の点数。候補に入っていなければ null
    candidate: Option<Candidate>,
    mark: Option<Mark>,
    /// セリフ生成に使うか
    selected: bool,
}

#[derive(sqlx::FromRow)]
struct RecallRow {
    intent: String,
    candidates: SqlJson<Vec<Candidate>>,
    timing: SqlJson<Timing>,
    created_at: String,
}

async fn marks(s: &AppState, line_id: i64) -> AppResult<HashMap<i64, Mark>> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT memory_id, mark FROM line_memory_marks WHERE line_id = ?")
            .bind(line_id)
            .fetch_all(&s.db)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(id, m)| (id, if m == "pin" { Mark::Pin } else { Mark::Exclude }))
        .collect())
}

async fn view(s: &AppState, line_id: i64) -> AppResult<RecallView> {
    let row: Option<RecallRow> = sqlx::query_as(
        "SELECT intent, candidates, timing, created_at FROM line_recalls WHERE line_id = ?",
    )
    .bind(line_id)
    .fetch_optional(&s.db)
    .await?;
    let marks = marks(s, line_id).await?;
    let candidates = row
        .as_ref()
        .map(|r| r.candidates.0.clone())
        .unwrap_or_default();
    let selected = recall::select(&candidates, &marks);

    // 候補の順に並べ、候補でない印付き (必ず使う → 使わない) を後ろに足す
    let mut order: Vec<(i64, Option<Candidate>)> = candidates
        .into_iter()
        .map(|c| (c.memory_id, Some(c)))
        .collect();
    let listed: HashSet<i64> = order.iter().map(|(id, _)| *id).collect();
    let mut rest: Vec<(&i64, &Mark)> = marks
        .iter()
        .filter(|(id, _)| !listed.contains(id))
        .collect();
    rest.sort_by_key(|(id, m)| (**m == Mark::Exclude, **id));
    order.extend(rest.into_iter().map(|(id, _)| (*id, None)));

    let mut items = Vec::with_capacity(order.len());
    for (id, candidate) in order {
        // 消した記憶は飛ばす
        let Some(memory) = sqlx::query_as::<_, Memory>(select_memory!("WHERE id = ?"))
            .bind(id)
            .fetch_optional(&s.db)
            .await?
        else {
            continue;
        };
        items.push(RecallItem {
            memory,
            candidate,
            mark: marks.get(&id).copied(),
            selected: selected.contains(&id),
        });
    }
    Ok(RecallView {
        recall: row.map(|r| RecallMeta {
            intent: r.intent,
            created_at: r.created_at,
            timing: r.timing.0,
        }),
        items,
    })
}

async fn show_recall(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<RecallView>> {
    // セリフが無ければ 404
    sqlx::query("SELECT id FROM lines WHERE id = ?")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(view(&s, id).await?))
}

#[derive(Deserialize, Default)]
pub struct RecallInput {
    /// これから話すこと。空ならセリフの本文を使う
    #[serde(default)]
    intent: Option<String>,
}

#[derive(sqlx::FromRow)]
struct LineRow {
    scene_id: i64,
    kind: String,
    character_id: Option<i64>,
    text: String,
    intent: String,
    sort_order: i64,
}

#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: i64,
    kind: String,
    period: String,
    people: SqlJson<Vec<String>>,
    text: String,
    embedding: Option<Vec<u8>>,
    embedding_key: Option<String>,
}

fn to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_bytes(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

fn short_profile(profile: &str) -> String {
    let first = profile
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let first = first.trim_end_matches('。');
    if first.chars().count() > PROFILE_CHARS {
        first.chars().take(PROFILE_CHARS).chain(['…']).collect()
    } else {
        first.to_owned()
    }
}

async fn context(s: &AppState, line_id: i64, intent: Option<String>) -> AppResult<(Context, i64)> {
    let line: LineRow = sqlx::query_as(
        "SELECT scene_id, kind, character_id, text, intent, sort_order FROM lines WHERE id = ?",
    )
    .bind(line_id)
    .fetch_one(&s.db)
    .await?;
    let character_id = match (line.kind.as_str(), line.character_id) {
        ("dialogue", Some(c)) => c,
        _ => {
            return Err(AppError::BadRequest(
                "話者のいるセリフでだけ記憶を探せます".into(),
            ));
        }
    };
    let intent = intent
        .map(|i| i.trim().to_owned())
        .filter(|i| !i.is_empty())
        .or_else(|| Some(line.intent.trim().to_owned()).filter(|t| !t.is_empty()))
        .or_else(|| Some(line.text.trim().to_owned()).filter(|t| !t.is_empty()))
        .ok_or_else(|| {
            AppError::BadRequest("セリフか、これから話すことを入力してください".into())
        })?;
    let (name, profile): (String, String) =
        sqlx::query_as("SELECT name, profile FROM characters WHERE id = ?")
            .bind(character_id)
            .fetch_one(&s.db)
            .await?;
    let (title, setting, summary): (String, String, String) =
        sqlx::query_as("SELECT title, setting, summary FROM scenes WHERE id = ?")
            .bind(line.scene_id)
            .fetch_one(&s.db)
            .await?;
    let mut history: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT l.kind, l.text, c.name FROM lines l LEFT JOIN characters c ON c.id = l.character_id
          WHERE l.scene_id = ? AND trim(l.text) != ''
            AND (l.sort_order < ? OR (l.sort_order = ? AND l.id < ?))
          ORDER BY l.sort_order DESC, l.id DESC LIMIT ?",
    )
    .bind(line.scene_id)
    .bind(line.sort_order)
    .bind(line.sort_order)
    .bind(line_id)
    .bind(HISTORY)
    .fetch_all(&s.db)
    .await?;
    history.reverse();
    let setting = [setting, summary, title]
        .into_iter()
        .map(|t| t.trim().to_owned())
        .find(|t| !t.is_empty())
        .unwrap_or_default();
    Ok((
        Context {
            speaker: name,
            profile: short_profile(&profile),
            setting,
            history: history
                .into_iter()
                .map(|(kind, text, who)| (if kind == "dialogue" { who } else { None }, text))
                .collect(),
            intent,
        },
        character_id,
    ))
}

/// キャラクターの記憶を、足りない・古い埋め込みを作り直してから読む。
async fn embedded_memories(
    s: &AppState,
    character_id: i64,
    speaker: &str,
) -> AppResult<Vec<(MemoryDoc, Vec<f32>)>> {
    let rows: Vec<MemoryRow> = sqlx::query_as(
        "SELECT id, kind, period, people, text, embedding, embedding_key FROM character_memories
          WHERE character_id = ? ORDER BY id",
    )
    .bind(character_id)
    .fetch_all(&s.db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    let mut stale = Vec::new();
    for r in rows {
        let doc = MemoryDoc {
            id: r.id,
            kind: r.kind,
            period: r.period,
            people: r.people.0,
            text: r.text,
        };
        let key = format!(
            "{}\n{}",
            rag::EMBEDDING_MODEL,
            recall::document_text(speaker, &doc)
        );
        match r.embedding {
            Some(e) if r.embedding_key.as_deref() == Some(key.as_str()) => {
                out.push((doc, from_bytes(&e)))
            }
            _ => stale.push((doc, key)),
        }
    }
    if !stale.is_empty() {
        let texts = stale
            .iter()
            .map(|(doc, _)| recall::document_text(speaker, doc))
            .collect();
        let vectors = s.rag.embed(texts).await.map_err(unavailable)?;
        for ((doc, key), v) in stale.into_iter().zip(vectors) {
            sqlx::query(
                "UPDATE character_memories SET embedding = ?, embedding_key = ? WHERE id = ?",
            )
            .bind(to_bytes(&v))
            .bind(key)
            .bind(doc.id)
            .execute(&s.db)
            .await?;
            out.push((doc, v));
        }
    }
    Ok(out)
}

fn unavailable(e: anyhow::Error) -> AppError {
    AppError::Unavailable(format!("記憶の検索ができません: {e:#}"))
}

/// セリフの記憶を探し、結果を保存する。
async fn search(s: &AppState, id: i64, intent: Option<String>) -> AppResult<()> {
    let (ctx, character_id) = context(s, id, intent).await?;
    if !s.rag.enabled() {
        return Err(unavailable(anyhow::anyhow!("disabled (ADE_RAG=off)")));
    }
    let marks = marks(s, id).await?;
    let memories: Vec<(MemoryDoc, Vec<f32>)> = embedded_memories(s, character_id, &ctx.speaker)
        .await?
        .into_iter()
        .filter(|(m, _)| marks.get(&m.id) != Some(&Mark::Exclude))
        .collect();
    let (candidates, timing) = recall::run(&s.rag, &ctx, &memories)
        .await
        .map_err(unavailable)?;
    tracing::info!(
        line = id,
        candidates = candidates.len(),
        embed_ms = timing.embed_ms,
        judge_ms = timing.judge_ms,
        "recall"
    );
    sqlx::query(
        "INSERT INTO line_recalls (line_id, intent, candidates, timing) VALUES (?, ?, ?, ?)
         ON CONFLICT (line_id) DO UPDATE SET intent = excluded.intent, candidates = excluded.candidates,
           timing = excluded.timing, created_at = datetime('now')",
    )
    .bind(id)
    .bind(&ctx.intent)
    .bind(SqlJson(&candidates))
    .bind(SqlJson(&timing))
    .execute(&s.db)
    .await?;
    Ok(())
}

async fn run_recall(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    body: Option<Json<RecallInput>>,
) -> AppResult<Json<RecallView>> {
    let input = body.map(|Json(b)| b).unwrap_or_default();
    search(&s, id, input.intent).await?;
    Ok(Json(view(&s, id).await?))
}

/// セリフ生成で使う記憶。行の intent で探し直し、使う記憶 (必ず使うものを含む) を返す。
///
/// 記憶の検索を使えない (無効・記憶がない・失敗) ときは、必ず使う記憶だけを返す。
pub(crate) async fn memories_for_writing(s: &AppState, line_id: i64) -> AppResult<Vec<MemoryDoc>> {
    if let Err(e) = search(s, line_id, None).await {
        tracing::warn!(line = line_id, "recall skipped: {e}");
    }
    let view = view(s, line_id).await?;
    Ok(view
        .items
        .into_iter()
        .filter(|i| i.selected)
        .map(|i| MemoryDoc {
            id: i.memory.id,
            kind: i.memory.kind,
            period: i.memory.period,
            people: i.memory.people.0,
            text: i.memory.text,
        })
        .collect())
}

#[derive(Deserialize)]
pub struct MarkInput {
    mark: Option<Mark>,
}

async fn set_mark(
    State(s): State<AppState>,
    Path((line_id, memory_id)): Path<(i64, i64)>,
    Json(i): Json<MarkInput>,
) -> AppResult<Json<RecallView>> {
    match i.mark {
        None => {
            sqlx::query("DELETE FROM line_memory_marks WHERE line_id = ? AND memory_id = ?")
                .bind(line_id)
                .bind(memory_id)
                .execute(&s.db)
                .await?;
        }
        Some(mark) => {
            sqlx::query(
                "INSERT INTO line_memory_marks (line_id, memory_id, mark) VALUES (?, ?, ?)
                 ON CONFLICT (line_id, memory_id) DO UPDATE SET mark = excluded.mark",
            )
            .bind(line_id)
            .bind(memory_id)
            .bind(if mark == Mark::Pin { "pin" } else { "exclude" })
            .execute(&s.db)
            .await?;
        }
    }
    Ok(Json(view(&s, line_id).await?))
}
