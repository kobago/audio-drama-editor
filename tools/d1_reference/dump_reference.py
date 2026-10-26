"""d1-3B (LFM2 テキスト部) の中間テンソルと判断の確率を dump し、Rust 移植 (crates/d1) の数値検証に使う。

decision-llm-comparison の仮想環境で実行する (CPU・fp32):

    ~/projects/github.com/kobago/decision-llm-comparison/.venv/bin/python -I tools/d1_reference/dump_reference.py \
        --out crates/d1/testdata/reference.safetensors
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import torch
from safetensors.torch import save_file
from transformers import AutoModel

sys.path.insert(0, str(Path(__file__).parents[1] / "rag_eval"))
import common  # noqa: E402

D1 = "LiquidAI/d1-3B"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    model = AutoModel.from_pretrained(D1, trust_remote_code=True, dtype=torch.float32).eval()
    engine = model.engine
    # remote code の prompt モジュール (trust_remote_code で読み込まれたもの)
    prompt = sys.modules[type(engine).__module__.rsplit(".", 1)[0] + ".prompt"]

    mems = {m["id"]: m for m in common.memories()}
    scene = common.scenes()[0]
    state = common.judge_state(scene)
    picked = [mems[i] for i in list(mems)[:4]]
    questions = {
        **{m["id"]: common.score_question(m) for m in picked[:3]},
        "pick": common.choice_question(picked),
        "noul": common.noul_question(picked[0]),
    }
    qs = {name: prompt.as_question(q) for name, q in questions.items()}

    lm = model.model.language_model
    out: dict[str, torch.Tensor] = {}
    meta: dict = {"state": state, "questions": questions, "names": list(questions)}

    with torch.inference_mode():
        # 1 問目の全文を素直な鎖として通し、層ごとの出力を残す
        first = next(iter(qs.values()))
        ids = engine.tokenizer.encode(engine.render(state, first), add_special_tokens=False)
        out["ids0"] = torch.tensor(ids, dtype=torch.int64)
        h = lm.embed_tokens(torch.tensor([ids]))
        pos = torch.arange(len(ids))
        for i, layer in enumerate(lm.layers):
            h = layer(h, pos, None)
            if i in (0, 2, 29):
                out[f"layer{i}"] = h[0].contiguous()
        h = lm.embedding_norm(h)
        out["final0"] = h[0].contiguous()
        out["logits0"] = model.lm_head(h[0, -1]).contiguous()

        # 状態を共有した 1 回の判断 (木) の確率
        answers = model.system_one(state=state, questions=questions)["answers"]
        probs = engine.run([(state, list(qs.values()), ())])[0][0]
        for name, p in zip(qs, probs):
            out[f"probs.{name}"] = torch.tensor(p, dtype=torch.float32)
            out[f"readout.{name}"] = torch.tensor(
                [t for g in prompt.readout_ids(engine.tokenizer, qs[name]) for t in g], dtype=torch.int64)
            out[f"rows.{name}"] = torch.tensor(
                engine.tokenizer.encode(engine.render(state, qs[name]), add_special_tokens=False), dtype=torch.int64)
        meta["answers"] = answers

    args.out.parent.mkdir(parents=True, exist_ok=True)
    save_file(out, str(args.out), metadata={"meta": json.dumps(meta, ensure_ascii=False)})
    for k, v in out.items():
        print(f"{k:24s} {tuple(v.shape)}")
    print(json.dumps(answers, ensure_ascii=False, indent=1)[:2000])


if __name__ == "__main__":
    main()
