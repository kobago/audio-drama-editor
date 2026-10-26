//! model.py の共通部品 (Linear, RMSNorm, RoPE, attention, SwiGLU)。

use candle_core::{Device, Module, Result, Tensor};
use candle_nn::VarBuilder;

use crate::ops::{self, Axis};

/// bias の加算を ops::bcast_add で行う Linear (candle_nn の bias 加算は Metal で遅い)。
#[derive(Debug, Clone)]
pub struct Linear {
    inner: candle_nn::Linear,
    bias: Option<Tensor>,
}

impl Module for Linear {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let y = self.inner.forward(x)?;
        match &self.bias {
            Some(b) => ops::bcast_add(&y, b, Axis::Last),
            None => Ok(y),
        }
    }
}

pub fn linear(in_dim: usize, out_dim: usize, bias: bool, vb: VarBuilder) -> Result<Linear> {
    let w = vb.get((out_dim, in_dim), "weight")?;
    let bias = if bias {
        Some(vb.get(out_dim, "bias")?)
    } else {
        None
    };
    Ok(Linear {
        inner: candle_nn::Linear::new(w, None),
        bias,
    })
}

/// 最終軸で正規化する RMSNorm。`weight` が (H, Dh) のときはヘッドごとの重みになる。
#[derive(Debug, Clone)]
pub struct RmsNorm {
    weight: Tensor,
    eps: f32,
}

impl RmsNorm {
    pub fn new<S: Into<candle_core::Shape>>(shape: S, eps: f64, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            weight: vb.get(shape, "weight")?,
            eps: eps as f32,
        })
    }
}

impl Module for RmsNorm {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        if self.weight.rank() == 1 {
            candle_nn::ops::rms_norm(&x.contiguous()?, &self.weight, self.eps)
        } else {
            ops::rms_norm_grouped(x, &self.weight, self.eps)
        }
    }
}

#[derive(Debug, Clone)]
pub struct SwiGlu {
    w1: Linear,
    w2: Linear,
    w3: Linear,
}

impl SwiGlu {
    pub fn new(dim: usize, hidden: usize, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            w1: linear(dim, hidden, false, vb.pp("w1"))?,
            w2: linear(hidden, dim, false, vb.pp("w2"))?,
            w3: linear(dim, hidden, false, vb.pp("w3"))?,
        })
    }
}

impl Module for SwiGlu {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        self.w2
            .forward(&(self.w1.forward(x)?.silu()? * self.w3.forward(x)?)?)
    }
}

/// 複素数表現の RoPE (`precompute_freqs_cis`) を cos/sin で持つ。形状は (S, head_dim / 2)。
#[derive(Debug, Clone)]
pub struct Rope {
    pub cos: Tensor,
    pub sin: Tensor,
}

impl Rope {
    pub fn new(head_dim: usize, seq_len: usize, device: &Device) -> Result<Self> {
        let half = head_dim / 2;
        let freqs: Vec<f32> = (0..half)
            .map(|i| 1.0 / 10000f32.powf((2 * i) as f32 / head_dim as f32))
            .collect();
        let mut cos = Vec::with_capacity(seq_len * half);
        let mut sin = Vec::with_capacity(seq_len * half);
        for t in 0..seq_len {
            for f in &freqs {
                let angle = t as f32 * f;
                cos.push(angle.cos());
                sin.push(angle.sin());
            }
        }
        Ok(Self {
            cos: Tensor::from_vec(cos, (seq_len, half), device)?,
            sin: Tensor::from_vec(sin, (seq_len, half), device)?,
        })
    }

    /// x: (B, H, S, Dh)。隣接する 2 要素を複素数として回す (torch.view_as_complex と同じ並び)。
    pub fn apply(&self, x: &Tensor) -> Result<Tensor> {
        candle_nn::rotary_emb::rope_i(&x.contiguous()?, &self.cos, &self.sin)
    }
}

/// 0/1 の key mask (B, Sk) を加算用の bias (0 / -1e9) にする。
pub fn mask_bias(mask: &Tensor) -> Result<Tensor> {
    (mask - 1.0)? * 1e9
}

/// `F.scaled_dot_product_attention`。q, k, v: (B, H, S, Dh)、bias: (B, Sk) の加算 mask。
///
/// 全キーがマスクされた行の出力は PyTorch (0) と異なるが、そうなるのは no-ref / 空 caption の
/// エンコーダだけで、その出力は直後のマスク乗算で 0 になる。
pub fn attention(q: &Tensor, k: &Tensor, v: &Tensor, bias: &Tensor) -> Result<Tensor> {
    let (b, h, sq, dh) = q.dims4()?;
    let sk = k.dim(2)?;
    let scale = 1.0 / (dh as f32).sqrt();
    let bias = bias.reshape((b, 1, 1, sk))?;
    if q.device().is_metal() {
        let mask = bias.broadcast_as((b, h, sq, sk))?;
        return candle_nn::ops::sdpa(
            &q.contiguous()?,
            &k.contiguous()?,
            &v.contiguous()?,
            Some(&mask),
            false,
            scale,
            1.0,
        );
    }
    let scores = (q.contiguous()?.matmul(&k.contiguous()?.t()?)? * scale as f64)?;
    let probs = candle_nn::ops::softmax_last_dim(&scores.broadcast_add(&bias)?)?;
    probs.matmul(&v.contiguous()?)
}

/// (B, S) の 0/1 マスクで (B, S, D) をゼロにする。
pub fn apply_mask(x: &Tensor, mask: &Tensor) -> Result<Tensor> {
    ops::bcast_mul(x, &mask.flatten_all()?, Axis::Channel)
}
