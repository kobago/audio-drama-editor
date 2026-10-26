//! `irodori_tts.model.TextToLatentRFDiT` の推論部分。

use anyhow::bail;
use candle_core::{D, Device, Module, Result, Tensor};
use candle_nn::{Embedding, VarBuilder};

use crate::config::ModelConfig;
use crate::layers::{Linear, RmsNorm, Rope, SwiGlu, apply_mask, attention, linear, mask_bias};
use crate::ops::{self, Axis};

struct SelfAttention {
    wq: Linear,
    wk: Linear,
    wv: Linear,
    wo: Linear,
    gate: Linear,
    q_norm: RmsNorm,
    k_norm: RmsNorm,
    heads: usize,
    head_dim: usize,
}

impl SelfAttention {
    fn new(dim: usize, heads: usize, eps: f64, vb: VarBuilder) -> Result<Self> {
        let head_dim = dim / heads;
        Ok(Self {
            wq: linear(dim, dim, false, vb.pp("wq"))?,
            wk: linear(dim, dim, false, vb.pp("wk"))?,
            wv: linear(dim, dim, false, vb.pp("wv"))?,
            wo: linear(dim, dim, false, vb.pp("wo"))?,
            gate: linear(dim, dim, false, vb.pp("gate"))?,
            q_norm: RmsNorm::new((heads, head_dim), eps, vb.pp("q_norm"))?,
            k_norm: RmsNorm::new((heads, head_dim), eps, vb.pp("k_norm"))?,
            heads,
            head_dim,
        })
    }

    fn forward(&self, x: &Tensor, bias: &Tensor, rope: &Rope) -> Result<Tensor> {
        let (b, s, dim) = x.dims3()?;
        let split = |t: Tensor| t.reshape((b, s, self.heads, self.head_dim));
        let q = ops::swap12(&self.q_norm.forward(&split(self.wq.forward(x)?)?)?)?;
        let k = ops::swap12(&self.k_norm.forward(&split(self.wk.forward(x)?)?)?)?;
        let v = ops::swap12(&split(self.wv.forward(x)?)?)?;
        let y = attention(&rope.apply(&q)?, &rope.apply(&k)?, &v, bias)?;
        let y = ops::swap12(&y)?.reshape((b, s, dim))?;
        let y = (y * candle_nn::ops::sigmoid(&self.gate.forward(x)?)?)?;
        self.wo.forward(&y)
    }
}

struct TextBlock {
    attention_norm: RmsNorm,
    attention: SelfAttention,
    mlp_norm: RmsNorm,
    mlp: SwiGlu,
}

impl TextBlock {
    fn new(dim: usize, heads: usize, mlp_ratio: f64, eps: f64, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            attention_norm: RmsNorm::new(dim, eps, vb.pp("attention_norm"))?,
            attention: SelfAttention::new(dim, heads, eps, vb.pp("attention"))?,
            mlp_norm: RmsNorm::new(dim, eps, vb.pp("mlp_norm"))?,
            mlp: SwiGlu::new(dim, (dim as f64 * mlp_ratio) as usize, vb.pp("mlp"))?,
        })
    }

    fn forward(&self, x: &Tensor, bias: &Tensor, rope: &Rope) -> Result<Tensor> {
        let x = (x + self
            .attention
            .forward(&self.attention_norm.forward(x)?, bias, rope)?)?;
        &x + self.mlp.forward(&self.mlp_norm.forward(&x)?)?
    }
}

/// TextEncoder / caption encoder / ReferenceLatentEncoder の共通部分。
struct MaskedEncoder {
    blocks: Vec<TextBlock>,
    head_dim: usize,
}

impl MaskedEncoder {
    fn new(
        dim: usize,
        layers: usize,
        heads: usize,
        mlp_ratio: f64,
        eps: f64,
        vb: VarBuilder,
    ) -> Result<Self> {
        let blocks = (0..layers)
            .map(|i| TextBlock::new(dim, heads, mlp_ratio, eps, vb.pp(format!("blocks.{i}"))))
            .collect::<Result<_>>()?;
        Ok(Self {
            blocks,
            head_dim: dim / heads,
        })
    }

    fn forward(&self, x: Tensor, mask: &Tensor) -> Result<Tensor> {
        let rope = Rope::new(self.head_dim, x.dim(1)?, x.device())?;
        let bias = mask_bias(mask)?;
        let mut x = apply_mask(&x, mask)?;
        for block in &self.blocks {
            x = apply_mask(&block.forward(&x, &bias, &rope)?, mask)?;
        }
        Ok(x)
    }
}

struct TextEncoder {
    embedding: Embedding,
    encoder: MaskedEncoder,
}

impl TextEncoder {
    #[allow(clippy::too_many_arguments)]
    fn new(
        vocab: usize,
        dim: usize,
        layers: usize,
        heads: usize,
        mlp_ratio: f64,
        eps: f64,
        vb: VarBuilder,
    ) -> Result<Self> {
        Ok(Self {
            embedding: Embedding::new(vb.get((vocab, dim), "text_embedding.weight")?, dim),
            encoder: MaskedEncoder::new(dim, layers, heads, mlp_ratio, eps, vb)?,
        })
    }

    fn forward(&self, ids: &Tensor, mask: &Tensor) -> Result<Tensor> {
        self.encoder.forward(self.embedding.forward(ids)?, mask)
    }
}

struct ReferenceLatentEncoder {
    in_proj: Linear,
    encoder: MaskedEncoder,
}

impl ReferenceLatentEncoder {
    fn new(cfg: &ModelConfig, vb: VarBuilder) -> Result<Self> {
        let in_dim = cfg.patched_latent_dim() * cfg.speaker_patch_size;
        Ok(Self {
            in_proj: linear(in_dim, cfg.speaker_dim, true, vb.pp("in_proj"))?,
            encoder: MaskedEncoder::new(
                cfg.speaker_dim,
                cfg.speaker_layers,
                cfg.speaker_heads,
                cfg.speaker_mlp_ratio(),
                cfg.norm_eps,
                vb,
            )?,
        })
    }

    fn forward(&self, latent: &Tensor, mask: &Tensor) -> Result<Tensor> {
        self.encoder
            .forward((self.in_proj.forward(latent)? / 6.0)?, mask)
    }
}

/// Echo 方式の low-rank AdaLN。正規化後の x と tanh 済みの gate を返す。
struct LowRankAdaLn {
    shift_down: Linear,
    scale_down: Linear,
    gate_down: Linear,
    shift_up: Linear,
    scale_up: Linear,
    gate_up: Linear,
    eps: f32,
}

impl LowRankAdaLn {
    fn new(dim: usize, rank: usize, eps: f64, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            shift_down: linear(dim, rank, false, vb.pp("shift_down"))?,
            scale_down: linear(dim, rank, false, vb.pp("scale_down"))?,
            gate_down: linear(dim, rank, false, vb.pp("gate_down"))?,
            shift_up: linear(rank, dim, true, vb.pp("shift_up"))?,
            scale_up: linear(rank, dim, true, vb.pp("scale_up"))?,
            gate_up: linear(rank, dim, true, vb.pp("gate_up"))?,
            eps: eps as f32,
        })
    }

    fn forward(&self, x: &Tensor, cond: &Tensor) -> Result<(Tensor, Tensor)> {
        let chunks = cond.chunk(3, D::Minus1)?;
        let shift = (self
            .shift_up
            .forward(&self.shift_down.forward(&chunks[0].silu()?)?)?
            + &chunks[0])?;
        let scale = (self
            .scale_up
            .forward(&self.scale_down.forward(&chunks[1].silu()?)?)?
            + &chunks[1])?;
        let gate = (self
            .gate_up
            .forward(&self.gate_down.forward(&chunks[2].silu()?)?)?
            + &chunks[2])?;
        // cond は全バッチ共通 (1, 1, D) なので、(1 + scale) を RMSNorm の重みとして畳み込む
        let x =
            candle_nn::ops::rms_norm(&x.contiguous()?, &(scale + 1.0)?.flatten_all()?, self.eps)?;
        let x = ops::bcast_add(&x, &shift.flatten_all()?, Axis::Last)?;
        Ok((x, gate.tanh()?.flatten_all()?))
    }
}

/// JointAttention の条件側 K/V (サンプリング中は固定なので事前計算する)。
pub struct ContextKv {
    k: Tensor,
    v: Tensor,
}

struct JointAttention {
    wq: Linear,
    wk: Linear,
    wv: Linear,
    wk_text: Linear,
    wv_text: Linear,
    wk_speaker: Option<(Linear, Linear)>,
    wk_caption: Option<(Linear, Linear)>,
    gate: Linear,
    wo: Linear,
    q_norm: RmsNorm,
    k_norm: RmsNorm,
    heads: usize,
    head_dim: usize,
}

impl JointAttention {
    fn new(cfg: &ModelConfig, vb: VarBuilder) -> Result<Self> {
        let dim = cfg.model_dim;
        let heads = cfg.num_heads;
        let head_dim = dim / heads;
        let kv = |ctx: usize, name: &str| -> Result<(Linear, Linear)> {
            Ok((
                linear(ctx, dim, false, vb.pp(format!("wk_{name}")))?,
                linear(ctx, dim, false, vb.pp(format!("wv_{name}")))?,
            ))
        };
        Ok(Self {
            wq: linear(dim, dim, false, vb.pp("wq"))?,
            wk: linear(dim, dim, false, vb.pp("wk"))?,
            wv: linear(dim, dim, false, vb.pp("wv"))?,
            wk_text: linear(cfg.text_dim, dim, false, vb.pp("wk_text"))?,
            wv_text: linear(cfg.text_dim, dim, false, vb.pp("wv_text"))?,
            wk_speaker: if cfg.use_speaker_condition() {
                Some(kv(cfg.speaker_dim, "speaker")?)
            } else {
                None
            },
            wk_caption: if cfg.use_caption_condition {
                Some(kv(cfg.caption_dim(), "caption")?)
            } else {
                None
            },
            gate: linear(dim, dim, false, vb.pp("gate"))?,
            wo: linear(dim, dim, false, vb.pp("wo"))?,
            q_norm: RmsNorm::new((heads, head_dim), cfg.norm_eps, vb.pp("q_norm"))?,
            k_norm: RmsNorm::new((heads, head_dim), cfg.norm_eps, vb.pp("k_norm"))?,
            heads,
            head_dim,
        })
    }

    fn split_heads(&self, x: &Tensor) -> Result<Tensor> {
        let (b, s, _) = x.dims3()?;
        x.reshape((b, s, self.heads, self.head_dim))
    }

    fn project_context(&self, cond: &Conditions) -> Result<ContextKv> {
        let mut ks = Vec::new();
        let mut vs = Vec::new();
        let mut push = |wk: &Linear, wv: &Linear, ctx: &Tensor| -> Result<()> {
            ks.push(ops::swap12(
                &self.k_norm.forward(&self.split_heads(&wk.forward(ctx)?)?)?,
            )?);
            vs.push(ops::swap12(&self.split_heads(&wv.forward(ctx)?)?)?);
            Ok(())
        };
        push(&self.wk_text, &self.wv_text, &cond.text_state)?;
        if let Some((wk, wv)) = &self.wk_speaker {
            push(wk, wv, &cond.speaker_state)?;
        }
        if let Some((wk, wv)) = &self.wk_caption {
            push(wk, wv, &cond.caption_state)?;
        }
        Ok(ContextKv {
            k: Tensor::cat(&ks, 2)?.contiguous()?,
            v: Tensor::cat(&vs, 2)?.contiguous()?,
        })
    }

    /// 前半のヘッドだけに RoPE をかける (chunk(2, dim=-2))。x: (B, H, S, Dh)
    fn rotary_half(&self, x: &Tensor, rope: &Rope) -> Result<Tensor> {
        let half = self.heads / 2;
        let rotated = rope.apply(&x.narrow(1, 0, half)?)?;
        Tensor::cat(
            &[rotated, x.narrow(1, half, self.heads - half)?.contiguous()?],
            1,
        )
    }

    fn forward(&self, x: &Tensor, ctx: &ContextKv, bias: &Tensor, rope: &Rope) -> Result<Tensor> {
        let (b, s, dim) = x.dims3()?;
        let q = ops::swap12(
            &self
                .q_norm
                .forward(&self.split_heads(&self.wq.forward(x)?)?)?,
        )?;
        let k = ops::swap12(
            &self
                .k_norm
                .forward(&self.split_heads(&self.wk.forward(x)?)?)?,
        )?;
        let v = ops::swap12(&self.split_heads(&self.wv.forward(x)?)?)?;
        let q = self.rotary_half(&q, rope)?;
        let k = Tensor::cat(&[self.rotary_half(&k, rope)?, ctx.k.clone()], 2)?;
        let v = Tensor::cat(&[v, ctx.v.clone()], 2)?;
        let y = ops::swap12(&attention(&q, &k, &v, bias)?)?.reshape((b, s, dim))?;
        let y = (y * candle_nn::ops::sigmoid(&self.gate.forward(x)?)?)?;
        self.wo.forward(&y)
    }
}

struct DiffusionBlock {
    attention: JointAttention,
    mlp: SwiGlu,
    attention_adaln: LowRankAdaLn,
    mlp_adaln: LowRankAdaLn,
}

impl DiffusionBlock {
    fn new(cfg: &ModelConfig, vb: VarBuilder) -> Result<Self> {
        let rank = cfg.adaln_rank.clamp(1, cfg.model_dim);
        Ok(Self {
            attention: JointAttention::new(cfg, vb.pp("attention"))?,
            mlp: SwiGlu::new(
                cfg.model_dim,
                (cfg.model_dim as f64 * cfg.mlp_ratio) as usize,
                vb.pp("mlp"),
            )?,
            attention_adaln: LowRankAdaLn::new(
                cfg.model_dim,
                rank,
                cfg.norm_eps,
                vb.pp("attention_adaln"),
            )?,
            mlp_adaln: LowRankAdaLn::new(cfg.model_dim, rank, cfg.norm_eps, vb.pp("mlp_adaln"))?,
        })
    }

    fn forward(
        &self,
        x: &Tensor,
        cond: &Tensor,
        ctx: &ContextKv,
        bias: &Tensor,
        rope: &Rope,
    ) -> Result<Tensor> {
        let (h, gate) = self.attention_adaln.forward(x, cond)?;
        let x = ops::bcast_fma(
            x,
            &gate,
            &self.attention.forward(&h, ctx, bias, rope)?,
            Axis::Last,
        )?;
        let (h, gate) = self.mlp_adaln.forward(&x, cond)?;
        ops::bcast_fma(&x, &gate, &self.mlp.forward(&h)?, Axis::Last)
    }
}

struct DurationBlock {
    norm: RmsNorm,
    mlp: SwiGlu,
    modulation: Linear,
    caption_modulation: Option<Linear>,
}

impl DurationBlock {
    fn forward(&self, x: &Tensor, speaker: &Tensor, caption: Option<&Tensor>) -> Result<Tensor> {
        let h = self.norm.forward(x)?;
        let mut m = self.modulation.forward(&speaker.silu()?)?;
        if let (Some(lin), Some(c)) = (&self.caption_modulation, caption) {
            m = (m + lin.forward(&c.silu()?)?)?;
        }
        let m = m.unsqueeze(1)?.chunk(3, D::Minus1)?;
        let h = h.broadcast_mul(&(&m[1] + 1.0)?)?.broadcast_add(&m[0])?;
        x + m[2].tanh()?.broadcast_mul(&self.mlp.forward(&h)?)?
    }
}

/// token_sum_adarn_zero_no_aux / token_sum_dual_adarn_zero_no_aux の尺予測器。
struct DurationPredictor {
    null_speaker: Tensor,
    null_caption: Option<Tensor>,
    token_input_proj: Linear,
    blocks: Vec<DurationBlock>,
    out_norm: RmsNorm,
    out_proj: Linear,
}

impl DurationPredictor {
    fn new(cfg: &ModelConfig, vb: VarBuilder) -> anyhow::Result<Self> {
        let dual = match cfg.duration_architecture.as_str() {
            "token_sum_adarn_zero_no_aux" => false,
            "token_sum_dual_adarn_zero_no_aux" => true,
            other => bail!("unsupported duration architecture: {other}"),
        };
        let hidden = cfg.duration_hidden_dim;
        let blocks = (0..cfg.duration_layers)
            .map(|i| {
                let vb = vb.pp(format!("token_blocks.{i}"));
                Ok(DurationBlock {
                    norm: RmsNorm::new(hidden, cfg.norm_eps, vb.pp("norm"))?,
                    mlp: SwiGlu::new(hidden, hidden, vb.pp("mlp"))?,
                    modulation: linear(cfg.speaker_dim, hidden * 3, true, vb.pp("modulation"))?,
                    caption_modulation: if dual {
                        Some(linear(
                            cfg.caption_dim(),
                            hidden * 3,
                            true,
                            vb.pp("caption_modulation"),
                        )?)
                    } else {
                        None
                    },
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            null_speaker: vb.get(cfg.speaker_dim, "null_speaker")?,
            null_caption: if dual {
                Some(vb.get(cfg.caption_dim(), "null_caption")?)
            } else {
                None
            },
            token_input_proj: linear(cfg.text_dim, hidden, true, vb.pp("token_input_proj"))?,
            blocks,
            out_norm: RmsNorm::new(hidden, cfg.norm_eps, vb.pp("token_out_norm"))?,
            out_proj: linear(hidden, 1, true, vb.pp("token_out_proj"))?,
        })
    }

    /// 予測フレーム数の log1p を返す。
    fn forward(&self, cond: &Conditions) -> Result<f32> {
        let speaker = if cond.has_speaker {
            cond.speaker_state.narrow(1, 0, 1)?.squeeze(1)?
        } else {
            self.null_speaker.unsqueeze(0)?
        };
        let caption = match &self.null_caption {
            Some(_) if cond.has_caption => {
                Some(masked_mean(&cond.caption_state, &cond.caption_mask)?)
            }
            Some(null) => Some(null.unsqueeze(0)?),
            None => None,
        };
        let mut h = self.token_input_proj.forward(&cond.text_state)?;
        for block in &self.blocks {
            h = block.forward(&h, &speaker, caption.as_ref())?;
        }
        let logits = self
            .out_proj
            .forward(&self.out_norm.forward(&h)?)?
            .squeeze(D::Minus1)?;
        // softplus
        let frames = (logits.exp()? + 1.0)?.log()?;
        let total = (frames * &cond.text_mask)?.sum_all()?.to_scalar::<f32>()?;
        Ok(total.max(0.0).ln_1p())
    }
}

fn masked_mean(state: &Tensor, mask: &Tensor) -> Result<Tensor> {
    let denom = mask.sum_keepdim(1)?.clamp(1.0, f64::MAX)?;
    apply_mask(state, mask)?.sum(1)?.broadcast_div(&denom)
}

/// 符号化済みの条件。B = 1 で持ち、CFG 時は uncond と並べてバッチ化する。
#[derive(Clone)]
pub struct Conditions {
    pub text_state: Tensor,
    pub text_mask: Tensor,
    pub speaker_state: Tensor,
    pub speaker_mask: Tensor,
    pub caption_state: Tensor,
    pub caption_mask: Tensor,
    pub has_speaker: bool,
    pub has_caption: bool,
}

impl Conditions {
    /// 指定した条件をゼロ状態・全マスクにしたもの (CFG の unconditional 側)。
    pub fn drop(&self, text: bool, speaker: bool, caption: bool) -> Result<Self> {
        let zero = |t: &Tensor, on: bool| if on { t.zeros_like() } else { Ok(t.clone()) };
        Ok(Self {
            text_state: zero(&self.text_state, text)?,
            text_mask: zero(&self.text_mask, text)?,
            speaker_state: zero(&self.speaker_state, speaker)?,
            speaker_mask: zero(&self.speaker_mask, speaker)?,
            caption_state: zero(&self.caption_state, caption)?,
            caption_mask: zero(&self.caption_mask, caption)?,
            has_speaker: self.has_speaker && !speaker,
            has_caption: self.has_caption && !caption,
        })
    }

    pub fn cat(items: &[Self]) -> Result<Self> {
        let cat =
            |f: fn(&Self) -> &Tensor| Tensor::cat(&items.iter().map(f).collect::<Vec<_>>(), 0);
        Ok(Self {
            text_state: cat(|c| &c.text_state)?,
            text_mask: cat(|c| &c.text_mask)?,
            speaker_state: cat(|c| &c.speaker_state)?,
            speaker_mask: cat(|c| &c.speaker_mask)?,
            caption_state: cat(|c| &c.caption_state)?,
            caption_mask: cat(|c| &c.caption_mask)?,
            has_speaker: items[0].has_speaker,
            has_caption: items[0].has_caption,
        })
    }
}

pub struct TextToLatent {
    pub cfg: ModelConfig,
    text_encoder: TextEncoder,
    text_norm: RmsNorm,
    caption_encoder: Option<(TextEncoder, RmsNorm)>,
    speaker_encoder: Option<(ReferenceLatentEncoder, RmsNorm)>,
    duration_predictor: Option<DurationPredictor>,
    cond_module: [Linear; 3],
    in_proj: Linear,
    blocks: Vec<DiffusionBlock>,
    out_norm: RmsNorm,
    out_proj: Linear,
    device: Device,
}

impl TextToLatent {
    pub fn new(cfg: ModelConfig, vb: VarBuilder) -> anyhow::Result<Self> {
        let eps = cfg.norm_eps;
        let text_encoder = TextEncoder::new(
            cfg.text_vocab_size,
            cfg.text_dim,
            cfg.text_layers,
            cfg.text_heads,
            cfg.text_mlp_ratio(),
            eps,
            vb.pp("text_encoder"),
        )?;
        let caption_encoder = if cfg.use_caption_condition {
            Some((
                TextEncoder::new(
                    cfg.caption_vocab_size(),
                    cfg.caption_dim(),
                    cfg.caption_layers(),
                    cfg.caption_heads(),
                    cfg.caption_mlp_ratio(),
                    eps,
                    vb.pp("caption_encoder"),
                )?,
                RmsNorm::new(cfg.caption_dim(), eps, vb.pp("caption_norm"))?,
            ))
        } else {
            None
        };
        let speaker_encoder = if cfg.use_speaker_condition() {
            Some((
                ReferenceLatentEncoder::new(&cfg, vb.pp("speaker_encoder"))?,
                RmsNorm::new(cfg.speaker_dim, eps, vb.pp("speaker_norm"))?,
            ))
        } else {
            None
        };
        if !cfg.use_speaker_condition() {
            bail!("checkpoints without speaker conditioning are not supported");
        }
        let duration_predictor = if cfg.use_duration_predictor {
            Some(DurationPredictor::new(&cfg, vb.pp("duration_predictor"))?)
        } else {
            None
        };
        let dim = cfg.model_dim;
        let cond_module = [
            linear(cfg.timestep_embed_dim, dim, false, vb.pp("cond_module.0"))?,
            linear(dim, dim, false, vb.pp("cond_module.2"))?,
            linear(dim, dim * 3, false, vb.pp("cond_module.4"))?,
        ];
        let blocks = (0..cfg.num_layers)
            .map(|i| DiffusionBlock::new(&cfg, vb.pp(format!("blocks.{i}"))))
            .collect::<Result<_>>()?;
        Ok(Self {
            text_encoder,
            text_norm: RmsNorm::new(cfg.text_dim, eps, vb.pp("text_norm"))?,
            caption_encoder,
            speaker_encoder,
            duration_predictor,
            cond_module,
            in_proj: linear(cfg.patched_latent_dim(), dim, true, vb.pp("in_proj"))?,
            blocks,
            out_norm: RmsNorm::new(dim, eps, vb.pp("out_norm"))?,
            out_proj: linear(dim, cfg.patched_latent_dim(), true, vb.pp("out_proj"))?,
            device: vb.device().clone(),
            cfg,
        })
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    /// `encode_conditions` 相当 (B = 1)。
    /// `ref_latent` は (1, T, latent_dim)。None なら no-ref (全マスク)。caption が空なら全マスク。
    pub fn encode_conditions(
        &self,
        text_ids: &[u32],
        ref_latent: Option<&Tensor>,
        caption_ids: &[u32],
        has_caption: bool,
    ) -> Result<Conditions> {
        let dev = &self.device;
        let ids = Tensor::new(text_ids, dev)?.unsqueeze(0)?;
        let text_mask = Tensor::ones((1, text_ids.len()), candle_core::DType::F32, dev)?;
        let text_state = self
            .text_norm
            .forward(&self.text_encoder.forward(&ids, &text_mask)?)?;

        let (encoder, norm) = self.speaker_encoder.as_ref().expect("speaker encoder");
        let (latent, mask, has_speaker) = match ref_latent {
            Some(l) => (
                l.clone(),
                Tensor::ones((1, l.dim(1)?), candle_core::DType::F32, dev)?,
                true,
            ),
            None => {
                let size = self.cfg.speaker_patch_size.max(1);
                (
                    Tensor::zeros(
                        (1, size, self.cfg.patched_latent_dim()),
                        candle_core::DType::F32,
                        dev,
                    )?,
                    Tensor::zeros((1, size), candle_core::DType::F32, dev)?,
                    false,
                )
            }
        };
        let (latent, mask) = patch_sequence(&latent, &mask, self.cfg.speaker_patch_size)?;
        let speaker = norm.forward(&encoder.forward(&latent, &mask)?)?;
        let mean = masked_mean(&speaker, &mask)?.unsqueeze(1)?;
        let any = mask.max_keepdim(1)?;
        let speaker_state = Tensor::cat(&[mean, speaker], 1)?;
        let speaker_mask = Tensor::cat(&[any, mask], 1)?;

        let (caption_state, caption_mask) = match &self.caption_encoder {
            Some((encoder, norm)) => {
                let ids = Tensor::new(caption_ids, dev)?.unsqueeze(0)?;
                let mask = if has_caption {
                    Tensor::ones((1, caption_ids.len()), candle_core::DType::F32, dev)?
                } else {
                    Tensor::zeros((1, caption_ids.len()), candle_core::DType::F32, dev)?
                };
                (norm.forward(&encoder.forward(&ids, &mask)?)?, mask)
            }
            None => (
                Tensor::zeros((1, 0, 1), candle_core::DType::F32, dev)?,
                Tensor::zeros((1, 0), candle_core::DType::F32, dev)?,
            ),
        };
        Ok(Conditions {
            text_state,
            text_mask,
            speaker_state,
            speaker_mask,
            caption_state,
            caption_mask,
            has_speaker,
            has_caption: has_caption && self.caption_encoder.is_some(),
        })
    }

    /// 予測フレーム数の log1p。尺予測器のないチェックポイントでは None。
    pub fn predict_duration_log_frames(&self, cond: &Conditions) -> Result<Option<f32>> {
        self.duration_predictor
            .as_ref()
            .map(|d| d.forward(cond))
            .transpose()
    }

    pub fn build_context(&self, cond: &Conditions) -> Result<(Vec<ContextKv>, Tensor)> {
        let kv = self
            .blocks
            .iter()
            .map(|b| b.attention.project_context(cond))
            .collect::<Result<_>>()?;
        let mut masks = vec![cond.text_mask.clone(), cond.speaker_mask.clone()];
        if self.cfg.use_caption_condition {
            masks.push(cond.caption_mask.clone());
        }
        Ok((kv, Tensor::cat(&masks, 1)?))
    }

    fn timestep_embedding(&self, t: f32) -> Result<Tensor> {
        let half = self.cfg.timestep_embed_dim / 2;
        let ln = 10000f32.ln();
        let mut out = vec![0f32; half * 2];
        for i in 0..half {
            let freq = 1000.0 * (-ln * i as f32 / half as f32).exp();
            let arg = t * freq;
            out[i] = arg.cos();
            out[half + i] = arg.sin();
        }
        Tensor::from_vec(out, (1, 1, half * 2), &self.device)
    }

    /// `forward_with_encoded_conditions`。x_t: (B, S, D)、ctx_mask: (B, Sctx) の 0/1
    pub fn forward(
        &self,
        x_t: &Tensor,
        t: f32,
        ctx: &[ContextKv],
        ctx_mask: &Tensor,
    ) -> Result<Tensor> {
        let mut c = self.timestep_embedding(t)?;
        c = self.cond_module[0].forward(&c)?.silu()?;
        c = self.cond_module[1].forward(&c)?.silu()?;
        let cond = self.cond_module[2].forward(&c)?;

        let (b, s, _) = x_t.dims3()?;
        let rope = Rope::new(self.cfg.model_dim / self.cfg.num_heads, s, &self.device)?;
        let self_mask = Tensor::ones((b, s), candle_core::DType::F32, &self.device)?;
        let bias = mask_bias(&Tensor::cat(&[&self_mask, ctx_mask], 1)?)?;
        let mut x = self.in_proj.forward(x_t)?;
        for (block, kv) in self.blocks.iter().zip(ctx) {
            x = block.forward(&x, &cond, kv, &bias, &rope)?;
        }
        self.out_proj.forward(&self.out_norm.forward(&x)?)
    }
}

fn patch_sequence(seq: &Tensor, mask: &Tensor, patch: usize) -> Result<(Tensor, Tensor)> {
    if patch <= 1 {
        return Ok((seq.clone(), mask.clone()));
    }
    let (b, s, d) = seq.dims3()?;
    let usable = (s / patch) * patch;
    let seq = seq
        .narrow(1, 0, usable)?
        .reshape((b, usable / patch, d * patch))?;
    let mask = mask
        .narrow(1, 0, usable)?
        .reshape((b, usable / patch, patch))?
        .min(D::Minus1)?;
    Ok((seq, mask))
}
