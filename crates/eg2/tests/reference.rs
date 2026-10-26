//! tools/eg2_reference/dump_reference.py が出力した PyTorch の中間テンソルと突き合わせる。
//! testdata と Hugging Face キャッシュの重みが無い環境ではスキップする。
//!
//!     cargo test -p eg2 --release --features metal -- --nocapture

use std::collections::HashMap;
use std::path::Path;

use candle_core::{DType, Device, Tensor};

const REFERENCE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testdata/reference.safetensors"
);

fn check(name: &str, actual: &Tensor, expected: &Tensor, tol: f32) {
    let a = actual
        .to_device(&Device::Cpu)
        .unwrap()
        .flatten_all()
        .unwrap();
    let e = expected
        .to_device(&Device::Cpu)
        .unwrap()
        .flatten_all()
        .unwrap();
    assert_eq!(a.dims(), e.dims(), "{name}: shape mismatch");
    let diff = (&a - &e)
        .unwrap()
        .abs()
        .unwrap()
        .max(0)
        .unwrap()
        .to_scalar::<f32>()
        .unwrap();
    let scale = e
        .abs()
        .unwrap()
        .max(0)
        .unwrap()
        .to_scalar::<f32>()
        .unwrap()
        .max(1e-6);
    let rel = diff / scale;
    eprintln!("{name:20} max_abs_diff={diff:.3e} rel={rel:.3e} (scale {scale:.3e})");
    assert!(
        rel <= tol && rel.is_finite(),
        "{name}: rel diff {rel:.3e} > {tol:.0e}"
    );
}

#[test]
fn matches_pytorch_reference() {
    if !Path::new(REFERENCE).exists() {
        eprintln!("skip: {REFERENCE} not found");
        return;
    }
    let Ok(paths) = eg2::Paths::from_hf_cache(eg2::DEFAULT_MODEL) else {
        eprintln!("skip: weights are not in the Hugging Face cache");
        return;
    };
    let r: HashMap<String, Tensor> =
        candle_core::safetensors::load(REFERENCE, &Device::Cpu).unwrap();
    let bytes = std::fs::read(REFERENCE).unwrap();
    let (_, meta) = safetensors::SafeTensors::read_metadata(&bytes).unwrap();
    let texts: Vec<String> =
        serde_json::from_str(&meta.metadata().as_ref().unwrap()["texts"]).unwrap();

    let device = eg2::select_device().unwrap();
    eprintln!("device: {device:?}");
    let m = eg2::EmbeddingGemma2::load(&paths, &device).unwrap();

    // トークナイズ (<bos> ... <eos>)
    let ids = r["input_ids"].to_vec2::<i64>().unwrap();
    let mask = r["attention_mask"].to_vec2::<i64>().unwrap();
    for (i, text) in texts.iter().enumerate() {
        let n = mask[i].iter().filter(|&&x| x == 1).count();
        let expected: Vec<u32> = ids[i][..n].iter().map(|&x| x as u32).collect();
        assert_eq!(
            m.tokenize(text, 2048).unwrap(),
            expected,
            "tokens of text {i}"
        );
    }

    let ids_t = r["input_ids"]
        .to_dtype(DType::U32)
        .unwrap()
        .to_device(&device)
        .unwrap();
    let mask_t = r["attention_mask"]
        .to_dtype(DType::U32)
        .unwrap()
        .to_device(&device)
        .unwrap();
    let model = m.model();
    let embeds = model.embed_tokens(&ids_t).unwrap();
    check("embeds", &embeds, &r["embeds"], 1e-6);
    check(
        "per_layer_inputs",
        &model.per_layer_inputs(&embeds).unwrap(),
        &r["per_layer_inputs"],
        1e-5,
    );
    for i in [0usize, 5, 23] {
        let out = model.forward(&ids_t, &mask_t, Some(i)).unwrap();
        check(&format!("layer{i}"), &out, &r[&format!("layer{i}")], 1e-4);
    }
    check(
        "last_hidden_state",
        &model.forward(&ids_t, &mask_t, None).unwrap(),
        &r["last_hidden_state"],
        1e-4,
    );
    check(
        "embeddings",
        &model.embed(&ids_t, &mask_t).unwrap(),
        &r["embeddings"],
        1e-4,
    );

    // 文字列からの経路 (パディングなしで 1 件ずつでも同じになる)
    let single = m.embed(&[texts[1].as_str()]).unwrap();
    let expected = r["embeddings"].get(1).unwrap().to_vec1::<f32>().unwrap();
    let cos = eg2::cosine(&single[0], &expected);
    eprintln!("single text cosine {cos:.6}");
    assert!(cos > 0.99999);
}
