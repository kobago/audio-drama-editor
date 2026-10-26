//! Hugging Face Hub のローカルキャッシュからファイルを探す。

use std::path::PathBuf;

use anyhow::{Context, bail};

fn hub_cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("HF_HUB_CACHE") {
        return dir.into();
    }
    let home = std::env::var("HF_HOME").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache").join("huggingface")
    });
    home.join("hub")
}

/// `org/name` リポジトリの `file` をキャッシュから探す。`refs/main` の snapshot を優先する。
pub fn cached_file(repo: &str, file: &str) -> anyhow::Result<PathBuf> {
    let repo_dir = hub_cache_dir().join(format!("models--{}", repo.replace('/', "--")));
    let snapshots = repo_dir.join("snapshots");
    if let Ok(rev) = std::fs::read_to_string(repo_dir.join("refs").join("main")) {
        let path = snapshots.join(rev.trim()).join(file);
        if path.exists() {
            return Ok(path);
        }
    }
    let entries = std::fs::read_dir(&snapshots)
        .with_context(|| format!("{repo} is not in the Hugging Face cache ({})", snapshots.display()))?;
    for entry in entries.flatten() {
        let path = entry.path().join(file);
        if path.exists() {
            return Ok(path);
        }
    }
    bail!("{file} of {repo} is not in the Hugging Face cache; run `hf download {repo}` first")
}
