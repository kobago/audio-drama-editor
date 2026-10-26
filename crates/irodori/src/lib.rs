//! Irodori-TTS (v3 / v3-VoiceDesign) の candle による推論実装。
//!
//! 参照実装: <https://github.com/Aratako/Irodori-TTS> (`irodori_tts/inference_runtime.py`)。
//! SilentCipher の透かしは実装しない。CFG は "independent" モードのみ。

pub mod audio;
pub mod codec;
pub mod config;
pub mod hub;
pub mod layers;
pub mod model;
pub mod ops;
pub mod sampler;
pub mod text;

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use rand::SeedableRng;
use rand_distr::{Distribution, StandardNormal};

pub use crate::codec::{DacVae, SAMPLE_RATE};
pub use crate::config::ModelConfig;
pub use crate::model::TextToLatent;
pub use crate::sampler::SamplerParams;
use crate::text::{TextTokenizer, normalize_text};

pub const DEFAULT_CHECKPOINT: &str = "Aratako/Irodori-TTS-600M-v3-VoiceDesign";
pub const DEFAULT_CODEC: &str = "Aratako/Semantic-DACVAE-Japanese-32dim";

/// 有効な feature に応じて推論デバイスを選ぶ。cuda → metal → cpu の順。
pub fn select_device() -> anyhow::Result<Device> {
    #[cfg(feature = "cuda")]
    {
        return Ok(Device::new_cuda(0)?);
    }
    #[cfg(all(feature = "metal", not(feature = "cuda")))]
    {
        return Ok(Device::new_metal(0)?);
    }
    #[allow(unreachable_code)]
    Ok(Device::Cpu)
}

pub struct Paths {
    pub checkpoint: PathBuf,
    pub codec: PathBuf,
    pub text_tokenizer: PathBuf,
    pub caption_tokenizer: PathBuf,
}

impl Paths {
    /// Hugging Face のローカルキャッシュから探す。
    pub fn from_hf_cache(checkpoint_repo: &str, codec_repo: &str) -> anyhow::Result<Self> {
        let checkpoint = hub::cached_file(checkpoint_repo, "model.safetensors")?;
        let cfg = read_config(&checkpoint)?;
        Ok(Self {
            codec: hub::cached_file(codec_repo, "weights.pth")?,
            text_tokenizer: hub::cached_file(&cfg.text_tokenizer_repo, "tokenizer.json")?,
            caption_tokenizer: hub::cached_file(cfg.caption_tokenizer_repo(), "tokenizer.json")?,
            checkpoint,
        })
    }
}

pub fn read_config(checkpoint: &Path) -> anyhow::Result<ModelConfig> {
    let file = std::fs::File::open(checkpoint).with_context(|| format!("open {}", checkpoint.display()))?;
    // SAFETY: 読み取り専用で mmap し、この関数内でのみ参照する。
    let mmap = unsafe { memmap2::Mmap::map(&file)? };
    let (_, meta) = safetensors::SafeTensors::read_metadata(&mmap)?;
    let json = meta.metadata().as_ref().and_then(|m| m.get("config_json")).context("checkpoint has no config_json metadata")?;
    Ok(serde_json::from_str(json)?)
}

/// 話者の参照。登録時に一度だけ encode した latent を使い回す。
pub enum Speaker<'a> {
    None,
    Latent(&'a Tensor),
}

#[derive(Debug, Clone)]
pub struct Request {
    pub text: String,
    /// VoiceDesign の演技・声質指示。空なら無条件。
    pub caption: String,
    pub seed: u64,
    pub sampler: SamplerParams,
    /// 指定時は尺予測を使わずこの秒数にする。
    pub seconds: Option<f32>,
    pub duration_scale: f32,
    pub min_seconds: f32,
    pub max_seconds: f32,
    pub trim_tail: bool,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            text: String::new(),
            caption: String::new(),
            seed: 0,
            sampler: SamplerParams::default(),
            seconds: None,
            duration_scale: 1.0,
            min_seconds: 0.5,
            max_seconds: 30.0,
            trim_tail: true,
        }
    }
}

pub struct Irodori {
    pub model: TextToLatent,
    pub codec: DacVae,
    text_tokenizer: TextTokenizer,
    caption_tokenizer: TextTokenizer,
}

impl Irodori {
    pub fn load(paths: &Paths, device: &Device) -> anyhow::Result<Self> {
        let cfg = read_config(&paths.checkpoint)?;
        if cfg.latent_patch_size != 1 {
            bail!("latent_patch_size != 1 is not supported");
        }
        // SAFETY: チェックポイントは読み込み中に書き換えられない前提。
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[&paths.checkpoint], DType::F32, device)? };
        let text_tokenizer = TextTokenizer::from_file(&paths.text_tokenizer, cfg.text_add_bos)?;
        let caption_tokenizer = TextTokenizer::from_file(&paths.caption_tokenizer, cfg.caption_add_bos())?;
        let model = TextToLatent::new(cfg, vb)?;
        let codec = DacVae::load(&paths.codec, device)?;
        if codec.latent_dim() != model.cfg.latent_dim {
            bail!("latent dim mismatch: codec={} model={}", codec.latent_dim(), model.cfg.latent_dim);
        }
        Ok(Self { model, codec, text_tokenizer, caption_tokenizer })
    }

    fn frames_per_second(&self) -> f32 {
        SAMPLE_RATE as f32 / self.codec.hop_length() as f32
    }

    /// 参照音声 (任意のサンプルレート、モノラル) を話者 latent (1, T, D) にする。
    /// -16 dB にラウドネス正規化し、`max_seconds` で切り詰める。
    pub fn encode_reference(&self, wav: &[f32], sample_rate: u32, max_seconds: f32) -> anyhow::Result<Tensor> {
        let mut wav = audio::resample(wav, sample_rate, SAMPLE_RATE)?;
        wav.truncate((max_seconds * SAMPLE_RATE as f32) as usize);
        let wav = audio::normalize_loudness(&wav, SAMPLE_RATE, -16.0)?;
        Ok(self.codec.encode(&wav)?)
    }

    /// 1 セリフを合成し、48 kHz モノラル波形を返す。
    pub fn synthesize(&self, req: &Request, speaker: Speaker) -> anyhow::Result<Vec<f32>> {
        let cfg = &self.model.cfg;
        let text = normalize_text(&req.text).trim().to_owned();
        if text.is_empty() {
            bail!("text became empty after normalization");
        }
        let text_ids = self.text_tokenizer.encode(&text, cfg.max_text_len())?;
        let caption = req.caption.trim();
        let has_caption = cfg.use_caption_condition && !caption.is_empty();
        let caption_ids = self.caption_tokenizer.encode(caption, cfg.max_caption_len())?;
        let ref_latent = match speaker {
            Speaker::None => None,
            Speaker::Latent(l) => Some(l.to_device(self.model.device())?),
        };
        let cond = self.model.encode_conditions(&text_ids, ref_latent.as_ref(), &caption_ids, has_caption)?;

        let fps = self.frames_per_second();
        let latent_steps = match req.seconds {
            Some(s) => (s.clamp(req.min_seconds, req.max_seconds) * fps).ceil() as usize,
            None => {
                let log_frames = self.model.predict_duration_log_frames(&cond)?.context("checkpoint has no duration predictor")?;
                let frames = (log_frames.exp_m1() * req.duration_scale).round() as usize;
                let min = (req.min_seconds * fps).ceil().max(1.0) as usize;
                let max = (req.max_seconds * fps).floor().max(1.0) as usize;
                frames.clamp(min, max)
            }
        };

        let noise = gaussian_noise(req.seed, latent_steps, cfg.patched_latent_dim(), self.model.device())?;
        let latent = sampler::sample(&self.model, &cond, &noise, &req.sampler)?;
        let audio = self.codec.decode(&latent)?.squeeze(0)?.to_vec1::<f32>()?;

        let hop = self.codec.hop_length();
        let mut samples = latent_steps * hop;
        if req.trim_tail {
            let flat = audio::find_flattening_point(&latent.squeeze(0)?, 20, 0.05, 0.1)?;
            if flat > 0 {
                samples = samples.min(flat * hop);
            }
        }
        Ok(audio[..samples.min(audio.len())].to_vec())
    }
}

pub fn gaussian_noise(seed: u64, steps: usize, dim: usize, device: &Device) -> candle_core::Result<Tensor> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let v: Vec<f32> = (0..steps * dim).map(|_| StandardNormal.sample(&mut rng)).collect();
    Tensor::from_vec(v, (1, steps, dim), device)
}
