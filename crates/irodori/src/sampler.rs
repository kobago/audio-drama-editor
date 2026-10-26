//! `irodori_tts.rf.sample_euler_rf_cfg` (cfg_guidance_mode = "independent") の移植。

use candle_core::{Result, Tensor};

use crate::model::{Conditions, TextToLatent};

#[derive(Debug, Clone)]
pub struct SamplerParams {
    pub num_steps: usize,
    pub cfg_scale_text: f32,
    pub cfg_scale_caption: f32,
    pub cfg_scale_speaker: f32,
    pub cfg_min_t: f32,
    pub cfg_max_t: f32,
    pub truncation_factor: Option<f32>,
}

impl Default for SamplerParams {
    fn default() -> Self {
        Self {
            num_steps: 40,
            cfg_scale_text: 3.0,
            cfg_scale_caption: 3.0,
            cfg_scale_speaker: 5.0,
            cfg_min_t: 0.5,
            cfg_max_t: 1.0,
            truncation_factor: None,
        }
    }
}

/// t のスケジュール (init_scale = 0.999 の linear)。torch と同じく f32 で計算する。
fn t_schedule(num_steps: usize) -> Vec<f32> {
    (0..=num_steps)
        .map(|i| {
            // torch.linspace(0, 1, n + 1)
            let u = if num_steps == 0 {
                0.0
            } else {
                i as f32 / num_steps as f32
            };
            (1.0 - u) * 0.999
        })
        .collect()
}

/// `noise`: (1, S, D) の初期ノイズ。戻り値は (1, S, D) の latent。
pub fn sample(
    model: &TextToLatent,
    cond: &Conditions,
    noise: &Tensor,
    p: &SamplerParams,
) -> Result<Tensor> {
    let mut x = match p.truncation_factor {
        Some(f) => (noise * f as f64)?,
        None => noise.clone(),
    };

    // 有効な CFG 項目ごとに、その条件だけを落とした bundle を並べる
    let mut bundles = vec![cond.clone()];
    let mut scales = Vec::new();
    if p.cfg_scale_text > 0.0 {
        bundles.push(cond.drop(true, false, false)?);
        scales.push(p.cfg_scale_text);
    }
    if p.cfg_scale_speaker > 0.0 && cond.has_speaker {
        bundles.push(cond.drop(false, true, false)?);
        scales.push(p.cfg_scale_speaker);
    }
    if p.cfg_scale_caption > 0.0 && cond.has_caption {
        bundles.push(cond.drop(false, false, true)?);
        scales.push(p.cfg_scale_caption);
    }
    let n = bundles.len();
    let (ctx_cfg, mask_cfg) = model.build_context(&Conditions::cat(&bundles)?)?;
    let (ctx_cond, mask_cond) = model.build_context(cond)?;

    let schedule = t_schedule(p.num_steps);
    for i in 0..p.num_steps {
        let (t, t_next) = (schedule[i], schedule[i + 1]);
        let use_cfg = n > 1 && p.cfg_min_t <= t && t <= p.cfg_max_t;
        let v = if use_cfg {
            let xs = Tensor::cat(&vec![&x; n], 0)?;
            let out = model.forward(&xs, t, &ctx_cfg, &mask_cfg)?;
            let base = out.narrow(0, 0, 1)?;
            let mut v = base.clone();
            for (j, scale) in scales.iter().enumerate() {
                let diff = (&base - out.narrow(0, j + 1, 1)?)?;
                v = (v + (diff * *scale as f64)?)?;
            }
            v
        } else {
            model.forward(&x, t, &ctx_cond, &mask_cond)?
        };
        x = (x + (v * (t_next - t) as f64)?)?;
    }
    Ok(x)
}
