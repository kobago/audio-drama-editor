"""EmbeddingGemma 2 (テキスト) の中間テンソルを dump し、Rust 移植 (crates/eg2) の数値検証に使う。

eg2-playground の仮想環境で実行する (CPU・fp32):

    ~/projects/github.com/kobago/eg2-playground/.venv/bin/python -I tools/eg2_reference/dump_reference.py \
        --out crates/eg2/testdata/reference.safetensors
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import torch
from safetensors.torch import save_file
from sentence_transformers import SentenceTransformer

EG2 = "google/embeddinggemma-2"

# 長さの違う文を 1 バッチにして、パディングとマスクの扱いも確かめる
TEXTS = [
    "task: search result | query: アリスが遅れてきたことを軽く責める",
    "title: アリスの好み・習慣 | text: （ボブ）ボブは待ち合わせに、いつも5分遅れてくる。",
    "title: アリスの思い出 | text: （小学4年の夏・ボブ）夏祭りで、ボブと「来年もその次も、一緒に花火を見よう」と約束した。"
    "それから毎年、二人で花火を見ている。",
]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    torch.manual_seed(0)
    st = SentenceTransformer(
        EG2,
        device="cpu",
        config_kwargs={"vision_config": None, "audio_config": None},
        model_kwargs={"dtype": torch.float32},
    )
    st.prompts = {}
    st.default_prompt_name = None

    lm = next(m for _, m in st.named_modules() if type(m).__name__ == "EmbeddingGemma2TextModel")

    captured: dict[str, torch.Tensor] = {}

    def keep(name):
        def hook(_module, _inputs, output):
            out = output[0] if isinstance(output, tuple) else output
            captured[name] = out.detach().clone()

        return hook

    lm.embed_tokens.register_forward_hook(keep("embeds"))
    lm.ple.register_forward_hook(keep("per_layer_inputs"))
    for i in (0, 5, len(lm.layers) - 1):
        lm.layers[i].register_forward_hook(keep(f"layer{i}"))
    lm.register_forward_hook(lambda _m, _i, o: captured.__setitem__("last_hidden_state", o.last_hidden_state.detach().clone()))

    # encode() は文を長さ順に並べ替えてから通すので、中間テンソルの順序を保つためモデルを直接呼ぶ
    features = st.tokenize(TEXTS)
    with torch.inference_mode():
        check = st.encode(TEXTS, normalize_embeddings=True, convert_to_tensor=True)
        captured.clear()  # encode() の分を捨て、入力の順序どおりの 1 回だけを残す
        embeddings = st(dict(features))["sentence_embedding"]
    assert torch.allclose(embeddings, check, atol=1e-5), "direct forward and encode() disagree"

    out = {
        "input_ids": features["input_ids"].to(torch.int64).contiguous(),
        "attention_mask": features["attention_mask"].to(torch.int64).contiguous(),
        **{k: v.contiguous() for k, v in captured.items()},
        "embeddings": embeddings.contiguous(),
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    save_file(out, str(args.out), metadata={"texts": json.dumps(TEXTS, ensure_ascii=False)})
    for k, v in out.items():
        print(f"{k:20s} {tuple(v.shape)} {v.dtype}")


if __name__ == "__main__":
    main()
