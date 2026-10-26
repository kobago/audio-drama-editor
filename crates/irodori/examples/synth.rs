//! 1 セリフを合成して WAV に書き出す。
//!
//!     cargo run -p irodori --release --features metal --example synth -- \
//!         --text "こんにちは" --caption "明るい女性の声" [--ref-wav ref.wav] --out out.wav

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Context;
use irodori::{Irodori, Paths, Request, Speaker, audio};

fn main() -> anyhow::Result<()> {
    let mut req = Request::default();
    let mut ref_wav: Option<PathBuf> = None;
    let mut out = PathBuf::from("out.wav");
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .with_context(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--text" => req.text = value,
            "--caption" => req.caption = value,
            "--ref-wav" => ref_wav = Some(value.into()),
            "--out" => out = value.into(),
            "--seed" => req.seed = value.parse()?,
            "--steps" => req.sampler.num_steps = value.parse()?,
            "--seconds" => req.seconds = Some(value.parse()?),
            other => anyhow::bail!("unknown flag {other}"),
        }
    }

    let device = irodori::select_device()?;
    let t = Instant::now();
    let paths = Paths::from_hf_cache(irodori::DEFAULT_CHECKPOINT, irodori::DEFAULT_CODEC)?;
    let rt = Irodori::load(&paths, &device)?;
    eprintln!("load: {:.2?} ({device:?})", t.elapsed());

    let latent = match &ref_wav {
        Some(path) => {
            let t = Instant::now();
            let (wav, sr) = audio::read_wav(path)?;
            let latent = rt.encode_reference(&wav, sr, 30.0)?;
            eprintln!("encode reference: {:.2?}", t.elapsed());
            Some(latent)
        }
        None => None,
    };
    let speaker = latent.as_ref().map_or(Speaker::None, Speaker::Latent);

    for i in 0..2 {
        let t = Instant::now();
        let samples = rt.synthesize(&req, speaker_ref(&speaker))?;
        let secs = samples.len() as f32 / irodori::SAMPLE_RATE as f32;
        eprintln!(
            "synthesize #{i}: {:.2?} for {secs:.2}s of audio",
            t.elapsed()
        );
        if i == 0 {
            audio::write_wav(&out, &samples, irodori::SAMPLE_RATE)?;
        }
    }
    eprintln!("wrote {}", out.display());
    Ok(())
}

fn speaker_ref<'a>(s: &'a Speaker<'a>) -> Speaker<'a> {
    match s {
        Speaker::None => Speaker::None,
        Speaker::Latent(l) => Speaker::Latent(l),
    }
}
