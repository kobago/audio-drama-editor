//! RAG の判断 1 場面ぶん (3 段階 × 20 件 + 選択 1 問) にかかる時間を測る。
//!
//!     cargo run -p d1 --release --features metal --example bench

use std::time::Instant;

use d1::Question;

fn main() -> anyhow::Result<()> {
    let device = d1::select_device()?;
    let t = Instant::now();
    let dtype = match std::env::var("D1_DTYPE").as_deref() {
        Ok("f16") => candle_core::DType::F16,
        Ok("f32") => candle_core::DType::F32,
        _ => d1::default_dtype(&device),
    };
    let d1 = d1::D1::load(
        &d1::Paths::from_hf_cache(d1::DEFAULT_MODEL)?,
        &device,
        dtype,
    )?;
    println!("{device:?}: load {:.1} s", t.elapsed().as_secs_f64());
    let data = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/rag_eval/data/memories.jsonl"
    ))?;
    let mems: Vec<String> = data
        .lines()
        .take(20)
        .map(|l| {
            let m: serde_json::Value = serde_json::from_str(l).unwrap();
            m["text"].as_str().unwrap().to_string()
        })
        .collect();
    let state = "登場人物: アリス（高校2年生）\n場面: 放課後の屋上。アリスが先に来て待っていた。\n\
                 直前のやりとり:\nボブ「ごめん、待たせた。」\nアリスが次に話すこと: 遅れてきたことを軽く責める";
    let score: Vec<Question> = mems
        .iter()
        .map(|m| Question::Score {
            instructions: format!(
                "次の記憶は、この場面でのアリスの次のセリフにどのくらい関係するか: 「{m}」"
            ),
            levels: vec![
                "関係ない: この場面では思い出さない".into(),
                "少し関係する: 背景として頭にある".into(),
                "強く関係する: セリフの中身や言い方に直接表れる".into(),
            ],
        })
        .collect();
    let mut options: Vec<(String, String)> = mems
        .iter()
        .enumerate()
        .map(|(i, m)| (format!("m{i:02}"), m.clone()))
        .collect();
    options.push(("none".into(), "どれも関係しない".into()));
    let choice = vec![Question::Choice {
        instructions: "アリスがこの場面で次のセリフを話すとき、いちばん強く思い出したり踏まえたりする記憶はどれか".into(),
        options,
    }];

    d1.probabilities(state, &choice)?; // 慣らし
    for _ in 0..3 {
        let t = Instant::now();
        let s = d1.probabilities(state, &score)?;
        let score_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        let c = d1.probabilities(state, &choice)?;
        let choice_ms = t.elapsed().as_secs_f64() * 1000.0;
        println!(
            "score×{} {score_ms:.0} ms (first {:.3}), choice {choice_ms:.0} ms (none {:.3})",
            s.len(),
            d1::expected_level(&s[0]),
            c[0].last().unwrap()
        );
    }
    Ok(())
}
