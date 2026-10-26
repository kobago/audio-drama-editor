//! PyTorch (d1-3B のリモートコード、CPU・fp32) の参照テンソルと比べる。
//!
//! 参照は `tools/d1_reference/dump_reference.py` で作る。無ければ飛ばす。
//!
//!     cargo test -p d1 --release -- --nocapture

use std::path::Path;

use candle_core::{DType, Device, Tensor};
use d1::{D1, Paths, Question};
use serde_json::Value;

fn tensor(st: &safetensors::SafeTensors, name: &str, device: &Device) -> Tensor {
    let view = st.tensor(name).unwrap_or_else(|_| panic!("{name} missing"));
    let shape: Vec<usize> = view.shape().to_vec();
    match view.dtype() {
        safetensors::Dtype::F32 => {
            let v: Vec<f32> = view
                .data()
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            Tensor::from_vec(v, shape, device).unwrap()
        }
        safetensors::Dtype::I64 => {
            let v: Vec<u32> = view
                .data()
                .chunks_exact(8)
                .map(|b| i64::from_le_bytes(b.try_into().unwrap()) as u32)
                .collect();
            Tensor::from_vec(v, shape, device).unwrap()
        }
        other => panic!("{name}: unexpected dtype {other:?}"),
    }
}

fn ids(st: &safetensors::SafeTensors, name: &str) -> Vec<u32> {
    tensor(st, name, &Device::Cpu).to_vec1().unwrap()
}

fn compare(name: &str, got: &Tensor, want: &Tensor, tol: f32) {
    let got = got
        .to_dtype(DType::F32)
        .unwrap()
        .to_device(&Device::Cpu)
        .unwrap();
    let diff = (got - want)
        .unwrap()
        .abs()
        .unwrap()
        .flatten_all()
        .unwrap()
        .max(0)
        .unwrap()
        .to_scalar::<f32>()
        .unwrap();
    let scale = want
        .abs()
        .unwrap()
        .flatten_all()
        .unwrap()
        .max(0)
        .unwrap()
        .to_scalar::<f32>()
        .unwrap();
    let rel = diff / scale.max(1e-6);
    println!("{name:20} max_abs_diff={diff:.3e} rel={rel:.3e} (scale {scale:.3e})");
    assert!(rel < tol, "{name}: rel {rel} >= {tol}");
}

fn question(q: &Value) -> Question {
    let instructions = q["instructions"].as_str().unwrap().to_string();
    match q["type"].as_str().unwrap() {
        "noul" => Question::Noul {
            instructions,
            criteria: None,
        },
        "score" => Question::Score {
            instructions,
            levels: q["criteria"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap().to_string())
                .collect(),
        },
        _ => Question::Choice {
            instructions,
            options: q["criteria"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                .collect(),
        },
    }
}

#[test]
fn matches_pytorch_reference() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/reference.safetensors");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skip: {} がありません", path.display());
        return;
    };
    let (_, header) = safetensors::SafeTensors::read_metadata(&bytes).unwrap();
    let meta: Value = serde_json::from_str(&header.metadata().as_ref().unwrap()["meta"]).unwrap();
    let st = safetensors::SafeTensors::deserialize(&bytes).unwrap();

    let device = d1::select_device().unwrap();
    println!("device: {device:?}");
    let d1 = D1::load(
        &Paths::from_hf_cache(d1::DEFAULT_MODEL).unwrap(),
        &device,
        DType::F32,
    )
    .unwrap();
    let tok = d1.tokenizer();

    let state = meta["state"].as_str().unwrap();
    let names: Vec<&str> = meta["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    let questions: Vec<Question> = names
        .iter()
        .map(|n| question(&meta["questions"][n]))
        .collect();

    // 入力文と読むトークン
    let prefix = d1::prefix_text("<|startoftext|>", Some(state));
    for (name, q) in names.iter().zip(&questions) {
        let text = prefix.clone() + &d1::suffix_text(tok, q).unwrap();
        let got = tok.encode(text, false).unwrap().get_ids().to_vec();
        assert_eq!(got, ids(&st, &format!("rows.{name}")), "{name}: tokens");
        let readout: Vec<u32> = d1::readout_ids(tok, q).unwrap().concat();
        assert_eq!(
            readout,
            ids(&st, &format!("readout.{name}")),
            "{name}: readout ids"
        );
    }

    // 1 問目の全文を素直な鎖として
    let ids0 = tensor(&st, "ids0", &device).unsqueeze(0).unwrap();
    let model = d1.model();
    for layer in [0, 2, 29] {
        let (h, _) = model.forward(&ids0, None, Some(layer)).unwrap();
        compare(
            &format!("layer{layer}"),
            &h.squeeze(0).unwrap(),
            &tensor(&st, &format!("layer{layer}"), &Device::Cpu),
            1e-4,
        );
    }
    let (h, _) = model.forward(&ids0, None, None).unwrap();
    let h = h.squeeze(0).unwrap();
    compare("final", &h, &tensor(&st, "final0", &Device::Cpu), 1e-4);
    let last = h.narrow(0, h.dim(0).unwrap() - 1, 1).unwrap();
    let logits = model.logits(&last).unwrap().squeeze(0).unwrap();
    compare(
        "logits",
        &logits,
        &tensor(&st, "logits0", &Device::Cpu),
        1e-4,
    );

    // 状態を共有した判断の確率
    let probs = d1.probabilities(state, &questions).unwrap();
    for (name, p) in names.iter().zip(probs) {
        let want: Vec<f32> = tensor(&st, &format!("probs.{name}"), &Device::Cpu)
            .to_vec1()
            .unwrap();
        let diff = p
            .iter()
            .zip(&want)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        println!("probs.{name:8} {p:.4?} want {want:.4?} (max diff {diff:.2e})");
        assert!(diff < 1e-3, "{name}: probabilities differ by {diff}");
    }
}
