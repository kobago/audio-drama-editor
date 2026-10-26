//! LiquidAI d1-3B の判断 (System One) を candle で計算する。
//!
//! 参照実装: d1-3B のリモートコード (`hybrid.py`、`lfm2_vl.py`、`runner.py`、`prompt.py`)。
//! LFM2-VL のうちテキスト部 (LFM2: 短い畳み込みと attention の混成) だけを移植する。
//!
//! 判断は文章を生成せず、答えの欄のロジットを選択肢のトークンで読む。状態 (幹) を 1 回だけ読んで
//! 層ごとの状態 (畳み込みの直近の入力と attention の K/V) を残し、質問 (枝) はまとめてその続きから読む。

mod prompt;

use std::path::PathBuf;

use anyhow::{Context, anyhow};
use candle_core::{DType, Device, IndexOp, Module, Result, Tensor};
use candle_nn::{Embedding, Linear, VarBuilder};
use serde::Deserialize;
use tokenizers::Tokenizer;

pub use prompt::{Question, prefix_text, readout_ids, suffix_text};

pub const DEFAULT_MODEL: &str = "LiquidAI/d1-3B";

/// 1 回にまとめて読む質問のトークン数の上限 (パディングを含む)。
const BRANCH_TOKENS: usize = 4096;

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

/// デバイスに合う重みの型。CPU は fp32、Metal は fp16 (M2 では bf16 が fp32 と同じ速さで、
/// fp16 のほうが速く fp32 にも近い)、CUDA は bf16 (配布されている重みの型)。
pub fn default_dtype(device: &Device) -> DType {
    match device {
        Device::Cpu => DType::F32,
        Device::Metal(_) => DType::F16,
        _ => DType::BF16,
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RopeParams {
    rope_theta: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TextConfig {
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub norm_eps: f64,
    #[serde(rename = "conv_L_cache")]
    pub conv_cache: usize,
    pub layer_types: Vec<String>,
    rope_parameters: RopeParams,
    pub vocab_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct VlConfig {
    text_config: TextConfig,
}

impl TextConfig {
    /// `config.json` (LFM2-VL) の `text_config`。
    pub fn from_vl_json(text: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str::<VlConfig>(text)?.text_config)
    }

    fn head_dim(&self) -> usize {
        self.hidden_size / self.num_attention_heads
    }
}

struct RmsNorm {
    weight: Tensor,
    eps: f32,
}

impl RmsNorm {
    fn new(dim: usize, eps: f64, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            weight: vb.get(dim, "weight")?,
            eps: eps as f32,
        })
    }
}

impl Module for RmsNorm {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        candle_nn::ops::rms_norm(&x.contiguous()?, &self.weight, self.eps)
    }
}

fn linear(i: usize, o: usize, vb: VarBuilder) -> Result<Linear> {
    Ok(Linear::new(vb.get((o, i), "weight")?, None))
}

struct Mlp {
    w1: Linear,
    w2: Linear,
    w3: Linear,
}

impl Mlp {
    fn new(cfg: &TextConfig, vb: VarBuilder) -> Result<Self> {
        let (d, h) = (cfg.hidden_size, cfg.intermediate_size);
        Ok(Self {
            w1: linear(d, h, vb.pp("w1"))?,
            w2: linear(h, d, vb.pp("w2"))?,
            w3: linear(d, h, vb.pp("w3"))?,
        })
    }
}

impl Module for Mlp {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let gate = candle_nn::ops::silu(&self.w1.forward(x)?)?;
        self.w2.forward(&(gate * self.w3.forward(x)?)?)
    }
}

/// `out(C * conv(B * x))`。畳み込みは深さ方向の因果畳み込み。
struct ShortConv {
    in_proj: Linear,
    out_proj: Linear,
    /// タップごとの重み (C,)。
    taps: Vec<Tensor>,
}

impl ShortConv {
    fn new(cfg: &TextConfig, vb: VarBuilder) -> Result<Self> {
        let d = cfg.hidden_size;
        let w = vb.get((d, 1, cfg.conv_cache), "conv.weight")?;
        let taps = (0..cfg.conv_cache)
            .map(|k| w.i((.., 0, k)))
            .collect::<Result<_>>()?;
        Ok(Self {
            in_proj: linear(d, 3 * d, vb.pp("in_proj"))?,
            out_proj: linear(d, d, vb.pp("out_proj"))?,
            taps,
        })
    }

    /// `x` (N, L, C)、`prev` は直前に読んだ入力 (1, keep, C)。戻り値は出力と、続きのための直近の入力。
    fn forward(&self, x: &Tensor, prev: Option<&Tensor>) -> Result<(Tensor, Tensor)> {
        let (n, l, c) = x.dims3()?;
        let keep = self.taps.len() - 1;
        let bcu = self.in_proj.forward(x)?;
        let b = bcu.narrow(2, 0, c)?;
        let cc = bcu.narrow(2, c, c)?;
        let u = bcu.narrow(2, 2 * c, c)?;
        let bu = (b * u)?;
        let lead = match prev {
            Some(p) => p.broadcast_as((n, keep, c))?.contiguous()?,
            None => Tensor::zeros((n, keep, c), bu.dtype(), bu.device())?,
        };
        let padded = Tensor::cat(&[&lead, &bu], 1)?;
        let mut z = padded.narrow(1, 0, l)?.broadcast_mul(&self.taps[0])?;
        for (k, tap) in self.taps.iter().enumerate().skip(1) {
            z = (z + padded.narrow(1, k, l)?.broadcast_mul(tap)?)?;
        }
        let state = padded.i(0..1)?.narrow(1, l, keep)?.contiguous()?;
        Ok((self.out_proj.forward(&(cc * z)?)?, state))
    }
}

struct Rope {
    inv_freq: Vec<f32>,
}

impl Rope {
    fn new(head_dim: usize, theta: f64) -> Self {
        let inv_freq = (0..head_dim / 2)
            .map(|i| (1.0 / theta.powf(2.0 * i as f64 / head_dim as f64)) as f32)
            .collect();
        Self { inv_freq }
    }

    /// 位置 `start..start + len` の cos / sin (len, head_dim / 2)。
    fn cos_sin(&self, start: usize, len: usize, device: &Device) -> Result<(Tensor, Tensor)> {
        let half = self.inv_freq.len();
        let angles: Vec<f32> = (start..start + len)
            .flat_map(|p| self.inv_freq.iter().map(move |f| p as f32 * f))
            .collect();
        let a = Tensor::from_vec(angles, (len, half), device)?;
        Ok((a.cos()?, a.sin()?))
    }
}

struct Attention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    out_proj: Linear,
    q_norm: RmsNorm,
    k_norm: RmsNorm,
    heads: usize,
    kv_heads: usize,
    head_dim: usize,
}

impl Attention {
    fn new(cfg: &TextConfig, vb: VarBuilder) -> Result<Self> {
        let (d, hd) = (cfg.hidden_size, cfg.head_dim());
        let (h, kv) = (cfg.num_attention_heads, cfg.num_key_value_heads);
        Ok(Self {
            q_proj: linear(d, h * hd, vb.pp("q_proj"))?,
            k_proj: linear(d, kv * hd, vb.pp("k_proj"))?,
            v_proj: linear(d, kv * hd, vb.pp("v_proj"))?,
            out_proj: linear(h * hd, d, vb.pp("out_proj"))?,
            q_norm: RmsNorm::new(hd, cfg.norm_eps, vb.pp("q_layernorm"))?,
            k_norm: RmsNorm::new(hd, cfg.norm_eps, vb.pp("k_layernorm"))?,
            heads: h,
            kv_heads: kv,
            head_dim: hd,
        })
    }

    /// `x` (N, L, d)。`prev` は幹の K/V (1, kv, P, D) で、各行はその続きとして読む。
    /// 戻り値は出力と、幹と合わせた K/V (N = 1 のときだけ続きに使える)。
    fn forward(
        &self,
        x: &Tensor,
        prev: Option<&(Tensor, Tensor)>,
        cos: &Tensor,
        sin: &Tensor,
    ) -> Result<(Tensor, (Tensor, Tensor))> {
        let (n, l, _) = x.dims3()?;
        let hd = self.head_dim;
        let heads = |t: Tensor, h: usize, norm: Option<&RmsNorm>| -> Result<Tensor> {
            let t = t.reshape((n, l, h, hd))?;
            let t = match norm {
                Some(norm) => norm.forward(&t)?,
                None => t,
            };
            t.transpose(1, 2)?.contiguous()
        };
        let q = heads(self.q_proj.forward(x)?, self.heads, Some(&self.q_norm))?;
        let k = heads(self.k_proj.forward(x)?, self.kv_heads, Some(&self.k_norm))?;
        let v = heads(self.v_proj.forward(x)?, self.kv_heads, None)?;
        // RoPE は fp32 で回してから元の型に戻す
        let rope = |t: &Tensor| -> Result<Tensor> {
            candle_nn::rotary_emb::rope(&t.to_dtype(DType::F32)?, cos, sin)?.to_dtype(x.dtype())
        };
        let (q, k) = (rope(&q)?, rope(&k)?);

        let (k_all, v_all, p) = match prev {
            Some((pk, pv)) => {
                let p = pk.dim(2)?;
                let pk = pk.broadcast_as((n, self.kv_heads, p, hd))?;
                let pv = pv.broadcast_as((n, self.kv_heads, p, hd))?;
                (Tensor::cat(&[&pk, &k], 2)?, Tensor::cat(&[&pv, &v], 2)?, p)
            }
            None => (k.clone(), v.clone(), 0),
        };
        let total = p + l;

        // attention は fp32 で。GQA は K/V の頭を繰り返す
        let groups = self.heads / self.kv_heads;
        let repeat = |t: &Tensor| -> Result<Tensor> {
            t.unsqueeze(2)?
                .broadcast_as((n, self.kv_heads, groups, total, hd))?
                .reshape((n, self.heads, total, hd))?
                .to_dtype(DType::F32)
        };
        let (kr, vr) = (repeat(&k_all)?, repeat(&v_all)?);
        let scale = (hd as f64).powf(-0.5);
        let scores = (q.to_dtype(DType::F32)?.matmul(&kr.t()?)? * scale)?;
        // 幹はすべて見え、自分の行の中は因果
        let mask: Vec<f32> = (0..l)
            .flat_map(|i| (0..total).map(move |j| if j <= p + i { 0.0 } else { f32::NEG_INFINITY }))
            .collect();
        let mask = Tensor::from_vec(mask, (1, 1, l, total), x.device())?;
        let probs = candle_nn::ops::softmax_last_dim(&scores.broadcast_add(&mask)?)?;
        let y = probs.matmul(&vr)?.to_dtype(x.dtype())?;
        let y = y.transpose(1, 2)?.reshape((n, l, self.heads * hd))?;
        let kv = if n == 1 {
            (k_all.contiguous()?, v_all.contiguous()?)
        } else {
            (k_all, v_all)
        };
        Ok((self.out_proj.forward(&y)?, kv))
    }
}

enum Operator {
    Conv(ShortConv),
    Attn(Attention),
}

struct Layer {
    op: Operator,
    mlp: Mlp,
    operator_norm: RmsNorm,
    ffn_norm: RmsNorm,
}

/// 層ごとに、続きを読むために残す状態。
#[derive(Clone)]
enum LayerState {
    Conv(Tensor),
    Attn(Tensor, Tensor),
}

/// 読んだ列の続きを読むための状態 (幹)。
#[derive(Clone)]
pub struct Trunk {
    len: usize,
    layers: Vec<LayerState>,
}

impl Trunk {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

pub struct TextModel {
    embed: Embedding,
    layers: Vec<Layer>,
    norm: RmsNorm,
    rope: Rope,
    lm_head: Linear,
    device: Device,
}

impl TextModel {
    /// `vb` は LFM2 の言語モデル (`model.language_model`) を指す。出力層は埋め込みと共有。
    pub fn new(cfg: &TextConfig, vb: VarBuilder) -> Result<Self> {
        let embed_w = vb.get((cfg.vocab_size, cfg.hidden_size), "embed_tokens.weight")?;
        let layers = cfg
            .layer_types
            .iter()
            .enumerate()
            .map(|(i, kind)| {
                let vb = vb.pp(format!("layers.{i}"));
                let op = if kind == "conv" {
                    Operator::Conv(ShortConv::new(cfg, vb.pp("conv"))?)
                } else {
                    Operator::Attn(Attention::new(cfg, vb.pp("self_attn"))?)
                };
                Ok(Layer {
                    op,
                    mlp: Mlp::new(cfg, vb.pp("feed_forward"))?,
                    operator_norm: RmsNorm::new(
                        cfg.hidden_size,
                        cfg.norm_eps,
                        vb.pp("operator_norm"),
                    )?,
                    ffn_norm: RmsNorm::new(cfg.hidden_size, cfg.norm_eps, vb.pp("ffn_norm"))?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            embed: Embedding::new(embed_w.clone(), cfg.hidden_size),
            layers,
            norm: RmsNorm::new(cfg.hidden_size, cfg.norm_eps, vb.pp("embedding_norm"))?,
            rope: Rope::new(cfg.head_dim(), cfg.rope_parameters.rope_theta),
            lm_head: Linear::new(embed_w, None),
            device: vb.device().clone(),
        })
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    /// `ids` (N, L) を、`trunk` があればその続きとして読む。行は右詰めのパディングでよい
    /// (因果なので、パディングは前のトークンに届かない)。
    ///
    /// 戻り値は最後の正規化後の隠れ状態 (N, L, d) と、N = 1 のときの続きのための状態。
    /// `stop_after` を指定すると、その層の出力 (正規化前) をそのまま返す。
    pub fn forward(
        &self,
        ids: &Tensor,
        trunk: Option<&Trunk>,
        stop_after: Option<usize>,
    ) -> Result<(Tensor, Option<Trunk>)> {
        let (n, l) = ids.dims2()?;
        let start = trunk.map_or(0, |t| t.len);
        let (cos, sin) = self.rope.cos_sin(start, l, &self.device)?;
        let mut x = self.embed.forward(ids)?;
        let mut states = Vec::with_capacity(self.layers.len());
        for (i, layer) in self.layers.iter().enumerate() {
            let h = layer.operator_norm.forward(&x)?;
            let prev = trunk.map(|t| &t.layers[i]);
            let (y, state) = match &layer.op {
                Operator::Conv(conv) => {
                    let prev = match prev {
                        Some(LayerState::Conv(p)) => Some(p),
                        _ => None,
                    };
                    let (y, s) = conv.forward(&h, prev)?;
                    (y, LayerState::Conv(s))
                }
                Operator::Attn(attn) => {
                    let prev = match prev {
                        Some(LayerState::Attn(k, v)) => Some((k.clone(), v.clone())),
                        _ => None,
                    };
                    let (y, (k, v)) = attn.forward(&h, prev.as_ref(), &cos, &sin)?;
                    (y, LayerState::Attn(k, v))
                }
            };
            let h = (x + y)?;
            x = (&h + layer.mlp.forward(&layer.ffn_norm.forward(&h)?)?)?;
            states.push(state);
            if stop_after == Some(i) {
                return Ok((x, None));
            }
        }
        let trunk = (n == 1).then(|| Trunk {
            len: start + l,
            layers: states,
        });
        Ok((self.norm.forward(&x)?, trunk))
    }

    /// 正規化後の隠れ状態 (N, d) から語彙のロジット (N, vocab) を fp32 で。
    pub fn logits(&self, hidden: &Tensor) -> Result<Tensor> {
        self.lm_head.forward(hidden)?.to_dtype(DType::F32)
    }

    /// `trunk` の続きとして各行を読み、各行の最後のトークンでのロジット (N, vocab)。
    pub fn answer(&self, trunk: &Trunk, rows: &[Vec<u32>]) -> Result<Tensor> {
        let longest = rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut flat = Vec::with_capacity(rows.len() * longest);
        for r in rows {
            flat.extend_from_slice(r);
            flat.extend(std::iter::repeat_n(0u32, longest - r.len()));
        }
        let ids = Tensor::from_vec(flat, (rows.len(), longest), &self.device)?;
        let (h, _) = self.forward(&ids, Some(trunk), None)?;
        let last: Vec<Tensor> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| h.i((i, r.len() - 1)))
            .collect::<Result<_>>()?;
        self.logits(&Tensor::stack(&last, 0)?)
    }

    /// 列を読み、続きのための状態を返す。
    pub fn read(&self, ids: &[u32]) -> Result<Trunk> {
        let t = Tensor::new(ids, &self.device)?.unsqueeze(0)?;
        let (_, trunk) = self.forward(&t, None, None)?;
        Ok(trunk.expect("one row"))
    }
}

pub struct Paths {
    pub config: PathBuf,
    pub tokenizer: PathBuf,
    pub weights: PathBuf,
}

impl Paths {
    pub fn from_hf_cache(repo: &str) -> anyhow::Result<Self> {
        Ok(Self {
            config: hfcache::cached_file(repo, "config.json")?,
            tokenizer: hfcache::cached_file(repo, "tokenizer.json")?,
            weights: hfcache::cached_file(repo, "model.safetensors")?,
        })
    }
}

/// d1-3B の判断。状態に対する質問ごとに、選択肢の確率を返す。
pub struct D1 {
    model: TextModel,
    tokenizer: Tokenizer,
    bos: String,
}

impl D1 {
    pub fn load(paths: &Paths, device: &Device, dtype: DType) -> anyhow::Result<Self> {
        let cfg = TextConfig::from_vl_json(&std::fs::read_to_string(&paths.config)?)?;
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[&paths.weights], dtype, device)? };
        let model = TextModel::new(&cfg, vb.pp("model.language_model"))?;
        let tokenizer =
            Tokenizer::from_file(&paths.tokenizer).map_err(|e| anyhow!("tokenizer: {e}"))?;
        Ok(Self {
            model,
            tokenizer,
            bos: "<|startoftext|>".into(),
        })
    }

    pub fn model(&self) -> &TextModel {
        &self.model
    }

    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// 状態を 1 回だけ読み、各質問の選択肢の確率を返す (noul は [はい, いいえ])。
    pub fn probabilities(
        &self,
        state: &str,
        questions: &[Question],
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        let trunk_ids = prompt::tokens(&self.tokenizer, &prefix_text(&self.bos, Some(state)))?;
        let trunk = self.model.read(&trunk_ids).context("read state")?;
        let rows = questions
            .iter()
            .map(|q| prompt::tokens(&self.tokenizer, &suffix_text(&self.tokenizer, q)?))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut out = vec![Vec::new(); questions.len()];
        for batch in plan(&rows, BRANCH_TOKENS) {
            let batch_rows: Vec<Vec<u32>> = batch.iter().map(|&i| rows[i].clone()).collect();
            let logits = self.model.answer(&trunk, &batch_rows)?;
            for (k, &i) in batch.iter().enumerate() {
                out[i] = readout(&self.tokenizer, &questions[i], &logits.i(k)?)?;
            }
        }
        Ok(out)
    }
}

/// 行を長さ順に並べ、パディング込みで `budget` トークンに収まるまとまりに分ける (行の添字)。
fn plan(rows: &[Vec<u32>], budget: usize) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by_key(|&i| rows[i].len());
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    for i in order {
        // 長さ順なので、いま足す行がまとまりでいちばん長い
        if !cur.is_empty() && (cur.len() + 1) * rows[i].len() > budget {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(i);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 答えの欄のロジット (vocab,) から、選択肢ごとの確率。各選択肢はいちばん高い表記で数える。
/// (全語彙の log-softmax の正規化項は選択肢の間で打ち消し合うので、ロジットのままでよい。)
pub fn readout(tok: &Tokenizer, q: &Question, logits: &Tensor) -> anyhow::Result<Vec<f32>> {
    let logits: Vec<f32> = logits.to_vec1()?;
    let scores: Vec<f32> = readout_ids(tok, q)?
        .iter()
        .map(|g| {
            g.iter()
                .map(|&i| logits[i as usize])
                .fold(f32::MIN, f32::max)
        })
        .collect();
    let m = scores.iter().copied().fold(f32::MIN, f32::max);
    let exps: Vec<f32> = scores.iter().map(|s| (s - m).exp()).collect();
    let sum: f32 = exps.iter().sum();
    Ok(exps.into_iter().map(|e| e / sum).collect())
}

/// score の期待値 (0〜段階数-1)。
pub fn expected_level(probs: &[f32]) -> f32 {
    probs.iter().enumerate().map(|(i, p)| i as f32 * p).sum()
}

pub fn load_default(device: &Device) -> anyhow::Result<D1> {
    D1::load(
        &Paths::from_hf_cache(DEFAULT_MODEL)?,
        device,
        default_dtype(device),
    )
}
