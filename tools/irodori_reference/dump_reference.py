"""Irodori-TTS (PyTorch) の中間テンソルを dump し、Rust 移植 (crates/irodori) の数値検証に使う。

Irodori-TTS の venv で実行する:

    ~/projects/github.com/irodori-tts/.venv/bin/python -I tools/irodori_reference/dump_reference.py \
        --irodori ~/projects/github.com/irodori-tts \
        --ref-wav path/to/ref.wav \
        --out crates/irodori/testdata/reference.safetensors

CPU / fp32 で計算する。乱数は torch の CPU generator を使うので、初期ノイズも dump して
Rust 側はそれを入力として使う。
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import torch


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--irodori", type=Path, required=True)
    parser.add_argument("--checkpoint", default="Aratako/Irodori-TTS-600M-v3-VoiceDesign")
    parser.add_argument("--ref-wav", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--text", default="「こんにちは！今日は、いい天気ですね…」")
    parser.add_argument("--caption", default="落ち着いた女性の声で、近い距離感でやわらかく読み上げてください。")
    parser.add_argument("--steps", type=int, default=4)
    parser.add_argument("--seed", type=int, default=1234)
    parser.add_argument("--max-ref-seconds", type=float, default=4.0)
    args = parser.parse_args()

    sys.path.insert(0, str(args.irodori))
    from huggingface_hub import hf_hub_download
    from safetensors.torch import save_file

    from irodori_tts.codec import DACVAECodec
    from irodori_tts.config import ModelConfig
    from irodori_tts.inference_runtime import _load_audio, _load_checkpoint_for_inference
    from irodori_tts.model import TextToLatentRFDiT
    from irodori_tts.rf import sample_euler_rf_cfg
    from irodori_tts.text_normalization import normalize_text
    from irodori_tts.tokenizer import PretrainedTextTokenizer

    torch.manual_seed(0)
    ckpt_path = Path(hf_hub_download(args.checkpoint, "model.safetensors"))
    state, cfg_dict, _train_cfg = _load_checkpoint_for_inference(ckpt_path)
    cfg = ModelConfig(**cfg_dict)
    model = TextToLatentRFDiT(cfg)
    model.load_state_dict(state)
    model.eval()

    tok = PretrainedTextTokenizer.from_pretrained(cfg.text_tokenizer_repo, add_bos=cfg.text_add_bos)
    cap_tok = PretrainedTextTokenizer.from_pretrained(
        cfg.caption_tokenizer_repo_resolved, add_bos=cfg.caption_add_bos_resolved
    )
    codec = DACVAECodec.load(device="cpu")

    out: dict[str, torch.Tensor] = {}
    normalized = normalize_text(args.text).strip()

    text_ids = tok.encode(normalized)[None, :]
    text_mask = torch.ones_like(text_ids, dtype=torch.bool)
    caption_ids = cap_tok.encode(args.caption)[None, :]
    caption_mask = torch.ones_like(caption_ids, dtype=torch.bool)
    out["text_ids"] = text_ids.to(torch.int64)
    out["caption_ids"] = caption_ids.to(torch.int64)

    with torch.inference_mode():
        # --- DACVAE encode (reference) ---
        wav, sr = _load_audio(args.ref_wav)
        assert sr == codec.sample_rate, "use a 48 kHz reference so resampling stays out of the comparison"
        wav = wav[:, : int(args.max_ref_seconds * sr)]
        out["ref_wav_raw"] = wav.contiguous()
        normalized_wav = codec._normalize_loudness(wav, sr, -16.0)
        out["ref_wav_normalized"] = normalized_wav.contiguous()
        ref_latent = codec.encode_waveform(wav.unsqueeze(0), sample_rate=sr, normalize_db=-16.0)
        out["ref_latent"] = ref_latent.contiguous()  # (1, T, 32)
        ref_mask = torch.ones(ref_latent.shape[:2], dtype=torch.bool)

        # --- condition encoders ---
        text_state, _, speaker_state, speaker_mask, caption_state, _ = model.encode_conditions(
            text_input_ids=text_ids,
            text_mask=text_mask,
            ref_latent=ref_latent,
            ref_mask=ref_mask,
            caption_input_ids=caption_ids,
            caption_mask=caption_mask,
        )
        out["text_state"] = text_state.contiguous()
        out["speaker_state"] = speaker_state.contiguous()  # (1, 1 + T, 768) 先頭は masked mean
        out["caption_state"] = caption_state.contiguous()

        # no-ref / no-caption の条件 (全マスク) も検証する
        _, _, speaker_state_noref, _, caption_state_empty, _ = model.encode_conditions(
            text_input_ids=text_ids,
            text_mask=text_mask,
            ref_latent=torch.zeros((1, 1, cfg.latent_dim)),
            ref_mask=torch.zeros((1, 1), dtype=torch.bool),
            caption_input_ids=caption_ids,
            caption_mask=torch.zeros_like(caption_mask),
        )
        out["speaker_state_noref"] = speaker_state_noref.contiguous()
        out["caption_state_empty"] = caption_state_empty.contiguous()

        # --- duration ---
        log_frames = model.predict_duration_log_frames(
            text_state=text_state,
            text_mask=text_mask,
            speaker_state=speaker_state,
            speaker_mask=speaker_mask,
            caption_state=caption_state,
            caption_mask=caption_mask,
            duration_features=torch.zeros((1, cfg.duration_aux_dim)),
            has_speaker=torch.ones((1,), dtype=torch.bool),
            has_caption=torch.ones((1,), dtype=torch.bool),
        )
        out["duration_log_frames"] = log_frames.contiguous()
        latent_steps = max(1, int(round(float(torch.expm1(log_frames).mean()))))

        # --- single DiT forward ---
        rng = torch.Generator(device="cpu").manual_seed(args.seed)
        noise = torch.randn((1, latent_steps, cfg.patched_latent_dim), generator=rng)
        out["noise"] = noise.contiguous()
        t0 = torch.full((1,), 0.999)
        out["v_first"] = model.forward_with_encoded_conditions(
            x_t=noise,
            t=t0,
            text_state=text_state,
            text_mask=text_mask,
            speaker_state=speaker_state,
            speaker_mask=speaker_mask,
            caption_state=caption_state,
            caption_mask=caption_mask,
        ).contiguous()

        # --- full sampler (independent CFG) ---
        latent = sample_euler_rf_cfg(
            model=model,
            text_input_ids=text_ids,
            text_mask=text_mask,
            ref_latent=ref_latent,
            ref_mask=ref_mask,
            sequence_length=latent_steps,
            caption_input_ids=caption_ids,
            caption_mask=caption_mask,
            num_steps=args.steps,
            seed=args.seed,
        )
        out["latent"] = latent.contiguous()

        # --- DACVAE decode ---
        out["audio"] = codec.decode_latent(latent).contiguous()  # (1, 1, samples)

    meta = {
        "normalized_text": normalized,
        "text": args.text,
        "caption": args.caption,
        "steps": str(args.steps),
        "seed": str(args.seed),
        "config": json.dumps(cfg_dict, ensure_ascii=False),
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    save_file(out, str(args.out), metadata=meta)
    for k, v in out.items():
        print(f"{k:24s} {tuple(v.shape)} {v.dtype}")
    print("normalized_text:", normalized)


if __name__ == "__main__":
    main()
