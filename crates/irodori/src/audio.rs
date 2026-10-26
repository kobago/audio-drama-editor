//! 波形まわりの前後処理 (ラウドネス正規化、末尾トリム、リサンプル、WAV 入出力)。

use std::path::Path;

use anyhow::Context;
use candle_core::{Result, Tensor};

/// audiotools の `AudioSignal.normalize(target_db)` + `ensure_max_of_audio()` 相当。
/// ラウドネスは ITU-R BS.1770 の integrated loudness (EBU R128)。
pub fn normalize_loudness(
    wav: &[f32],
    sample_rate: u32,
    target_db: f64,
) -> anyhow::Result<Vec<f32>> {
    let mut meter = ebur128::EbuR128::new(1, sample_rate, ebur128::Mode::I)?;
    meter.add_frames_f32(wav)?;
    let loudness = meter.loudness_global()?.max(-70.0);
    let gain = 10f64.powf((target_db - loudness) / 20.0) as f32;
    let mut out: Vec<f32> = wav.iter().map(|x| x * gain).collect();
    let peak = out.iter().fold(0f32, |m, x| m.max(x.abs()));
    if peak > 1.0 {
        out.iter_mut().for_each(|x| *x /= peak);
    }
    Ok(out)
}

/// Echo 方式の末尾検出: 窓内がほぼ平坦かつ 0 付近になる最初の位置。latent: (T, D)
pub fn find_flattening_point(
    latent: &Tensor,
    window: usize,
    std_threshold: f32,
    mean_threshold: f32,
) -> Result<usize> {
    let rows = latent.to_vec2::<f32>()?;
    let total = rows.len();
    if total == 0 || window == 0 {
        return Ok(total);
    }
    let dim = rows[0].len();
    let zero = vec![0f32; dim];
    let row = |i: usize| if i < total { &rows[i] } else { &zero };
    for i in 0..total {
        let n = (window * dim) as f64;
        let (mut sum, mut sq) = (0f64, 0f64);
        for j in i..i + window {
            for &v in row(j) {
                sum += v as f64;
                sq += (v as f64) * (v as f64);
            }
        }
        let mean = sum / n;
        let std = (sq / n - mean * mean).max(0.0).sqrt();
        if (std as f32) < std_threshold && ((mean as f32) - 0.0).abs() < mean_threshold {
            return Ok(i);
        }
    }
    Ok(total)
}

pub fn resample(wav: &[f32], from: u32, to: u32) -> anyhow::Result<Vec<f32>> {
    use rubato::{FftFixedIn, Resampler};
    if from == to {
        return Ok(wav.to_vec());
    }
    let chunk = 1024;
    let mut r = FftFixedIn::<f32>::new(from as usize, to as usize, chunk, 2, 1)?;
    let delay = r.output_delay();
    let expected = (wav.len() as u64 * to as u64 / from as u64) as usize;
    let mut out = Vec::with_capacity(expected + delay + chunk);
    let mut pos = 0;
    while out.len() < expected + delay {
        let need = r.input_frames_next();
        let mut buf = vec![0f32; need];
        let n = need.min(wav.len().saturating_sub(pos));
        buf[..n].copy_from_slice(&wav[pos..pos + n]);
        pos += n;
        out.extend_from_slice(&r.process(&[buf], None)?[0]);
    }
    Ok(out[delay..delay + expected].to_vec())
}

/// WAV を読み、モノラル化して `(samples, sample_rate)` を返す。
pub fn read_wav(path: &Path) -> anyhow::Result<(Vec<f32>, u32)> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("open {}", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<std::result::Result<_, _>>()?
        }
    };
    let mono = samples
        .chunks(channels)
        .map(|c| c.iter().sum::<f32>() / channels as f32)
        .collect();
    Ok((mono, spec.sample_rate))
}

pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}
