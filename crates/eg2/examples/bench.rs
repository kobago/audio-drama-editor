//! 記憶 60 件と検索文の埋め込みにかかる時間を測る。
//!
//!     cargo run -p eg2 --release --features metal --example bench

use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let device = eg2::select_device()?;
    let model = eg2::load_default(&device)?;
    let data = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/rag_eval/data/memories.jsonl"
    ))?;
    let docs: Vec<String> = data
        .lines()
        .map(|l| {
            let m: serde_json::Value = serde_json::from_str(l).unwrap();
            eg2::document_prompt(
                Some(m["kind"].as_str().unwrap()),
                m["text"].as_str().unwrap(),
            )
        })
        .collect();
    let docs: Vec<&str> = docs.iter().map(String::as_str).collect();
    let query = eg2::query_prompt("アリスが遅れてきたボブを軽く責める");

    model.embed(&docs[..4])?; // 慣らし
    for _ in 0..2 {
        let t = Instant::now();
        let d = model.embed(&docs)?;
        let docs_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        let q = model.embed(&[&query, &query, &query])?;
        let query_ms = t.elapsed().as_secs_f64() * 1000.0;
        let best = d
            .iter()
            .map(|v| eg2::cosine(&q[0], v))
            .fold(f32::MIN, f32::max);
        println!(
            "{device:?}: docs×{} {docs_ms:.0} ms, query×3 {query_ms:.0} ms (best cos {best:.3})",
            d.len()
        );
    }
    Ok(())
}
