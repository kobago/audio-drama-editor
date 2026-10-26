"""EG2 で全記憶を埋め込み、場面ごとに類似度の順位を results/eg2.json に書く。

eg2-playground の仮想環境で実行する:

    ~/projects/github.com/kobago/eg2-playground/.venv/bin/python -I tools/rag_eval/retrieve.py
"""

from __future__ import annotations

import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import numpy as np  # noqa: E402
import torch  # noqa: E402
from sentence_transformers import SentenceTransformer  # noqa: E402

import common  # noqa: E402

EG2 = "google/embeddinggemma-2"


def main() -> None:
    t = time.perf_counter()
    model = SentenceTransformer(
        EG2,
        device="mps" if torch.backends.mps.is_available() else "cpu",
        config_kwargs={"vision_config": None, "audio_config": None},
        model_kwargs={"dtype": torch.float32},
    )
    # プレフィックスは common で付けるので、モデル側の既定プロンプトは使わない
    model.prompts = {}
    model.default_prompt_name = None
    load_s = time.perf_counter() - t

    mems, scs = common.memories(), common.scenes()
    t = time.perf_counter()
    doc = model.encode([common.document_prompt(m) for m in mems], normalize_embeddings=True, batch_size=16)
    index_s = time.perf_counter() - t

    # 検索文の作り方ごとの順位と、3 つの順位を RRF (k=60) で統合したもの
    out: dict[str, dict] = {}
    query_ms = []
    for s in scs:
        texts = common.query_texts(s)
        t = time.perf_counter()
        qs = model.encode([common.query_prompt(x) for x in texts.values()], normalize_embeddings=True)
        query_ms.append((time.perf_counter() - t) * 1000)
        per = {}
        fused = np.zeros(len(mems))
        for name, q in zip(texts, qs):
            sims = doc @ q
            order = np.argsort(-sims)
            per[name] = [{"id": mems[i]["id"], "score": float(sims[i])} for i in order]
            for rank, i in enumerate(order):
                fused[i] += 1 / (60 + rank + 1)
        order = np.argsort(-fused)
        per["fusion"] = [{"id": mems[i]["id"], "score": float(fused[i])} for i in order]
        out[s["id"]] = per

    common.RESULTS.mkdir(exist_ok=True)
    (common.RESULTS / "eg2.json").write_text(json.dumps(
        {"timing": {"load_s": load_s, "index_s": index_s, "query_ms_median": float(np.median(query_ms))},
         "rankings": out},
        ensure_ascii=False, indent=1))
    print(f"load {load_s:.1f}s, index {len(mems)} memories {index_s:.2f}s, query median {np.median(query_ms):.0f}ms")


if __name__ == "__main__":
    main()
