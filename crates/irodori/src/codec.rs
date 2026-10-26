//! Semantic-DACVAE (dacvae パッケージ) の encoder / decoder。
//!
//! Irodori が使う経路だけを移植する:
//! - encode: `encoder(_pad(wav))` → `quantizer.in_proj` の mean (deterministic encode)
//! - decode: `quantizer.out_proj` → decoder。透かし分岐は使わず、最後は
//!   `wm_model.encoder_block.forward_no_conv` (Snake → Conv → Tanh)。

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use candle_core::{Device, Module, Result, Tensor};

use crate::ops::{self, Axis};

pub const SAMPLE_RATE: u32 = 48_000;

struct Weights {
    map: HashMap<String, Tensor>,
    device: Device,
}

impl Weights {
    fn load(path: &Path, device: &Device) -> anyhow::Result<Self> {
        let map = candle_core::pickle::read_all_with_key(path, Some("state_dict"))
            .with_context(|| format!("load {}", path.display()))?
            .into_iter()
            .collect();
        Ok(Self { map, device: device.clone() })
    }

    fn get(&self, name: &str) -> Result<Tensor> {
        self.map
            .get(name)
            .ok_or_else(|| candle_core::Error::Msg(format!("missing tensor {name}")))?
            .to_device(&self.device)
    }

    fn has(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    /// `weight_norm` (dim=0) を畳み込んだ重み。g * v / ||v|| (0 次元以外でノルム)。
    fn conv_weight(&self, prefix: &str) -> Result<Tensor> {
        let plain = format!("{prefix}.weight");
        if self.has(&plain) {
            return self.get(&plain);
        }
        let g = self.get(&format!("{prefix}.weight_g"))?;
        let v = self.get(&format!("{prefix}.weight_v"))?;
        let norm = v.sqr()?.sum_keepdim(2)?.sum_keepdim(1)?.sqrt()?;
        v.broadcast_mul(&g.broadcast_div(&norm)?)
    }
}

struct Conv1d {
    weight: Tensor,
    bias: Tensor,
    padding: usize,
    stride: usize,
    dilation: usize,
}

impl Conv1d {
    /// pad_mode="none" の NormConv1d: padding = (kernel - stride) * dilation / 2
    fn load(w: &Weights, prefix: &str, stride: usize, dilation: usize) -> Result<Self> {
        let weight = w.conv_weight(prefix)?;
        let kernel = weight.dim(2)?;
        Ok(Self {
            bias: w.get(&format!("{prefix}.bias"))?,
            padding: (kernel - stride) * dilation / 2,
            weight,
            stride,
            dilation,
        })
    }
}

impl Module for Conv1d {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let (c_out, c_in, kernel) = self.weight.dims3()?;
        let y = if kernel == 1 && self.stride == 1 {
            self.weight.reshape((c_out, c_in))?.broadcast_matmul(x)?
        } else if self.stride == 1 {
            // 入力をずらしながら kernel 回の matmul を足し合わせる (Metal の im2col より速い)
            let len = x.dim(2)? + 2 * self.padding - self.dilation * (kernel - 1);
            let xp = x.pad_with_zeros(2, self.padding, self.padding)?;
            let mut acc: Option<Tensor> = None;
            for j in 0..kernel {
                let wj = self.weight.narrow(2, j, 1)?.squeeze(2)?;
                let y = wj.broadcast_matmul(&xp.narrow(2, j * self.dilation, len)?.contiguous()?)?;
                acc = Some(match acc {
                    Some(a) => (a + y)?,
                    None => y,
                });
            }
            acc.expect("kernel > 0")
        } else {
            x.conv1d(&self.weight, self.padding, self.stride, self.dilation, 1)?
        };
        ops::bcast_add(&y, &self.bias, Axis::Channel)
    }
}

struct ConvTranspose1d {
    weight: Tensor,
    bias: Tensor,
    stride: usize,
}

impl ConvTranspose1d {
    fn load(w: &Weights, prefix: &str) -> Result<Self> {
        let weight = w.conv_weight(prefix)?;
        Ok(Self { stride: weight.dim(2)? / 2, bias: w.get(&format!("{prefix}.bias"))?, weight })
    }
}

impl Module for ConvTranspose1d {
    /// pad_mode="none": padding = (stride + 1) / 2、output_padding = stride % 2
    ///
    /// kernel = 2 * stride なので、matmul で列を作り、前半を位置 l*s、後半を (l+1)*s に
    /// 重ね合わせる (candle の Metal 版 conv_transpose1d は非常に遅い)。
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let s = self.stride;
        let (padding, output_padding) = (s.div_ceil(2), s % 2);
        let (b, c_in, len) = x.dims3()?;
        let (_, c_out, kernel) = self.weight.dims3()?;
        if kernel != 2 * s {
            let y = x.conv_transpose1d(&self.weight, padding, output_padding, s, 1, 1)?;
            return ops::bcast_add(&y, &self.bias, Axis::Channel);
        }
        let cols = self.weight.reshape((c_in, c_out * kernel))?.t()?.broadcast_matmul(x)?;
        let cols = cols.reshape((b, c_out, 2, s, len))?;
        let half = |i: usize| -> Result<Tensor> {
            cols.narrow(2, i, 1)?.squeeze(2)?.transpose(2, 3)?.reshape((b, c_out, len * s))
        };
        let full = (half(0)?.pad_with_zeros(2, 0, s)? + half(1)?.pad_with_zeros(2, s, 0)?)?;
        let out_len = (len - 1) * s + kernel - 2 * padding + output_padding;
        ops::bcast_add(&full.narrow(2, padding, out_len)?, &self.bias, Axis::Channel)
    }
}

struct Snake {
    alpha: Tensor,
}

impl Snake {
    fn load(w: &Weights, prefix: &str) -> Result<Self> {
        Ok(Self { alpha: w.get(&format!("{prefix}.alpha"))?.flatten_all()? })
    }
}

impl Module for Snake {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        ops::snake(x, &self.alpha)
    }
}

/// Snake → Conv(k7, dilation) → Snake → Conv(k1)、残差あり。
struct ResidualUnit {
    snake1: Snake,
    conv1: Conv1d,
    snake2: Snake,
    conv2: Conv1d,
}

impl ResidualUnit {
    fn load(w: &Weights, prefix: &str, dilation: usize) -> Result<Self> {
        Ok(Self {
            snake1: Snake::load(w, &format!("{prefix}.block.0"))?,
            conv1: Conv1d::load(w, &format!("{prefix}.block.1"), 1, dilation)?,
            snake2: Snake::load(w, &format!("{prefix}.block.2"))?,
            conv2: Conv1d::load(w, &format!("{prefix}.block.3"), 1, 1)?,
        })
    }
}

impl Module for ResidualUnit {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let y = self.conv1.forward(&self.snake1.forward(x)?)?;
        let y = self.conv2.forward(&self.snake2.forward(&y)?)?;
        y + x
    }
}

struct Seq(Vec<Box<dyn Module + Send + Sync>>);

impl Module for Seq {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        self.0.iter().try_fold(x.clone(), |x, m| m.forward(&x))
    }
}

pub struct DacVae {
    encoder: Seq,
    in_proj: Conv1d,
    out_proj: Conv1d,
    decoder: Seq,
    latent_dim: usize,
    hop_length: usize,
    device: Device,
}

impl DacVae {
    pub fn load(path: &Path, device: &Device) -> anyhow::Result<Self> {
        let w = Weights::load(path, device)?;

        let mut enc: Vec<Box<dyn Module + Send + Sync>> = vec![Box::new(Conv1d::load(&w, "encoder.block.0", 1, 1)?)];
        let mut hop_length = 1;
        let mut i = 1;
        while w.has(&format!("encoder.block.{i}.block.4.bias")) {
            let p = format!("encoder.block.{i}.block");
            let stride = w.conv_weight(&format!("{p}.4"))?.dim(2)? / 2;
            hop_length *= stride;
            for (j, d) in [(0, 1), (1, 3), (2, 9)] {
                enc.push(Box::new(ResidualUnit::load(&w, &format!("{p}.{j}"), d)?));
            }
            enc.push(Box::new(Snake::load(&w, &format!("{p}.3"))?));
            enc.push(Box::new(Conv1d::load(&w, &format!("{p}.4"), stride, 1)?));
            i += 1;
        }
        enc.push(Box::new(Snake::load(&w, &format!("encoder.block.{i}"))?));
        enc.push(Box::new(Conv1d::load(&w, &format!("encoder.block.{}", i + 1), 1, 1)?));

        let mut dec: Vec<Box<dyn Module + Send + Sync>> = vec![Box::new(Conv1d::load(&w, "decoder.model.0", 1, 1)?)];
        let mut i = 1;
        while w.has(&format!("decoder.model.{i}.block.0.alpha")) {
            // DecoderBlock.forward は偶数番目のチャンク (Snake, ConvT, RU d1, RU d3, RU d9) だけを通る
            let p = format!("decoder.model.{i}.block");
            dec.push(Box::new(Snake::load(&w, &format!("{p}.0"))?));
            dec.push(Box::new(ConvTranspose1d::load(&w, &format!("{p}.1"))?));
            for (j, d) in [(4, 1), (5, 3), (8, 9)] {
                dec.push(Box::new(ResidualUnit::load(&w, &format!("{p}.{j}"), d)?));
            }
            i += 1;
        }
        dec.push(Box::new(Snake::load(&w, "decoder.wm_model.encoder_block.pre.0")?));
        dec.push(Box::new(Conv1d::load(&w, "decoder.wm_model.encoder_block.pre.1", 1, 1)?));

        let out_proj = Conv1d::load(&w, "quantizer.out_proj", 1, 1)?;
        Ok(Self {
            encoder: Seq(enc),
            in_proj: Conv1d::load(&w, "quantizer.in_proj", 1, 1)?,
            latent_dim: out_proj.weight.dim(1)?,
            out_proj,
            decoder: Seq(dec),
            hop_length,
            device: device.clone(),
        })
    }

    pub fn latent_dim(&self) -> usize {
        self.latent_dim
    }

    pub fn hop_length(&self) -> usize {
        self.hop_length
    }

    /// 48 kHz モノラル波形 → (1, T, latent_dim) の latent (VAE の平均)。
    pub fn encode(&self, wav: &[f32]) -> Result<Tensor> {
        let padded = reflect_pad_to_multiple(wav, self.hop_length);
        let x = Tensor::from_vec(padded, (1, 1, ()), &self.device)?;
        let z = self.in_proj.forward(&self.encoder.forward(&x)?)?;
        z.narrow(1, 0, self.latent_dim)?.transpose(1, 2)?.contiguous()
    }

    /// (B, T, latent_dim) → (B, samples) の波形。
    pub fn decode(&self, latent: &Tensor) -> Result<Tensor> {
        let z = latent.transpose(1, 2)?.contiguous()?.to_device(&self.device)?;
        let y = self.decoder.forward(&self.out_proj.forward(&z)?)?;
        y.tanh()?.squeeze(1)
    }
}

fn reflect_pad_to_multiple(wav: &[f32], hop: usize) -> Vec<f32> {
    let rem = wav.len() % hop;
    let mut out = wav.to_vec();
    if rem != 0 {
        let n = wav.len();
        for i in 0..hop - rem {
            out.push(wav[n - 2 - i]);
        }
    }
    out
}
