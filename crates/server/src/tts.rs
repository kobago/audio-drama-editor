//! Irodori-TTS の推論ワーカー。
//!
//! モデルは専用スレッドで読み込み、合成リクエストを 1 件ずつ順に処理する (GPU を占有するため)。
//! 話者の参照音声から作る latent はパスごとにキャッシュする。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use irodori::{Irodori, Paths, Request, SamplerParams, Speaker};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EngineStatus {
    Disabled,
    Loading,
    Ready { device: String },
    Failed { error: String },
}

/// 1 セリフの合成に必要な入力。
#[derive(Debug, Clone)]
pub struct SynthesisInput {
    pub text: String,
    pub caption: String,
    /// 話者の参照音声 (data/files 配下の絶対パス)。None なら参照なし。
    pub reference: Option<PathBuf>,
    pub seed: u64,
    pub params: SynthesisParams,
}

/// クライアントから調整できる生成パラメータ。テイクの params 列にもこの形で保存する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisParams {
    #[serde(default = "default_steps")]
    pub num_steps: usize,
    #[serde(default = "default_cfg_text")]
    pub cfg_scale_text: f32,
    #[serde(default = "default_cfg_text")]
    pub cfg_scale_caption: f32,
    #[serde(default = "default_cfg_speaker")]
    pub cfg_scale_speaker: f32,
    /// 尺予測を使わず秒数を固定する
    pub seconds: Option<f32>,
    #[serde(default = "one")]
    pub duration_scale: f32,
}

fn default_steps() -> usize {
    40
}
fn default_cfg_text() -> f32 {
    3.0
}
fn default_cfg_speaker() -> f32 {
    5.0
}
fn one() -> f32 {
    1.0
}

impl Default for SynthesisParams {
    fn default() -> Self {
        serde_json::from_value(Value::Object(Default::default())).expect("defaults")
    }
}

pub struct Synthesized {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

type Reply = oneshot::Sender<anyhow::Result<Synthesized>>;
type OnStart = Box<dyn FnOnce() + Send>;

pub struct TtsEngine {
    tx: mpsc::UnboundedSender<(SynthesisInput, OnStart, Reply)>,
    status: Arc<RwLock<EngineStatus>>,
}

impl TtsEngine {
    /// ワーカースレッドを起動し、バックグラウンドでモデルを読み込む。
    pub fn spawn(checkpoint: String, codec: String) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<(SynthesisInput, OnStart, Reply)>();
        let status = Arc::new(RwLock::new(EngineStatus::Loading));
        let worker_status = status.clone();
        std::thread::Builder::new()
            .name("irodori".into())
            .spawn(move || {
                let loaded = (|| -> anyhow::Result<(Irodori, String)> {
                    let device = irodori::select_device()?;
                    let paths = Paths::from_hf_cache(&checkpoint, &codec)?;
                    Ok((Irodori::load(&paths, &device)?, format!("{device:?}")))
                })();
                let rt = match loaded {
                    Ok((rt, device)) => {
                        tracing::info!("irodori loaded on {device}");
                        *worker_status.write().unwrap() = EngineStatus::Ready { device };
                        rt
                    }
                    Err(e) => {
                        tracing::error!("failed to load irodori: {e:?}");
                        *worker_status.write().unwrap() = EngineStatus::Failed {
                            error: format!("{e:#}"),
                        };
                        while let Some((_, _, reply)) = rx.blocking_recv() {
                            let _ = reply.send(Err(anyhow::anyhow!("TTS engine failed to load")));
                        }
                        return;
                    }
                };
                let mut speakers: HashMap<PathBuf, candle_core::Tensor> = HashMap::new();
                while let Some((input, on_start, reply)) = rx.blocking_recv() {
                    on_start();
                    let _ = reply.send(synthesize(&rt, &mut speakers, input));
                }
            })
            .expect("spawn irodori worker");
        Self { tx, status }
    }

    pub fn disabled() -> Self {
        let (tx, _) = mpsc::unbounded_channel();
        Self {
            tx,
            status: Arc::new(RwLock::new(EngineStatus::Disabled)),
        }
    }

    pub fn status(&self) -> EngineStatus {
        self.status.read().unwrap().clone()
    }

    /// キューに積み、合成が終わるまで待つ。`on_start` はワーカーが処理を始めるときに呼ばれる。
    pub async fn synthesize(
        &self,
        input: SynthesisInput,
        on_start: impl FnOnce() + Send + 'static,
    ) -> anyhow::Result<Synthesized> {
        if let EngineStatus::Disabled = self.status() {
            anyhow::bail!("TTS engine is disabled");
        }
        let (reply, rx) = oneshot::channel();
        self.tx
            .send((input, Box::new(on_start), reply))
            .map_err(|_| anyhow::anyhow!("TTS worker stopped"))?;
        rx.await?
    }
}

fn synthesize(
    rt: &Irodori,
    speakers: &mut HashMap<PathBuf, candle_core::Tensor>,
    input: SynthesisInput,
) -> anyhow::Result<Synthesized> {
    let latent = match &input.reference {
        Some(path) => Some(match speakers.get(path) {
            Some(l) => l.clone(),
            None => {
                let (wav, sr) = irodori::audio::read_wav(path)?;
                let l = rt.encode_reference(&wav, sr, 30.0)?;
                speakers.insert(path.clone(), l.clone());
                l
            }
        }),
        None => None,
    };
    let p = &input.params;
    let req = Request {
        text: input.text,
        caption: input.caption,
        seed: input.seed,
        sampler: SamplerParams {
            num_steps: p.num_steps,
            cfg_scale_text: p.cfg_scale_text,
            cfg_scale_caption: p.cfg_scale_caption,
            cfg_scale_speaker: p.cfg_scale_speaker,
            ..Default::default()
        },
        seconds: p.seconds,
        duration_scale: p.duration_scale,
        ..Default::default()
    };
    let speaker = latent.as_ref().map_or(Speaker::None, Speaker::Latent);
    let samples = rt.synthesize(&req, speaker)?;
    Ok(Synthesized {
        samples,
        sample_rate: irodori::SAMPLE_RATE,
    })
}

/// キャラクターの基本の声 (voice_caption) と、セリフの演技メタデータから VoiceDesign の caption を組み立てる。
///
/// performance: { emotion, intensity (0-1), tempo, volume, distance, voice_quality, note }
pub fn compose_caption(voice_caption: &str, performance: &Value) -> String {
    let get = |k: &str| {
        performance
            .get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(emotion) = get("emotion") {
        let degree = match performance.get("intensity").and_then(Value::as_f64) {
            Some(i) if i < 0.34 => "かすかな",
            Some(i) if i >= 0.67 => "強い",
            _ => "",
        };
        parts.push(format!("{degree}{emotion}を込めて"));
    }
    if let Some(q) = get("voice_quality") {
        parts.push(format!("{q}声で"));
    }
    if let Some(t) = get("tempo") {
        parts.push(format!("{t}テンポで"));
    }
    if let Some(v) = get("volume") {
        parts.push(format!("{v}声量で"));
    }
    match get("distance") {
        Some("近い") => parts.push("近い距離感で".into()),
        Some("遠い") => parts.push("遠くに向かって".into()),
        _ => {}
    }

    let mut caption = voice_caption.trim().trim_end_matches('。').to_owned();
    if !parts.is_empty() {
        if !caption.is_empty() {
            caption.push('。');
        }
        caption.push_str(&parts.join("、"));
        caption.push_str("話す");
    }
    if let Some(note) = get("note") {
        if !caption.is_empty() {
            caption.push('。');
        }
        caption.push_str(note.trim_end_matches('。'));
    }
    if !caption.is_empty() {
        caption.push('。');
    }
    caption
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn caption_from_voice_and_performance() {
        assert_eq!(compose_caption("", &json!({})), "");
        assert_eq!(
            compose_caption("落ち着いた女性の声。", &json!({})),
            "落ち着いた女性の声。"
        );
        assert_eq!(
            compose_caption(
                "落ち着いた女性の声",
                &json!({"emotion": "動揺", "intensity": 0.8, "voice_quality": "震えた", "distance": "近い", "note": "語尾は消え入るように"})
            ),
            "落ち着いた女性の声。強い動揺を込めて、震えた声で、近い距離感で話す。語尾は消え入るように。"
        );
        assert_eq!(
            compose_caption("", &json!({"emotion": "不満", "intensity": 0.2})),
            "かすかな不満を込めて話す。"
        );
    }
}
