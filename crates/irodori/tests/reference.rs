//! tools/irodori_reference/dump_reference.py が出力した PyTorch の中間テンソルと突き合わせる。
//! testdata と Hugging Face キャッシュの重みが無い環境ではスキップする。
//!
//!     cargo test -p irodori --release --features metal -- --nocapture

use std::collections::HashMap;
use std::path::Path;

use candle_core::{Device, Tensor};
use irodori::model::Conditions;
use irodori::{Irodori, Paths, SamplerParams, audio, sampler, text};

const REFERENCE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testdata/reference.safetensors"
);

struct Reference {
    tensors: HashMap<String, Tensor>,
    meta: HashMap<String, String>,
}

impl Reference {
    fn load() -> Option<Self> {
        if !Path::new(REFERENCE).exists() {
            eprintln!("skip: {REFERENCE} not found");
            return None;
        }
        let tensors = candle_core::safetensors::load(REFERENCE, &Device::Cpu).unwrap();
        let bytes = std::fs::read(REFERENCE).unwrap();
        let (_, meta) = safetensors::SafeTensors::read_metadata(&bytes).unwrap();
        Some(Self {
            tensors,
            meta: meta.metadata().clone().unwrap_or_default(),
        })
    }

    fn get(&self, name: &str) -> Tensor {
        self.tensors[name].clone()
    }

    fn ids(&self, name: &str) -> Vec<u32> {
        self.get(name)
            .flatten_all()
            .unwrap()
            .to_vec1::<i64>()
            .unwrap()
            .into_iter()
            .map(|v| v as u32)
            .collect()
    }
}

/// max|a - b| / max|b| を返し、`tol` 以下であることを確認する。
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
    eprintln!("{name:24} max_abs_diff={diff:.3e} rel={rel:.3e} (scale {scale:.3e})");
    assert!(
        rel <= tol && rel.is_finite(),
        "{name}: rel diff {rel:.3e} > {tol:.0e}"
    );
}

fn conditions_from_reference(rt: &Irodori, r: &Reference) -> Conditions {
    let latent = r.get("ref_latent").to_device(rt.model.device()).unwrap();
    rt.model
        .encode_conditions(
            &r.ids("text_ids"),
            Some(&latent),
            &r.ids("caption_ids"),
            true,
        )
        .unwrap()
}

#[test]
fn matches_pytorch_reference() {
    let Some(r) = Reference::load() else { return };
    let Ok(paths) = Paths::from_hf_cache(irodori::DEFAULT_CHECKPOINT, irodori::DEFAULT_CODEC)
    else {
        eprintln!("skip: weights are not in the Hugging Face cache");
        return;
    };
    let device = irodori::select_device().unwrap();
    eprintln!("device: {device:?}");
    let rt = Irodori::load(&paths, &device).unwrap();

    // テキスト正規化・トークナイズ
    let normalized = text::normalize_text(&r.meta["text"]);
    assert_eq!(normalized, r.meta["normalized_text"]);
    let tok = text::TextTokenizer::from_file(&paths.text_tokenizer, true).unwrap();
    assert_eq!(tok.encode(&normalized, 256).unwrap(), r.ids("text_ids"));
    let cap_tok = text::TextTokenizer::from_file(&paths.caption_tokenizer, true).unwrap();
    assert_eq!(
        cap_tok.encode(&r.meta["caption"], 512).unwrap(),
        r.ids("caption_ids")
    );

    // ラウドネス正規化 (audiotools と ebur128 の実装差があるので緩め)
    let raw = r
        .get("ref_wav_raw")
        .flatten_all()
        .unwrap()
        .to_vec1::<f32>()
        .unwrap();
    let normalized_wav = audio::normalize_loudness(&raw, 48_000, -16.0).unwrap();
    check(
        "ref_wav_normalized",
        &Tensor::new(normalized_wav.as_slice(), &Device::Cpu).unwrap(),
        &r.get("ref_wav_normalized"),
        2e-2,
    );

    // DACVAE encode (同じ正規化済み波形から)
    let wav = r.get("ref_wav_normalized").to_vec1::<f32>().unwrap();
    check(
        "ref_latent",
        &rt.codec.encode(&wav).unwrap(),
        &r.get("ref_latent"),
        1e-3,
    );

    // 条件エンコーダ
    let cond = conditions_from_reference(&rt, &r);
    check("text_state", &cond.text_state, &r.get("text_state"), 1e-4);
    check(
        "speaker_state",
        &cond.speaker_state,
        &r.get("speaker_state"),
        1e-4,
    );
    check(
        "caption_state",
        &cond.caption_state,
        &r.get("caption_state"),
        1e-4,
    );

    let empty = rt
        .model
        .encode_conditions(&r.ids("text_ids"), None, &r.ids("caption_ids"), false)
        .unwrap();
    check(
        "speaker_state_noref",
        &empty.speaker_state,
        &r.get("speaker_state_noref"),
        1e-6,
    );
    check(
        "caption_state_empty",
        &empty.caption_state,
        &r.get("caption_state_empty"),
        1e-6,
    );

    // 尺予測
    let log_frames = rt
        .model
        .predict_duration_log_frames(&cond)
        .unwrap()
        .unwrap();
    check(
        "duration_log_frames",
        &Tensor::new(&[log_frames], &Device::Cpu).unwrap(),
        &r.get("duration_log_frames"),
        1e-4,
    );

    // DiT 1 回分
    let noise = r.get("noise").to_device(&device).unwrap();
    let (ctx, mask) = rt.model.build_context(&cond).unwrap();
    check(
        "v_first",
        &rt.model.forward(&noise, 0.999, &ctx, &mask).unwrap(),
        &r.get("v_first"),
        1e-3,
    );

    // サンプラー全体 (independent CFG)
    let params = SamplerParams {
        num_steps: r.meta["steps"].parse().unwrap(),
        ..Default::default()
    };
    let latent = sampler::sample(&rt.model, &cond, &noise, &params).unwrap();
    check("latent", &latent, &r.get("latent"), 2e-3);

    // DACVAE decode (PyTorch の latent から)
    let audio = rt.codec.decode(&r.get("latent")).unwrap();
    check("audio", &audio, &r.get("audio").squeeze(1).unwrap(), 2e-3);
}
