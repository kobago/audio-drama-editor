//! 判断の入力文と、答えの欄で読むトークン。`prompt.py` (System One) の移植。
//!
//! 状態は文字列だけを扱う (`json_only` の文字列の場合と同じ)。システムターンは付けない。

use std::collections::HashSet;

use anyhow::{Context, bail};
use tokenizers::Tokenizer;

const IM_START: &str = "<|im_start|>";
const IM_END: &str = "<|im_end|>";

/// 状態に対する質問。
#[derive(Debug, Clone)]
pub enum Question {
    /// はい / いいえ。`criteria` は (はいの意味, いいえの意味)。
    Noul {
        instructions: String,
        criteria: Option<(String, String)>,
    },
    /// 選択肢から 1 つ。`options` は (ラベル, 説明)。
    Choice {
        instructions: String,
        options: Vec<(String, String)>,
    },
    /// 0 から順の段階。`levels` は各段階の説明 (2〜10 段階)。
    Score {
        instructions: String,
        levels: Vec<String>,
    },
}

fn encode(tok: &Tokenizer, text: &str) -> anyhow::Result<Vec<u32>> {
    Ok(tok
        .encode(text, false)
        .map_err(|e| anyhow::anyhow!("tokenize: {e}"))?
        .get_ids()
        .to_vec())
}

/// ラベルが 1 文字の英字ならそのまま、26 個までは A..Z、それより多ければ 00.. を符号にする。
fn option_codes(labels: &[String]) -> Vec<String> {
    let labs: Vec<&str> = labels.iter().map(|l| l.trim()).collect();
    if !labs.is_empty()
        && labs
            .iter()
            .all(|l| l.chars().count() == 1 && l.chars().all(char::is_alphabetic))
    {
        return labs.iter().map(|l| l.to_string()).collect();
    }
    if labs.len() <= 26 {
        return (0..labs.len())
            .map(|i| char::from(b'A' + i as u8).to_string())
            .collect();
    }
    (0..labs.len()).map(|i| format!("{i:02}")).collect()
}

fn fallback_pool() -> Vec<String> {
    let upper = || (b'A'..=b'Z').map(char::from);
    let mut pool: Vec<String> = upper().map(String::from).collect();
    pool.extend((0..100).map(|i| format!("{i:02}")));
    pool.extend((b'a'..=b'z').map(|c| char::from(c).to_string()));
    pool.extend((0..200).map(|i| format!("#{i}")));
    pool.extend(upper().flat_map(|a| upper().map(move |b| format!("{a}{b}"))));
    pool
}

/// 選択肢ごとに、1 トークンで重ならない符号を割り当てる: [(符号, トークン ID)]。
fn aliases(tok: &Tokenizer, labels: &[String]) -> anyhow::Result<Vec<(String, u32)>> {
    let mut used = HashSet::new();
    let mut out = Vec::new();
    let mut take = |raw: &str, out: &mut Vec<(String, u32)>| -> anyhow::Result<bool> {
        let enc = encode(tok, raw)?;
        if enc.len() != 1 || used.contains(&enc[0]) {
            return Ok(false);
        }
        used.insert(enc[0]);
        out.push((raw.to_string(), enc[0]));
        Ok(true)
    };
    let mut pool = None;
    for code in option_codes(labels) {
        if take(&code, &mut out)? {
            continue;
        }
        let mut found = false;
        for raw in pool.get_or_insert_with(fallback_pool).iter() {
            if take(raw, &mut out)? {
                found = true;
                break;
            }
        }
        if !found {
            bail!("no single-token alias left for {} options", labels.len());
        }
    }
    Ok(out)
}

/// 1 トークンになる文字列のトークン ID (重複を除く)。
fn single_ids(tok: &Tokenizer, texts: &[String]) -> anyhow::Result<Vec<u32>> {
    let mut out = Vec::new();
    for t in texts {
        let enc = encode(tok, t)?;
        if enc.len() == 1 && !out.contains(&enc[0]) {
            out.push(enc[0]);
        }
    }
    Ok(out)
}

/// 選択肢ごとに読むトークン ID の組。答えは各組の最大値の softmax。
pub fn readout_ids(tok: &Tokenizer, q: &Question) -> anyhow::Result<Vec<Vec<u32>>> {
    let strs = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match q {
        Question::Noul { .. } => {
            let yes = single_ids(tok, &strs(&["yes", "Yes", "YES"]))?;
            let no = single_ids(tok, &strs(&["no", "No", "NO"]))?;
            if yes.is_empty() || no.is_empty() {
                bail!("tokenizer has no single-token yes/no");
            }
            Ok(vec![yes, no])
        }
        Question::Score { levels, .. } => (0..levels.len())
            .map(|i| {
                let g = single_ids(tok, &[i.to_string()])?;
                if g.is_empty() {
                    bail!(
                        "score with {} levels needs single-token digits",
                        levels.len()
                    );
                }
                Ok(g)
            })
            .collect(),
        Question::Choice { options, .. } => {
            if options.is_empty() {
                bail!("choice with no options");
            }
            let labels: Vec<String> = options.iter().map(|(l, _)| l.clone()).collect();
            aliases(tok, &labels)?
                .into_iter()
                .map(|(code, tid)| {
                    let mut g = vec![tid];
                    g.extend(
                        single_ids(tok, &[format!(" {code}")])?
                            .into_iter()
                            .filter(|&i| i != tid),
                    );
                    Ok(g)
                })
                .collect()
        }
    }
}

fn question_block(tok: &Tokenizer, q: &Question) -> anyhow::Result<String> {
    Ok(match q {
        Question::Choice {
            instructions,
            options,
        } => {
            let labels: Vec<String> = options.iter().map(|(l, _)| l.clone()).collect();
            let codes = aliases(tok, &labels)?;
            let lines: Vec<String> = options
                .iter()
                .zip(&codes)
                .map(|((label, desc), (code, _))| {
                    let text = if desc.is_empty() {
                        label.replace('_', " ")
                    } else {
                        desc.clone()
                    };
                    format!("{code} {text}")
                })
                .collect();
            format!(
                "{instructions}\n\nOptions:\n{}\n\nReply with the option code only.",
                lines.join("\n")
            )
        }
        Question::Noul {
            instructions,
            criteria,
        } => {
            let extra = criteria
                .as_ref()
                .map(|(y, n)| format!("\nYes: {y}\nNo: {n}"))
                .unwrap_or_default();
            format!("{instructions}{extra}\n\nReply with yes or no only.")
        }
        Question::Score {
            instructions,
            levels,
        } => {
            let legend: Vec<String> = levels
                .iter()
                .enumerate()
                .map(|(i, l)| format!("{i} {l}"))
                .collect();
            format!(
                "{instructions}\n\n{}\n\nReply with a single digit 0-{} only.",
                legend.join("\n"),
                levels.len() - 1
            )
        }
    })
}

/// 質問の前、すべての質問で共通の部分 (BOS、ユーザーターンの頭、状態)。
pub fn prefix_text(bos: &str, state: Option<&str>) -> String {
    let body = state
        .map(|s| format!("{s}\n\n\nQUESTION:\n"))
        .unwrap_or_default();
    format!("{bos}{IM_START}user\n{body}")
}

/// 質問とアシスタントの見出し。答えの欄の直前まで。
pub fn suffix_text(tok: &Tokenizer, q: &Question) -> anyhow::Result<String> {
    let body = question_block(tok, q).context("question")?;
    Ok(format!("{body}{IM_END}\n{IM_START}assistant\n"))
}

pub(crate) fn tokens(tok: &Tokenizer, text: &str) -> anyhow::Result<Vec<u32>> {
    encode(tok, text)
}
