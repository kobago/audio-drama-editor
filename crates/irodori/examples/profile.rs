//! 段階ごとの所要時間を測る。
use std::time::Instant;

use irodori::{Irodori, Paths, SamplerParams, audio, gaussian_noise, sampler, text};

fn main() -> anyhow::Result<()> {
    let device = irodori::select_device()?;
    let paths = Paths::from_hf_cache(irodori::DEFAULT_CHECKPOINT, irodori::DEFAULT_CODEC)?;
    let rt = Irodori::load(&paths, &device)?;
    let tok = text::TextTokenizer::from_file(&paths.text_tokenizer, true)?;
    let ids = tok.encode(
        &text::normalize_text("遅いよ、もう。ずっと待ってたんだから…でも、来てくれてありがとう。"),
        256,
    )?;
    let cap = tok.encode("少し拗ねたような若い女性の声。", 512)?;
    let (wav, sr) = audio::read_wav(std::path::Path::new(&std::env::args().nth(1).unwrap()))?;
    let latent = rt.encode_reference(&wav, sr, 30.0)?;
    let steps = 210;

    for _ in 0..2 {
        let t = Instant::now();
        let cond = rt
            .model
            .encode_conditions(&ids, Some(&latent), &cap, true)?;
        device.synchronize()?;
        eprintln!("encode_conditions {:.2?}", t.elapsed());

        let noise = gaussian_noise(0, steps, 32, &device)?;
        let (ctx, mask) = rt.model.build_context(&irodori::model::Conditions::cat(&[
            cond.clone(),
            cond.drop(true, false, false)?,
            cond.drop(false, true, false)?,
            cond.drop(false, false, true)?,
        ])?)?;
        let x4 = candle_core::Tensor::cat(&[&noise, &noise, &noise, &noise], 0)?;
        device.synchronize()?;
        let t = Instant::now();
        for _ in 0..5 {
            rt.model.forward(&x4, 0.9, &ctx, &mask)?;
        }
        device.synchronize()?;
        eprintln!("DiT forward B=4: {:.2?}/step", t.elapsed() / 5);

        let t = Instant::now();
        let z = sampler::sample(&rt.model, &cond, &noise, &SamplerParams::default())?;
        device.synchronize()?;
        eprintln!("sampler 40 steps: {:.2?}", t.elapsed());

        let t = Instant::now();
        let a = rt.codec.decode(&z)?;
        device.synchronize()?;
        eprintln!("decode: {:.2?} ({:?})", t.elapsed(), a.dims());
    }
    Ok(())
}
