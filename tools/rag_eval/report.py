"""results/eg2.json と results/judge.json から指標を出す。どの仮想環境でも動く (標準ライブラリだけ)。

    python3 tools/rag_eval/report.py
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import common  # noqa: E402


def metrics(ranked: list[str], gold: set[str], ok: set[str]) -> dict[str, float]:
    def recall(k: int) -> float:
        return len(gold & set(ranked[:k])) / len(gold)

    gain = {**{m: 1.0 for m in ok}, **{m: 2.0 for m in gold}}
    dcg = sum(gain.get(m, 0) / math.log2(i + 2) for i, m in enumerate(ranked[:5]))
    ideal = sorted(gain.values(), reverse=True)[:5]
    idcg = sum(g / math.log2(i + 2) for i, g in enumerate(ideal))
    return {
        "R@3": recall(3),
        "R@5": recall(5),
        "all@3": float(gold <= set(ranked[:3])),
        "P@3": len((gold | ok) & set(ranked[:3])) / 3,
        "nDCG@5": dcg / idcg,
    }


def average(rows: list[dict[str, float]]) -> dict[str, float]:
    return {k: sum(r[k] for r in rows) / len(rows) for k in rows[0]}


def main() -> None:
    eg2 = json.loads((common.RESULTS / "eg2.json").read_text())
    judge = json.loads((common.RESULTS / "judge.json").read_text())
    scs = common.scenes()

    print("EG2 の検索文ごとの 必須の再現率 (@3 / @20)")
    for q in ["full", "focus", "intent", "fusion"]:
        r3 = [len(set(s["gold"]) & {r["id"] for r in eg2["rankings"][s["id"]][q][:3]}) / len(s["gold"]) for s in scs]
        r20 = [len(set(s["gold"]) & {r["id"] for r in eg2["rankings"][s["id"]][q][:20]}) / len(s["gold"]) for s in scs]
        print(f"  {q:7} {sum(r3) / len(r3):.3f} / {sum(r20) / len(r20):.3f}")
    print()
    base = judge.get("query", "full")
    systems: dict[str, dict[str, list[str]]] = {
        "EG2 のみ": {sid: [r["id"] for r in rs[base]] for sid, rs in eg2["rankings"].items()},
    }
    for mode, ranked in judge["modes"].items():
        systems[f"→ d1 {mode}"] = {sid: [r["id"] for r in rs] for sid, rs in ranked.items()}
    # 3 段階 (0〜1) と選択の確率を足して並べ替える
    if {"score", "choice"} <= set(judge["modes"]):
        combined = {}
        for sid in judge["modes"]["score"]:
            total: dict[str, float] = {}
            for mode in ("score", "choice"):
                for r in judge["modes"][mode][sid]:
                    total[r["id"]] = total.get(r["id"], 0.0) + r["score"]
            combined[sid] = sorted(total, key=lambda m: -total[m])
        systems["→ d1 score+choice"] = combined

    # EG2 の候補 20 件に正解が入っているか (2 段目の上限)
    cover = [len(set(s["gold"]) & set(systems["EG2 のみ"][s["id"]][:20])) / len(s["gold"]) for s in scs]
    print(f"EG2 上位20件の必須の再現率: {sum(cover) / len(cover):.3f}\n")

    keys = ["R@3", "R@5", "all@3", "P@3", "nDCG@5"]
    print("| 方式 | 必須の再現率@3 | @5 | 全必須@3 | 適合率@3 | nDCG@5 |")
    print("|---|---|---|---|---|---|")
    per_type: dict[str, dict[str, float]] = {}
    for name, ranked in systems.items():
        rows = [metrics(ranked[s["id"]], set(s["gold"]), set(s["ok"])) for s in scs]
        avg = average(rows)
        print(f"| {name} | " + " | ".join(f"{avg[k]:.3f}" for k in keys) + " |")
        for t in sorted({s["type"] for s in scs}):
            sub = [r for r, s in zip(rows, scs) if s["type"] == t]
            per_type.setdefault(t, {})[name] = average(sub)["R@3"]

    print("\n場面の種類ごとの 必須の再現率@3")
    names = list(systems)
    print("| 種類 | 件数 | " + " | ".join(names) + " |")
    print("|---|---|" + "---|" * len(names))
    for t, vals in per_type.items():
        n = sum(1 for s in scs if s["type"] == t)
        print(f"| {t} | {n} | " + " | ".join(f"{vals[k]:.2f}" for k in names) + " |")

    print("\n場面ごとの上位3件 (★必須 ○構わない)")
    mems = {m["id"]: m for m in common.memories()}
    for s in scs:
        print(f"\n{s['id']} [{s['type']}] {s['intent']}  必須: {', '.join(s['gold'])}")
        for name in ["EG2 のみ", list(systems)[-1]]:
            top = systems[name][s["id"]][:3]
            mark = lambda m: "★" if m in s["gold"] else ("○" if m in s["ok"] else "  ")  # noqa: E731
            print(f"  {name:24} " + " / ".join(f"{mark(m)}{m} {mems[m]['text'][:14]}" for m in top))

    print(f"\n時間: EG2 検索 {eg2['timing']['query_ms_median']:.0f}ms/場面 (検索文 4 通り), d1 "
          + ", ".join(f"{k} {v:.0f}" for k, v in judge["timing"].items()))


if __name__ == "__main__":
    main()
