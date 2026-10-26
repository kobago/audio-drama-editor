//! テキスト正規化 (`irodori_tts.text_normalization`) とトークナイズ (`PretrainedTextTokenizer`)。

use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, anyhow};
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

const SIMPLE_REPLACE: &[(&str, &str)] = &[
    ("\t", ""),
    ("[n]", ""),
    (r"\[n\]", ""),
    ("\u{3000}", ""),
    ("？", "?"),
    ("！", "!"),
    ("♥", "♡"),
    ("●", "○"),
    ("◯", "○"),
    ("〇", "○"),
];

static REGEX_REPLACE: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"[;▼♀♂《》≪≫①②③④⑤⑥]", ""),
        (r"[\x{02d7}\x{2010}-\x{2015}\x{2043}\x{2212}\x{23af}\x{23e4}\x{2500}\x{2501}\x{2e3a}\x{2e3b}]", ""),
        (r"[\x{ff5e}\x{301C}]", "ー"),
        (r"…{3,}", "……"),
    ]
    .into_iter()
    .map(|(p, r)| (Regex::new(p).unwrap(), r))
    .collect()
});

fn strip_outer_brackets(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    loop {
        if chars.len() < 2 {
            break;
        }
        let (start, end) = (chars[0], chars[chars.len() - 1]);
        let close = match start {
            '「' => '」',
            '『' => '』',
            '（' => '）',
            '【' => '】',
            '(' => ')',
            _ => break,
        };
        if close != end {
            break;
        }
        let mut depth = 0i32;
        let mut enclosing = true;
        for (i, &c) in chars.iter().enumerate() {
            if c == start {
                depth += 1;
            } else if c == end {
                depth -= 1;
            }
            if depth == 0 && i < chars.len() - 1 {
                enclosing = false;
                break;
            }
        }
        if enclosing && depth == 0 {
            chars = chars[1..chars.len() - 1].to_vec();
        } else {
            break;
        }
    }
    chars.into_iter().collect()
}

pub fn normalize_text(text: &str) -> String {
    let mut text = text.to_owned();
    for (old, new) in SIMPLE_REPLACE {
        text = text.replace(old, new);
    }
    for (re, rep) in REGEX_REPLACE.iter() {
        text = re.replace_all(&text, *rep).into_owned();
    }
    let text = strip_outer_brackets(&text);
    let text: String = text.nfkc().collect();
    text.replace("...", "…").replace("..", "…")
}

pub struct TextTokenizer {
    inner: tokenizers::Tokenizer,
    bos_id: Option<u32>,
}

impl TextTokenizer {
    pub fn from_file(path: &Path, add_bos: bool) -> anyhow::Result<Self> {
        let inner = tokenizers::Tokenizer::from_file(path).map_err(|e| anyhow!("load tokenizer: {e}"))?;
        let bos_id = if add_bos {
            Some(inner.token_to_id("<s>").context("tokenizer has no <s> token")?)
        } else {
            None
        };
        Ok(Self { inner, bos_id })
    }

    /// BOS を先頭に付けて `max_len` で切り詰める。
    pub fn encode(&self, text: &str, max_len: usize) -> anyhow::Result<Vec<u32>> {
        let enc = self.inner.encode(text, false).map_err(|e| anyhow!("tokenize: {e}"))?;
        let mut ids: Vec<u32> = self.bos_id.into_iter().collect();
        ids.extend_from_slice(enc.get_ids());
        ids.truncate(max_len);
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_matches_python() {
        assert_eq!(normalize_text("「こんにちは！今日は、いい天気ですね…」"), "こんにちは!今日は、いい天気ですね…");
        assert_eq!(normalize_text("「あ」「い」"), "「あ」「い」");
        assert_eq!(normalize_text("えっと…………うん〜"), "えっと……うんー");
        assert_eq!(normalize_text("ＡＢＣ　１２３..."), "ABC123…");
    }
}
