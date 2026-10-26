//! EmbeddingGemma 2 (`google/embeddinggemma-2`) のテキスト埋め込みを candle で計算する。
//!
//! 参照実装: transformers の `EmbeddingGemma2TextModel` と sentence-transformers の
//! 平均プーリング + L2 正規化。画像・音声のエンコーダは移植しない。

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, anyhow};
use candle_core::{D, DType, Device, Module, Result, Tensor};
use candle_nn::{Embedding, Linear, VarBuilder};
use serde::Deserialize;

pub const DEFAULT_MODEL: &str = "google/embeddinggemma-2";

/// 検索文に付けるプレフィックス。
pub fn query_prompt(text: &str) -> String {
    format!("task: search result | query: {text}")
}

/// 文書に付けるプレフィックス。
pub fn document_prompt(title: Option<&str>, text: &str) -> String {
    format!("title: {} | text: {text}", title.unwrap_or("none"))
}

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

#[derive(Debug, Clone, Deserialize)]
struct LayerOverride {
    head_dim: Option<usize>,
    num_key_value_heads: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
struct RopeParams {
    rope_theta: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TextConfig {
    hidden_size: usize,
    intermediate_size: usize,
    num_hidden_layers: usize,
    num_attention_heads: usize,
    num_key_value_heads: usize,
    head_dim: usize,
    hidden_size_per_layer_input: usize,
    embedding_dim: usize,
    vocab_size: usize,
    rms_norm_eps: f64,
    sliding_window: usize,
    layer_types: Vec<String>,
    /// 層番号 ("05" など) ごとの上書き
    #[serde(default)]
    per_layer_config: HashMap<String, LayerOverride>,
    rope_parameters: HashMap<String, RopeParams>,
}

impl TextConfig {
    fn layer(&self, i: usize) -> (usize, usize) {
        let o = self.per_layer_config.get(&format!("{i:02}"));
        (
            o.and_then(|o| o.head_dim).unwrap_or(self.head_dim),
            o.and_then(|o| o.num_key_value_heads)
                .unwrap_or(self.num_key_value_heads),
        )
    }
}

#[derive(Deserialize)]
struct Config {
    text_config: TextConfig,
}

/// `EmbeddingGemma2RMSNorm`: x * (mean(x²) + eps)^-0.5 * weight。weight がなければ掛けない。
#[derive(Debug, Clone)]
struct RmsNorm {
    weight: Option<Tensor>,
    eps: f64,
}

impl RmsNorm {
    fn new(dim: usize, eps: f64, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            weight: Some(vb.get(dim, "weight")?),
            eps,
        })
    }

    fn plain(eps: f64) -> Self {
        Self { weight: None, eps }
    }
}

impl Module for RmsNorm {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let x = x.broadcast_mul(&(x.sqr()?.mean_keepdim(D::Minus1)? + self.eps)?.powf(-0.5)?)?;
        match &self.weight {
            Some(w) => x.broadcast_mul(w),
            None => Ok(x),
        }
    }
}

fn linear(in_dim: usize, out_dim: usize, vb: VarBuilder) -> Result<Linear> {
    Ok(Linear::new(vb.get((out_dim, in_dim), "weight")?, None))
}

struct Mlp {
    gate: Linear,
    up: Linear,
    down: Linear,
}

impl Mlp {
    fn new(cfg: &TextConfig, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            gate: linear(cfg.hidden_size, cfg.intermediate_size, vb.pp("gate_proj"))?,
            up: linear(cfg.hidden_size, cfg.intermediate_size, vb.pp("up_proj"))?,
            down: linear(cfg.intermediate_size, cfg.hidden_size, vb.pp("down_proj"))?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        // gelu_pytorch_tanh
        self.down
            .forward(&(self.gate.forward(x)?.gelu()? * self.up.forward(x)?)?)
    }
}

/// RoPE (rotate_half の形)。cos/sin は (S, head_dim / 2)。
struct Rope {
    cos: Tensor,
    sin: Tensor,
}

impl Rope {
    fn new(head_dim: usize, theta: f64, seq_len: usize, device: &Device) -> Result<Self> {
        let half = head_dim / 2;
        let inv: Vec<f32> = (0..half)
            .map(|i| 1.0 / (theta as f32).powf((2 * i) as f32 / head_dim as f32))
            .collect();
        let mut cos = Vec::with_capacity(seq_len * half);
        let mut sin = Vec::with_capacity(seq_len * half);
        for t in 0..seq_len {
            for f in &inv {
                let a = t as f32 * f;
                cos.push(a.cos());
                sin.push(a.sin());
            }
        }
        Ok(Self {
            cos: Tensor::from_vec(cos, (seq_len, half), device)?,
            sin: Tensor::from_vec(sin, (seq_len, half), device)?,
        })
    }

    fn apply(&self, x: &Tensor) -> Result<Tensor> {
        candle_nn::rotary_emb::rope(&x.contiguous()?, &self.cos, &self.sin)
    }
}

struct Attention {
    q: Linear,
    k: Linear,
    v: Linear,
    o: Linear,
    q_norm: RmsNorm,
    k_norm: RmsNorm,
    v_norm: RmsNorm,
    heads: usize,
    kv_heads: usize,
    head_dim: usize,
}

impl Attention {
    fn new(cfg: &TextConfig, layer: usize, vb: VarBuilder) -> Result<Self> {
        let (head_dim, kv_heads) = cfg.layer(layer);
        let h = cfg.hidden_size;
        let heads = cfg.num_attention_heads;
        Ok(Self {
            q: linear(h, heads * head_dim, vb.pp("q_proj"))?,
            k: linear(h, kv_heads * head_dim, vb.pp("k_proj"))?,
            v: linear(h, kv_heads * head_dim, vb.pp("v_proj"))?,
            o: linear(heads * head_dim, h, vb.pp("o_proj"))?,
            q_norm: RmsNorm::new(head_dim, cfg.rms_norm_eps, vb.pp("q_norm"))?,
            k_norm: RmsNorm::new(head_dim, cfg.rms_norm_eps, vb.pp("k_norm"))?,
            v_norm: RmsNorm::plain(cfg.rms_norm_eps),
            heads,
            kv_heads,
            head_dim,
        })
    }

    /// x: (B, S, H)、bias: (B, 1, S or 1, S) の加算マスク
    fn forward(&self, x: &Tensor, bias: &Tensor, rope: &Rope) -> Result<Tensor> {
        let (b, s, _) = x.dims3()?;
        let split = |t: Tensor, n: usize| t.reshape((b, s, n, self.head_dim))?.transpose(1, 2);
        let q = rope.apply(
            &self
                .q_norm
                .forward(&split(self.q.forward(x)?, self.heads)?)?,
        )?;
        let k = rope.apply(
            &self
                .k_norm
                .forward(&split(self.k.forward(x)?, self.kv_heads)?)?,
        )?;
        let v = self
            .v_norm
            .forward(&split(self.v.forward(x)?, self.kv_heads)?)?;
        let rep = self.heads / self.kv_heads;
        let (k, v) = if rep > 1 {
            let repeat = |t: Tensor| -> Result<Tensor> {
                let (b, n, s, d) = t.dims4()?;
                t.unsqueeze(2)?
                    .broadcast_as((b, n, rep, s, d))?
                    .reshape((b, n * rep, s, d))
            };
            (repeat(k)?, repeat(v)?)
        } else {
            (k, v)
        };
        // scaling は 1.0 (q_norm / k_norm が大きさをそろえている)
        let scores = q.matmul(&k.contiguous()?.t()?)?.broadcast_add(bias)?;
        let probs = candle_nn::ops::softmax_last_dim(&scores)?;
        let y = probs.matmul(&v.contiguous()?)?.transpose(1, 2)?.reshape((
            b,
            s,
            self.heads * self.head_dim,
        ))?;
        self.o.forward(&y)
    }
}

struct Layer {
    attn: Attention,
    mlp: Mlp,
    input_norm: RmsNorm,
    post_attn_norm: RmsNorm,
    pre_ff_norm: RmsNorm,
    post_ff_norm: RmsNorm,
    ple_gate: Linear,
    ple_proj: Linear,
    ple_norm: RmsNorm,
    scalar: Tensor,
    sliding: bool,
}

impl Layer {
    fn new(cfg: &TextConfig, i: usize, vb: VarBuilder) -> Result<Self> {
        let h = cfg.hidden_size;
        let p = cfg.hidden_size_per_layer_input;
        let eps = cfg.rms_norm_eps;
        Ok(Self {
            attn: Attention::new(cfg, i, vb.pp("self_attn"))?,
            mlp: Mlp::new(cfg, vb.pp("mlp"))?,
            input_norm: RmsNorm::new(h, eps, vb.pp("input_layernorm"))?,
            post_attn_norm: RmsNorm::new(h, eps, vb.pp("post_attention_layernorm"))?,
            pre_ff_norm: RmsNorm::new(h, eps, vb.pp("pre_feedforward_layernorm"))?,
            post_ff_norm: RmsNorm::new(h, eps, vb.pp("post_feedforward_layernorm"))?,
            ple_gate: linear(h, p, vb.pp("ple_block.per_layer_input_gate"))?,
            ple_proj: linear(p, h, vb.pp("ple_block.per_layer_projection"))?,
            ple_norm: RmsNorm::new(h, eps, vb.pp("ple_block.post_per_layer_input_norm"))?,
            scalar: vb.get(1, "layer_scalar")?,
            sliding: cfg.layer_types[i] == "sliding_attention",
        })
    }

    fn forward(
        &self,
        x: &Tensor,
        per_layer: &Tensor,
        bias: &Tensor,
        rope: &Rope,
    ) -> Result<Tensor> {
        let h = self.post_attn_norm.forward(&self.attn.forward(
            &self.input_norm.forward(x)?,
            bias,
            rope,
        )?)?;
        let x = (x + h)?;
        let h = self
            .post_ff_norm
            .forward(&self.mlp.forward(&self.pre_ff_norm.forward(&x)?)?)?;
        let x = (x + h)?;
        // 層ごとの入力 (PLE) で残差をゲートする
        let g = (self.ple_gate.forward(&x)?.gelu()? * per_layer)?;
        let x = (&x + self.ple_norm.forward(&self.ple_proj.forward(&g)?)?)?;
        x.broadcast_mul(&self.scalar)
    }
}

pub struct TextModel {
    cfg: TextConfig,
    embed: Embedding,
    ple_proj: Linear,
    ple_norm: RmsNorm,
    layers: Vec<Layer>,
    norm: RmsNorm,
    projection: Linear,
    device: Device,
}

impl TextModel {
    pub fn new(cfg: TextConfig, vb: VarBuilder) -> Result<Self> {
        let vb = vb.pp("language_model");
        let n = cfg.num_hidden_layers;
        let layers = (0..n)
            .map(|i| Layer::new(&cfg, i, vb.pp(format!("layers.{i}"))))
            .collect::<Result<_>>()?;
        Ok(Self {
            embed: Embedding::new(
                vb.get((cfg.vocab_size, cfg.hidden_size), "embed_tokens.weight")?,
                cfg.hidden_size,
            ),
            ple_proj: linear(
                cfg.hidden_size,
                n * cfg.hidden_size_per_layer_input,
                vb.pp("ple.per_layer_model_projection"),
            )?,
            ple_norm: RmsNorm::new(
                cfg.hidden_size_per_layer_input,
                cfg.rms_norm_eps,
                vb.pp("ple.per_layer_projection_norm"),
            )?,
            layers,
            norm: RmsNorm::new(cfg.hidden_size, cfg.rms_norm_eps, vb.pp("norm"))?,
            projection: linear(
                cfg.hidden_size,
                cfg.embedding_dim,
                vb.pp("embedding_projection"),
            )?,
            device: vb.device().clone(),
            cfg,
        })
    }

    pub fn embedding_dim(&self) -> usize {
        self.cfg.embedding_dim
    }

    /// トークン埋め込み (スケール済み)。
    pub fn embed_tokens(&self, ids: &Tensor) -> Result<Tensor> {
        self.embed.forward(ids)? * (self.cfg.hidden_size as f64).sqrt()
    }

    /// 層ごとの入力 (B, S, layers, per_layer_dim)。
    pub fn per_layer_inputs(&self, embeds: &Tensor) -> Result<Tensor> {
        let (b, s, _) = embeds.dims3()?;
        let p = (self.ple_proj.forward(embeds)? * (self.cfg.hidden_size as f64).powf(-0.5))?;
        let p = p.reshape((
            b,
            s,
            self.cfg.num_hidden_layers,
            self.cfg.hidden_size_per_layer_input,
        ))?;
        self.ple_norm.forward(&p)
    }

    /// トークンごとの出力 (B, S, embedding_dim)。`stop_after` を渡すとその層の出力 (B, S, hidden) を返す。
    pub fn forward(
        &self,
        ids: &Tensor,
        mask: &Tensor,
        stop_after: Option<usize>,
    ) -> Result<Tensor> {
        let (b, s) = ids.dims2()?;
        let x0 = self.embed_tokens(ids)?;
        let per_layer = self.per_layer_inputs(&x0)?;

        // パディングのキーを隠す (B, 1, 1, S)。窓付き層は |i - j| > sliding_window も隠す
        let pad_bias = ((mask.to_dtype(DType::F32)? - 1.0)? * 1e9)?.reshape((b, 1, 1, s))?;
        let sliding_bias = if s > self.cfg.sliding_window {
            let w = self.cfg.sliding_window as i64;
            let v: Vec<f32> = (0..s as i64)
                .flat_map(|i| {
                    (0..s as i64).map(move |j| if (i - j).abs() <= w { 0.0 } else { -1e9 })
                })
                .collect();
            pad_bias.broadcast_add(&Tensor::from_vec(v, (1, 1, s, s), &self.device)?)?
        } else {
            pad_bias.clone()
        };

        let mut ropes: HashMap<(usize, bool), Rope> = HashMap::new();
        let mut x = x0;
        for (i, layer) in self.layers.iter().enumerate() {
            let head_dim = layer.attn.head_dim;
            let key = (head_dim, layer.sliding);
            if let std::collections::hash_map::Entry::Vacant(slot) = ropes.entry(key) {
                let kind = if layer.sliding {
                    "sliding_attention"
                } else {
                    "full_attention"
                };
                let theta = self
                    .cfg
                    .rope_parameters
                    .get(kind)
                    .map(|r| r.rope_theta)
                    .unwrap_or(10_000.0);
                slot.insert(Rope::new(head_dim, theta, s, &self.device)?);
            }
            let bias = if layer.sliding {
                &sliding_bias
            } else {
                &pad_bias
            };
            let pl = per_layer.narrow(2, i, 1)?.squeeze(2)?;
            x = layer.forward(&x, &pl, bias, &ropes[&key])?;
            if stop_after == Some(i) {
                return Ok(x);
            }
        }
        self.projection.forward(&self.norm.forward(&x)?)
    }

    /// 平均プーリング + L2 正規化した埋め込み (B, embedding_dim)。
    pub fn embed(&self, ids: &Tensor, mask: &Tensor) -> Result<Tensor> {
        let h = self.forward(ids, mask, None)?;
        let m = mask.to_dtype(DType::F32)?.unsqueeze(D::Minus1)?;
        let pooled = h
            .broadcast_mul(&m)?
            .sum(1)?
            .broadcast_div(&m.sum(1)?.clamp(1.0, f64::MAX)?)?;
        let norm = pooled
            .sqr()?
            .sum_keepdim(D::Minus1)?
            .sqrt()?
            .clamp(1e-12, f64::MAX)?;
        pooled.broadcast_div(&norm)
    }
}

pub struct Paths {
    pub config: PathBuf,
    pub weights: PathBuf,
    pub tokenizer: PathBuf,
}

impl Paths {
    pub fn from_hf_cache(repo: &str) -> anyhow::Result<Self> {
        Ok(Self {
            config: hfcache::cached_file(repo, "config.json")?,
            weights: hfcache::cached_file(repo, "model.safetensors")?,
            tokenizer: hfcache::cached_file(repo, "tokenizer.json")?,
        })
    }
}

/// トークナイザ付きの埋め込みモデル。
pub struct EmbeddingGemma2 {
    model: TextModel,
    tokenizer: tokenizers::Tokenizer,
    device: Device,
}

impl EmbeddingGemma2 {
    pub fn load(paths: &Paths, device: &Device) -> anyhow::Result<Self> {
        let cfg: Config = serde_json::from_str(&std::fs::read_to_string(&paths.config)?)?;
        // SAFETY: 重みファイルは読み込み中に書き換えられない前提。
        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[&paths.weights], DType::F32, device)? };
        let model = TextModel::new(cfg.text_config, vb)?;
        let tokenizer = tokenizers::Tokenizer::from_file(&paths.tokenizer)
            .map_err(|e| anyhow!("load tokenizer: {e}"))?;
        Ok(Self {
            model,
            tokenizer,
            device: device.clone(),
        })
    }

    pub fn model(&self) -> &TextModel {
        &self.model
    }

    /// `<bos> ... <eos>` 付きのトークン列 (sentence-transformers と同じ)。最大 `max_len` トークン。
    pub fn tokenize(&self, text: &str, max_len: usize) -> anyhow::Result<Vec<u32>> {
        let enc = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow!("tokenize: {e}"))?;
        let mut ids = enc.get_ids().to_vec();
        if ids.len() > max_len {
            let eos = *ids.last().context("empty tokenization")?;
            ids.truncate(max_len - 1);
            ids.push(eos);
        }
        Ok(ids)
    }

    /// 文をまとめて埋め込む。プレフィックス ([`query_prompt`] / [`document_prompt`]) は呼び出し側で付ける。
    pub fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        const BATCH: usize = 16;
        const MAX_LEN: usize = 2048;
        let mut out = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(BATCH) {
            let ids = chunk
                .iter()
                .map(|t| self.tokenize(t, MAX_LEN))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let len = ids.iter().map(Vec::len).max().unwrap_or(1);
            let mut flat = Vec::with_capacity(chunk.len() * len);
            let mut mask = Vec::with_capacity(chunk.len() * len);
            for row in &ids {
                flat.extend(
                    row.iter()
                        .copied()
                        .chain(std::iter::repeat_n(0, len - row.len())),
                );
                mask.extend(
                    std::iter::repeat_n(1u32, row.len())
                        .chain(std::iter::repeat_n(0, len - row.len())),
                );
            }
            let ids = Tensor::from_vec(flat, (chunk.len(), len), &self.device)?;
            let mask = Tensor::from_vec(mask, (chunk.len(), len), &self.device)?;
            out.extend(self.model.embed(&ids, &mask)?.to_vec2::<f32>()?);
        }
        Ok(out)
    }
}

/// 正規化済みベクトル同士のコサイン類似度 (内積)。
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// 既定のモデルをキャッシュから読み込む。
pub fn load_default(device: &Device) -> anyhow::Result<EmbeddingGemma2> {
    EmbeddingGemma2::load(&Paths::from_hf_cache(DEFAULT_MODEL)?, device)
}
