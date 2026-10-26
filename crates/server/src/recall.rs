//! セリフを書くときに思い出す記憶を選ぶ、2 段階の検索。
//!
//! 1. EmbeddingGemma 2 で、検索文 3 通り (場面すべて / 直前のセリフと意図 / 意図だけ) の順位を
//!    RRF で統合し、上位 [`CANDIDATES`] 件を候補にする。
//! 2. d1-3B で候補ごとに「次のセリフにどのくらい関係するか」を 3 段階で問い、さらに候補を並べて
//!    1 つ選ばせる。2 つを足した点数で並べ、上位 [`USE`] 件を使う。
//!
//! 方式と数値の根拠は tools/rag_eval/REPORT.md。

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::rag::RagEngine;

/// 判断モデルに渡す候補の数
pub const CANDIDATES: usize = 20;
/// セリフ生成に使う記憶の数 (必ず使う記憶は別に足す)
pub const USE: usize = 3;
/// RRF の定数
const RRF_K: f32 = 60.0;

pub fn kind_label(kind: &str) -> &str {
    match kind {
        "experience" => "経験",
        "recollection" => "思い出",
        "knowledge" => "知識",
        "relation" => "関係",
        "habit" => "好み・習慣",
        other => other,
    }
}

/// 検索と判断に使う記憶の中身。
#[derive(Debug, Clone)]
pub struct MemoryDoc {
    pub id: i64,
    pub kind: String,
    pub period: String,
    pub people: Vec<String>,
    pub text: String,
}

/// 記憶 1 件を、時期と関わる人物付きの 1 文にする。
pub fn memory_text(m: &MemoryDoc) -> String {
    let people = m.people.join("、");
    let head: Vec<&str> = [m.period.trim(), people.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if head.is_empty() {
        m.text.clone()
    } else {
        format!("（{}）{}", head.join("・"), m.text)
    }
}

/// 埋め込む文書。
pub fn document_text(speaker: &str, m: &MemoryDoc) -> String {
    let title = format!("{speaker}の{}", kind_label(&m.kind));
    eg2::document_prompt(Some(&title), &memory_text(m))
}

/// 記憶を引く場面。
#[derive(Debug, Clone)]
pub struct Context {
    pub speaker: String,
    /// 判断モデルに見せる人物の説明 (短く)
    pub profile: String,
    pub setting: String,
    /// 直前のやりとり (古い順): (話者, セリフ)。ト書きは話者なし
    pub history: Vec<(Option<String>, String)>,
    /// これから話すこと
    pub intent: String,
}

fn history_line((who, text): &(Option<String>, String)) -> String {
    match who {
        Some(who) => format!("{who}「{text}」"),
        None => format!("（{text}）"),
    }
}

pub fn scene_text(c: &Context) -> String {
    let mut parts = vec![format!("場面: {}", c.setting)];
    if !c.history.is_empty() {
        let lines: Vec<String> = c.history.iter().map(history_line).collect();
        parts.push(format!("直前のやりとり:\n{}", lines.join("\n")));
    }
    parts.push(format!("{}が次に話すこと: {}", c.speaker, c.intent));
    parts.join("\n")
}

/// 検索文 3 通り: 場面すべて / 直前のセリフと意図 / 意図だけ。
pub fn queries(c: &Context) -> [String; 3] {
    let last = c.history.last().map(history_line).unwrap_or_default();
    [
        scene_text(c),
        format!("{last}{}が次に話すこと: {}", c.speaker, c.intent),
        format!("{}が{}", c.speaker, c.intent),
    ]
    .map(|q| eg2::query_prompt(&q))
}

pub fn judge_state(c: &Context) -> String {
    let who = if c.profile.is_empty() {
        c.speaker.clone()
    } else {
        format!("{}（{}）", c.speaker, c.profile)
    };
    format!("登場人物: {who}\n{}", scene_text(c))
}

pub fn score_question(speaker: &str, m: &MemoryDoc) -> d1::Question {
    d1::Question::Score {
        instructions: format!(
            "次の記憶は、この場面での{speaker}の次のセリフにどのくらい関係するか: 「{}」",
            memory_text(m)
        ),
        levels: vec![
            "関係ない: この場面では思い出さない".into(),
            "少し関係する: 背景として頭にある".into(),
            "強く関係する: セリフの中身や言い方に直接表れる".into(),
        ],
    }
}

/// 候補を並べて、いちばん関係するものを 1 つ選ばせる (最後の選択肢は「どれも関係しない」)。
pub fn choice_question(speaker: &str, ms: &[&MemoryDoc]) -> d1::Question {
    let mut options: Vec<(String, String)> = ms
        .iter()
        .map(|m| (m.id.to_string(), memory_text(m)))
        .collect();
    options.push(("none".into(), "どれも関係しない".into()));
    d1::Question::Choice {
        instructions: format!(
            "{speaker}がこの場面で次のセリフを話すとき、いちばん強く思い出したり踏まえたりする記憶はどれか"
        ),
        options,
    }
}

/// 順位 (添字の並び) を RRF で統合する。点数の高い順。
pub fn rrf(rankings: &[Vec<usize>]) -> Vec<(usize, f32)> {
    let mut score: HashMap<usize, f32> = HashMap::new();
    for ranking in rankings {
        for (rank, &i) in ranking.iter().enumerate() {
            *score.entry(i).or_default() += 1.0 / (RRF_K + rank as f32 + 1.0);
        }
    }
    let mut out: Vec<(usize, f32)> = score.into_iter().collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// 候補 1 件の点数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub memory_id: i64,
    /// 検索文とのコサイン類似度 (3 通りの最大)
    pub similarity: f32,
    /// 3 段階の判断の期待値 (0〜1)
    pub relevance: f32,
    /// 選択の確率
    pub choice: f32,
    /// 並べ替えに使う点数 (relevance + choice)
    pub score: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timing {
    pub embed_ms: u64,
    pub judge_ms: u64,
    /// 「どれも関係しない」の確率
    pub none: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    Pin,
    Exclude,
}

/// 使う記憶: 必ず使うものと、除外していない候補の上位 [`USE`] 件。
pub fn select(candidates: &[Candidate], marks: &HashMap<i64, Mark>) -> HashSet<i64> {
    let mut out: HashSet<i64> = marks
        .iter()
        .filter(|(_, m)| **m == Mark::Pin)
        .map(|(id, _)| *id)
        .collect();
    let mut ranked: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| !marks.contains_key(&c.memory_id))
        .collect();
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
    out.extend(ranked.iter().take(USE).map(|c| c.memory_id));
    out
}

/// 2 段階の検索。`memories` は埋め込み済みの記憶 (除外したものは渡さない)。候補は点数の高い順。
pub async fn run(
    engine: &RagEngine,
    ctx: &Context,
    memories: &[(MemoryDoc, Vec<f32>)],
) -> anyhow::Result<(Vec<Candidate>, Timing)> {
    if memories.is_empty() {
        return Ok((Vec::new(), Timing::default()));
    }
    let t = Instant::now();
    let qs = engine.embed(queries(ctx).to_vec()).await?;
    let sims: Vec<Vec<f32>> = qs
        .iter()
        .map(|q| memories.iter().map(|(_, v)| dot(q, v)).collect())
        .collect();
    let rankings: Vec<Vec<usize>> = sims
        .iter()
        .map(|s| {
            let mut idx: Vec<usize> = (0..s.len()).collect();
            idx.sort_by(|&a, &b| s[b].total_cmp(&s[a]));
            idx
        })
        .collect();
    let top: Vec<usize> = rrf(&rankings)
        .into_iter()
        .take(CANDIDATES)
        .map(|(i, _)| i)
        .collect();
    let embed_ms = t.elapsed().as_millis() as u64;

    let t = Instant::now();
    let docs: Vec<&MemoryDoc> = top.iter().map(|&i| &memories[i].0).collect();
    let mut questions: Vec<d1::Question> = docs
        .iter()
        .map(|m| score_question(&ctx.speaker, m))
        .collect();
    questions.push(choice_question(&ctx.speaker, &docs));
    let probs = engine.judge(judge_state(ctx), questions).await?;
    let choice = probs.last().cloned().unwrap_or_default();
    let mut candidates: Vec<Candidate> = top
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let relevance = d1::expected_level(&probs[k]) / 2.0;
            let picked = choice.get(k).copied().unwrap_or(0.0);
            Candidate {
                memory_id: memories[i].0.id,
                similarity: sims.iter().map(|s| s[i]).fold(f32::MIN, f32::max),
                relevance,
                choice: picked,
                score: relevance + picked,
            }
        })
        .collect();
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
    let timing = Timing {
        embed_ms,
        judge_ms: t.elapsed().as_millis() as u64,
        none: choice.last().copied().unwrap_or(0.0),
    };
    Ok((candidates, timing))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(id: i64, period: &str, people: &[&str], text: &str) -> MemoryDoc {
        MemoryDoc {
            id,
            kind: "recollection".into(),
            period: period.into(),
            people: people.iter().map(|s| s.to_string()).collect(),
            text: text.into(),
        }
    }

    #[test]
    fn memory_text_has_period_and_people() {
        assert_eq!(
            memory_text(&memory(1, "小学4年の夏", &["ボブ"], "花火を見た。")),
            "（小学4年の夏・ボブ）花火を見た。"
        );
        assert_eq!(
            memory_text(&memory(1, "", &["ボブ", "母"], "x")),
            "（ボブ、母）x"
        );
        assert_eq!(memory_text(&memory(1, " ", &[], "x")), "x");
        assert_eq!(
            document_text("アリス", &memory(1, "", &[], "x")),
            "title: アリスの思い出 | text: x"
        );
    }

    #[test]
    fn prompts_follow_the_evaluated_format() {
        let ctx = Context {
            speaker: "アリス".into(),
            profile: "高校2年生".into(),
            setting: "放課後の屋上。".into(),
            history: vec![(Some("ボブ".into()), "ごめん、待たせた。".into())],
            intent: "遅れてきたことを軽く責める".into(),
        };
        assert_eq!(
            judge_state(&ctx),
            "登場人物: アリス（高校2年生）\n場面: 放課後の屋上。\n直前のやりとり:\nボブ「ごめん、待たせた。」\nアリスが次に話すこと: 遅れてきたことを軽く責める"
        );
        let [full, focus, intent] = queries(&ctx);
        assert!(full.starts_with("task: search result | query: 場面: 放課後の屋上。"));
        assert_eq!(
            focus,
            "task: search result | query: ボブ「ごめん、待たせた。」アリスが次に話すこと: 遅れてきたことを軽く責める"
        );
        assert_eq!(
            intent,
            "task: search result | query: アリスが遅れてきたことを軽く責める"
        );
    }

    #[test]
    fn rrf_rewards_agreement() {
        let fused = rrf(&[vec![0, 1, 2], vec![1, 0, 2], vec![1, 2, 0]]);
        assert_eq!(fused[0].0, 1);
        assert_eq!(fused.len(), 3);
    }

    #[test]
    fn select_respects_marks() {
        let c = |id: i64, score: f32| Candidate {
            memory_id: id,
            similarity: 0.0,
            relevance: 0.0,
            choice: 0.0,
            score,
        };
        let cands = [c(1, 0.9), c(2, 0.8), c(3, 0.7), c(4, 0.6), c(5, 0.5)];
        let none = HashMap::new();
        assert_eq!(select(&cands, &none), HashSet::from([1, 2, 3]));
        let marks = HashMap::from([(2, Mark::Exclude), (9, Mark::Pin)]);
        assert_eq!(select(&cands, &marks), HashSet::from([9, 1, 3, 4]));
    }
}
