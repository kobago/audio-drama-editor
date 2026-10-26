"""d1-3B で、記憶の候補ごとに「この場面で思い出すか」を判断させる。

decision-llm-comparison の仮想環境で、retrieve.py のあとに実行する:

    ~/projects/github.com/kobago/decision-llm-comparison/.venv/bin/python -I tools/rag_eval/judge.py

方式 (results/judge.json の modes)。候補は EG2 の検索文 3 通りを統合した順位 (fusion) の上位 20 件:
  score    候補ごとに 3 段階で問い、期待値で並べ替える
  choice   候補を選択肢に並べ、いちばん関係するものを選ばせた確率で並べ替える (1 問)
"""

from __future__ import annotations

import json
import statistics
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import torch  # noqa: E402
from transformers import AutoModel  # noqa: E402

import common  # noqa: E402

D1 = "LiquidAI/d1-3B"
TOP_K = 20
QUERY = "fusion"


def main() -> None:
    device = "mps" if torch.backends.mps.is_available() else "cpu"
    t = time.perf_counter()
    model = AutoModel.from_pretrained(D1, trust_remote_code=True, dtype=torch.bfloat16).to(device).eval()
    load_s = time.perf_counter() - t

    mems = {m["id"]: m for m in common.memories()}
    eg2 = {sid: r[QUERY] for sid, r in json.loads((common.RESULTS / "eg2.json").read_text())["rankings"].items()}

    def ask(state: str, ids: list[str], make) -> tuple[list[dict], float]:
        questions = {mid: make(mems[mid]) for mid in ids}
        t = time.perf_counter()
        with torch.inference_mode():
            out = model.system_one(state=state, questions=questions)
        if device == "mps":
            torch.mps.synchronize()
        ms = (time.perf_counter() - t) * 1000
        scored = []
        for mid, a in out["answers"].items():
            value = a["noul"] if a["type"] == "noul" else a["score"] / 2  # score は 0〜2 を 0〜1 に
            scored.append({"id": mid, "score": float(value)})
        scored.sort(key=lambda x: -x["score"])
        return scored, ms

    def ask_choice(state: str, ids: list[str]) -> tuple[list[dict], float]:
        t = time.perf_counter()
        with torch.inference_mode():
            out = model.system_one(state=state, questions={"pick": common.choice_question([mems[i] for i in ids])})
        if device == "mps":
            torch.mps.synchronize()
        probs = out["answers"]["pick"]["probabilities"]
        ranked = sorted(({"id": i, "score": float(probs[i])} for i in ids), key=lambda x: -x["score"])
        return ranked, (time.perf_counter() - t) * 1000

    # 1 回目は慣らし
    first = common.scenes()[0]
    ask_choice(common.judge_state(first), [r["id"] for r in eg2[first["id"]][:TOP_K]])

    modes = {"score": {}, "choice": {}}
    times = {k: [] for k in modes}
    for s in common.scenes():
        state = common.judge_state(s)
        top = [r["id"] for r in eg2[s["id"]][:TOP_K]]
        for mode, run in [
            ("score", lambda: ask(state, top, common.score_question)),
            ("choice", lambda: ask_choice(state, top)),
        ]:
            ranked, ms = run()
            modes[mode][s["id"]] = ranked
            times[mode].append(ms)
        print(s["id"], {k: round(v[-1]) for k, v in times.items()}, "ms", flush=True)

    timing = {"load_s": load_s, **{f"{k}_ms_median": statistics.median(v) for k, v in times.items()}}
    (common.RESULTS / "judge.json").write_text(
        json.dumps({"query": QUERY, "timing": timing, "modes": modes}, ensure_ascii=False, indent=1))
    print(json.dumps(timing, indent=1))


if __name__ == "__main__":
    main()
