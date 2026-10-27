//! LLM によるセリフと演技の生成。
//!
//! - 「続きを書く」: 計画をその場で立てて行を台本に入れ、セリフは 1 行ずつ裏で書く
//!   (書いている行は `gen_state = 'generating'`)。
//! - 「書き直す」: 今の案を `line_versions` に残してから、同じ意図 (または新しい意図) で書き直す。
//! - 「演技を付ける」: 既にあるセリフに演技だけを付ける。

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json as SqlJson;

use super::lines::{Line, fetch_line};
use super::memories::memories_for_writing;
use crate::AppState;
use crate::error::{AppError, AppResult};
use crate::writer::{self, CastMember, Planned, SceneInfo, ScriptLine, Writer};

const MAX_COUNT: usize = 8;
/// プロンプトに入れる、前の台本の行数
const BEFORE: usize = 40;
/// プロンプトに入れる、後に続く台本の行数
const AFTER: usize = 6;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/scenes/{id}/generate", post(generate))
        .route("/generations/{id}/redo", post(redo))
        .route("/lines/{id}/regenerate", post(regenerate))
        .route("/lines/{id}/versions", get(versions))
        .route("/line-versions/{id}/restore", post(restore))
        .route("/scenes/{id}/direct", post(direct))
}

fn bad_gateway(e: anyhow::Error) -> AppError {
    AppError::Unavailable(format!("LLM で生成できません: {e:#}"))
}

// --- 文脈の読み込み ---

async fn scene_info(s: &AppState, scene_id: i64) -> AppResult<(SceneInfo, i64)> {
    let (project_id, title, setting, summary, project_title, synopsis): (
        i64,
        String,
        String,
        String,
        String,
        String,
    ) = sqlx::query_as(
        "SELECT p.id, sc.title, sc.setting, sc.summary, p.title, p.synopsis
               FROM scenes sc JOIN projects p ON p.id = sc.project_id WHERE sc.id = ?",
    )
    .bind(scene_id)
    .fetch_one(&s.db)
    .await?;
    Ok((
        SceneInfo {
            project_title,
            synopsis,
            scene_title: title,
            setting,
            summary,
        },
        project_id,
    ))
}

#[derive(sqlx::FromRow)]
struct CastRow {
    id: i64,
    name: String,
    profile: String,
    speech_style: String,
    sample_lines: SqlJson<Value>,
    lora_id: Option<i64>,
    lora_scale: f64,
}

async fn cast(s: &AppState, project_id: i64) -> AppResult<Vec<CastMember>> {
    let rows: Vec<CastRow> = sqlx::query_as(
        "SELECT id, name, profile, speech_style, sample_lines, lora_id, lora_scale
           FROM characters WHERE project_id = ? ORDER BY sort_order, id",
    )
    .bind(project_id)
    .fetch_all(&s.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| CastMember {
            id: r.id,
            name: r.name,
            profile: r.profile,
            speech_style: r.speech_style,
            samples: r
                .sample_lines
                .0
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| {
                            Some((
                                x["length"].as_str()?.to_owned(),
                                x["text"].as_str()?.to_owned(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            lora: r.lora_id.map(|id| (id, r.lora_scale)),
        })
        .collect())
}

/// シーンの台本 (並び順)。生成中で本文のない行も含む。
async fn script(s: &AppState, scene_id: i64) -> AppResult<Vec<(i64, ScriptLine)>> {
    let rows: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT l.id, l.kind, l.text, c.name FROM lines l LEFT JOIN characters c ON c.id = l.character_id
          WHERE l.scene_id = ? ORDER BY l.sort_order, l.id",
    )
    .bind(scene_id)
    .fetch_all(&s.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, kind, text, name)| {
            let speaker = if kind == "dialogue" {
                Some(name.unwrap_or_else(|| "(話者なし)".into()))
            } else {
                None
            };
            (id, ScriptLine { speaker, text })
        })
        .collect())
}

/// `pos` の位置 (その行を含まない) の前後の台本。
fn around(
    script: &[(i64, ScriptLine)],
    pos: usize,
    skip: &[i64],
) -> (Vec<ScriptLine>, Vec<ScriptLine>) {
    let keep = |(id, l): &(i64, ScriptLine)| (!skip.contains(id)).then(|| l.clone());
    let before: Vec<ScriptLine> = script[..pos].iter().filter_map(keep).collect();
    let before = before[before.len().saturating_sub(BEFORE)..].to_vec();
    let after: Vec<ScriptLine> = script[pos..].iter().filter_map(keep).take(AFTER).collect();
    (before, after)
}

fn session(scene_id: i64) -> String {
    format!("audio-drama-editor-scene-{scene_id}")
}

// --- 続きを書く ---

#[derive(Deserialize)]
pub struct GenerateInput {
    /// この行の後ろに入れる。省略ならシーンの末尾
    after_line_id: Option<i64>,
    #[serde(default = "default_count")]
    count: usize,
    /// 次に起きることの指示
    #[serde(default)]
    direction: String,
}

fn default_count() -> usize {
    4
}

#[derive(Serialize)]
pub struct Generated {
    generation_id: i64,
    lines: Vec<Line>,
}

async fn generate(
    State(s): State<AppState>,
    Path(scene_id): Path<i64>,
    Json(i): Json<GenerateInput>,
) -> AppResult<(StatusCode, Json<Generated>)> {
    if !(1..=MAX_COUNT).contains(&i.count) {
        return Err(AppError::BadRequest(format!(
            "行数は 1〜{MAX_COUNT} で指定してください"
        )));
    }
    let generation_id: i64 = sqlx::query_scalar(
        "INSERT INTO generations (scene_id, after_line_id, count, direction) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(scene_id)
    .bind(i.after_line_id)
    .bind(i.count as i64)
    .bind(&i.direction)
    .fetch_one(&s.db)
    .await?;
    match run_generation(&s, generation_id).await {
        Ok(lines) => Ok((
            StatusCode::CREATED,
            Json(Generated {
                generation_id,
                lines,
            }),
        )),
        Err(e) => {
            // 計画に失敗したら生成の記録も残さない
            sqlx::query("DELETE FROM generations WHERE id = ?")
                .bind(generation_id)
                .execute(&s.db)
                .await?;
            Err(e)
        }
    }
}

/// 同じ条件でまとめて作り直す。前に入れた行は消す。
async fn redo(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<(StatusCode, Json<Generated>)> {
    sqlx::query("SELECT id FROM generations WHERE id = ?")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    sqlx::query("DELETE FROM lines WHERE generation_id = ?")
        .bind(id)
        .execute(&s.db)
        .await?;
    let lines = run_generation(&s, id).await?;
    Ok((
        StatusCode::CREATED,
        Json(Generated {
            generation_id: id,
            lines,
        }),
    ))
}

/// 計画を立てて行を入れ、セリフを裏で書き始める。
async fn run_generation(s: &AppState, generation_id: i64) -> AppResult<Vec<Line>> {
    let (scene_id, after_line_id, count, direction): (i64, Option<i64>, i64, String) =
        sqlx::query_as(
            "SELECT scene_id, after_line_id, count, direction FROM generations WHERE id = ?",
        )
        .bind(generation_id)
        .fetch_one(&s.db)
        .await?;
    let (scene, project_id) = scene_info(s, scene_id).await?;
    let cast = cast(s, project_id).await?;
    if cast.is_empty() {
        return Err(AppError::BadRequest(
            "先にキャストを登録してください".into(),
        ));
    }
    let script = script(s, scene_id).await?;
    let pos = match after_line_id {
        Some(id) => script
            .iter()
            .position(|(l, _)| *l == id)
            .map(|p| p + 1)
            .unwrap_or(script.len()),
        None => script.len(),
    };
    let (before, after) = around(&script, pos, &[]);

    let llm = crate::settings::llm(s).await?;
    let writer = Writer::new(&llm, &session(scene_id)).map_err(bad_gateway)?;
    let messages =
        writer::plan_messages(&scene, &cast, &before, &after, count as usize, &direction);
    let plan = writer
        .json(&messages, 0.7, None)
        .await
        .and_then(|v| writer::parse_plan(&v, &cast, count as usize))
        .map_err(bad_gateway)?;

    // 計画どおりに行を入れ、並び順を振り直す
    let mut tx = s.db.begin().await?;
    let mut new_ids = Vec::with_capacity(plan.len());
    for p in &plan {
        let id: i64 = match p {
            Planned::Dialogue {
                character_id,
                intent,
                length,
            } => sqlx::query_scalar(
                "INSERT INTO lines (scene_id, kind, character_id, text, length_class, intent, gen_state, generation_id)
                 VALUES (?, 'dialogue', ?, '', ?, ?, 'generating', ?) RETURNING id",
            )
            .bind(scene_id)
            .bind(character_id)
            .bind(length)
            .bind(intent)
            .bind(generation_id)
            .fetch_one(&mut *tx)
            .await?,
            Planned::Direction { text } => sqlx::query_scalar(
                "INSERT INTO lines (scene_id, kind, text, generation_id) VALUES (?, 'direction', ?, ?) RETURNING id",
            )
            .bind(scene_id)
            .bind(text)
            .bind(generation_id)
            .fetch_one(&mut *tx)
            .await?,
        };
        new_ids.push(id);
    }
    let mut order: Vec<i64> = script.iter().map(|(id, _)| *id).collect();
    order.splice(pos..pos, new_ids.iter().copied());
    for (k, id) in order.iter().enumerate() {
        sqlx::query("UPDATE lines SET sort_order = ? WHERE id = ?")
            .bind(k as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    let pending: Vec<i64> = plan
        .iter()
        .zip(&new_ids)
        .filter(|(p, _)| matches!(p, Planned::Dialogue { .. }))
        .map(|(_, id)| *id)
        .collect();
    spawn_writing(s.clone(), pending);

    let mut lines = Vec::with_capacity(new_ids.len());
    for id in new_ids {
        lines.push(fetch_line(&s.db, id).await?);
    }
    Ok(lines)
}

/// セリフを順に書く (前の行の本文を次の行の文脈にするため、1 行ずつ)。
fn spawn_writing(s: AppState, ids: Vec<i64>) {
    if ids.is_empty() {
        return;
    }
    tokio::spawn(async move {
        for id in ids {
            if let Err(e) = write_line(&s, id).await {
                tracing::warn!(line = id, "writing failed: {e:#}");
                let _ = sqlx::query(
                    "UPDATE lines SET gen_state = 'failed', gen_error = ? WHERE id = ? AND gen_state = 'generating'",
                )
                .bind(format!("{e:#}"))
                .bind(id)
                .execute(&s.db)
                .await;
            }
        }
    });
}

#[derive(sqlx::FromRow)]
struct WriteTarget {
    scene_id: i64,
    character_id: Option<i64>,
    intent: String,
    length_class: Option<String>,
    gen_state: Option<String>,
}

async fn write_line(s: &AppState, id: i64) -> anyhow::Result<()> {
    // 作り直しで消えた行や、書くのをやめた行は飛ばす
    let Some(t): Option<WriteTarget> = sqlx::query_as(
        "SELECT scene_id, character_id, intent, length_class, gen_state FROM lines WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&s.db)
    .await?
    else {
        return Ok(());
    };
    if t.gen_state.as_deref() != Some("generating") {
        return Ok(());
    }
    let (scene, project_id) = scene_info(s, t.scene_id).await?;
    let cast = cast(s, project_id).await?;
    let speaker = cast
        .iter()
        .find(|c| Some(c.id) == t.character_id)
        .ok_or_else(|| anyhow::anyhow!("話者がいません"))?
        .clone();
    let length = t.length_class.unwrap_or_else(|| "M".into());

    // 記憶は毎回探す (行の intent で)
    let memories = memories_for_writing(s, id).await.unwrap_or_default();

    let script = script(s, t.scene_id).await?;
    let pos = script
        .iter()
        .position(|(l, _)| *l == id)
        .ok_or_else(|| anyhow::anyhow!("行が見つかりません"))?;
    let (before, after) = around(&script, pos, &[id]);

    let llm = crate::settings::llm(s).await?;
    let writer = Writer::new(&llm, &session(t.scene_id))?;
    let mut best: Option<(String, Value)> = None;
    let mut feedback: Option<String> = None;
    for _ in 0..=writer::MAX_RETRIES {
        let messages = writer::write_messages(
            &scene,
            &speaker,
            &cast,
            &before,
            &after,
            &t.intent,
            &length,
            &memories,
            feedback.as_deref(),
        );
        let value = writer.json(&messages, 0.9, speaker.lora).await?;
        let (text, perf) = writer::parse_written(&value, &speaker.name)?;
        let miss = writer::length_miss(&text, &length);
        feedback = writer::length_feedback(&text, &length);
        if best
            .as_ref()
            .is_none_or(|(b, _)| miss < writer::length_miss(b, &length))
        {
            best = Some((text, perf));
        }
        if feedback.is_none() {
            break;
        }
    }
    let (text, perf) = best.expect("at least one attempt");
    sqlx::query(
        "UPDATE lines SET text = ?, performance = ?, gen_state = NULL, gen_error = NULL
          WHERE id = ? AND gen_state = 'generating'",
    )
    .bind(text)
    .bind(SqlJson(perf))
    .bind(id)
    .execute(&s.db)
    .await?;
    Ok(())
}

// --- 書き直す ---

#[derive(Deserialize, Default)]
pub struct RegenerateInput {
    /// 新しい意図。省略なら今の意図のまま
    intent: Option<String>,
    /// 新しい長さ区分。省略なら今のまま
    length_class: Option<String>,
}

async fn save_version(s: &AppState, id: i64) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO line_versions (line_id, text, intent, length_class, performance)
         SELECT id, text, intent, length_class, performance FROM lines WHERE id = ? AND trim(text) != ''",
    )
    .bind(id)
    .execute(&s.db)
    .await?;
    Ok(())
}

async fn regenerate(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    body: Option<Json<RegenerateInput>>,
) -> AppResult<Json<Line>> {
    let i = body.map(|Json(b)| b).unwrap_or_default();
    let (kind, character_id, gen_state): (String, Option<i64>, Option<String>) =
        sqlx::query_as("SELECT kind, character_id, gen_state FROM lines WHERE id = ?")
            .bind(id)
            .fetch_one(&s.db)
            .await?;
    if kind != "dialogue" || character_id.is_none() {
        return Err(AppError::BadRequest(
            "話者のいるセリフだけ書き直せます".into(),
        ));
    }
    if gen_state.as_deref() == Some("generating") {
        return Err(AppError::BadRequest("いま書いているところです".into()));
    }
    if let Some(c) = &i.length_class
        && writer::length_range(c).is_none()
    {
        return Err(AppError::BadRequest(format!("invalid length_class: {c}")));
    }
    // 設定が足りなければ、書き始める前に伝える
    Writer::new(&crate::settings::llm(&s).await?, "check").map_err(bad_gateway)?;
    save_version(&s, id).await?;
    sqlx::query(
        "UPDATE lines SET intent = COALESCE(?, intent), length_class = COALESCE(?, length_class, 'M'),
            gen_state = 'generating', gen_error = NULL WHERE id = ?",
    )
    .bind(i.intent.map(|t| t.trim().to_owned()))
    .bind(i.length_class)
    .bind(id)
    .execute(&s.db)
    .await?;
    spawn_writing(s.clone(), vec![id]);
    Ok(Json(fetch_line(&s.db, id).await?))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Version {
    id: i64,
    line_id: i64,
    text: String,
    intent: String,
    length_class: Option<String>,
    performance: SqlJson<Value>,
    created_at: String,
}

async fn versions(State(s): State<AppState>, Path(id): Path<i64>) -> AppResult<Json<Vec<Version>>> {
    let rows = sqlx::query_as("SELECT * FROM line_versions WHERE line_id = ? ORDER BY id DESC")
        .bind(id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

/// 前の案に戻す。今の案は前の案として残す (入れ替え)。
async fn restore(State(s): State<AppState>, Path(version_id): Path<i64>) -> AppResult<Json<Line>> {
    let v: Version = sqlx::query_as("SELECT * FROM line_versions WHERE id = ?")
        .bind(version_id)
        .fetch_one(&s.db)
        .await?;
    let mut tx = s.db.begin().await?;
    sqlx::query(
        "INSERT INTO line_versions (line_id, text, intent, length_class, performance)
         SELECT id, text, intent, length_class, performance FROM lines WHERE id = ? AND trim(text) != ''",
    )
    .bind(v.line_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE lines SET text = ?, intent = ?, length_class = ?, performance = ?, gen_state = NULL, gen_error = NULL
          WHERE id = ?",
    )
    .bind(&v.text)
    .bind(&v.intent)
    .bind(&v.length_class)
    .bind(&v.performance)
    .bind(v.line_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM line_versions WHERE id = ?")
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(fetch_line(&s.db, v.line_id).await?))
}

// --- 演技を付ける ---

#[derive(Deserialize)]
pub struct DirectInput {
    line_ids: Vec<i64>,
}

async fn direct(
    State(s): State<AppState>,
    Path(scene_id): Path<i64>,
    Json(i): Json<DirectInput>,
) -> AppResult<Json<Vec<Line>>> {
    let script = script(&s, scene_id).await?;
    let targets: Vec<i64> = script
        .iter()
        .filter(|(id, l)| {
            i.line_ids.contains(id) && l.speaker.is_some() && !l.text.trim().is_empty()
        })
        .map(|(id, _)| *id)
        .collect();
    if targets.is_empty() {
        return Err(AppError::BadRequest(
            "演技を付けるセリフがありません".into(),
        ));
    }
    let (scene, project_id) = scene_info(&s, scene_id).await?;
    let cast = cast(&s, project_id).await?;
    let writer =
        Writer::new(&crate::settings::llm(&s).await?, &session(scene_id)).map_err(bad_gateway)?;
    let messages = writer::direct_messages(&scene, &cast, &script, &targets);
    let directed = writer
        .json(&messages, 0.4, None)
        .await
        .map(|v| writer::parse_direction(&v))
        .map_err(bad_gateway)?;
    let mut out = Vec::new();
    for id in targets {
        if let Some(perf) = directed.get(&id) {
            sqlx::query("UPDATE lines SET performance = ? WHERE id = ?")
                .bind(SqlJson(perf))
                .bind(id)
                .execute(&s.db)
                .await?;
            out.push(fetch_line(&s.db, id).await?);
        }
    }
    Ok(Json(out))
}

/// 起動時: 前回の生成の途中で止まった行を失敗にする (書く処理はもう動いていない)。
pub(crate) async fn fail_interrupted(db: &sqlx::SqlitePool) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE lines SET gen_state = 'failed', gen_error = 'サーバーが止まったため中断しました' WHERE gen_state = 'generating'",
    )
    .execute(db)
    .await?;
    Ok(())
}
