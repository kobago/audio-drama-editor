"""検索文・文書・判断モデルの入力の組み立てと、データの読み込み。

検索 (EG2) と判断 (d1-3B) は別の仮想環境で動かすので、どちらからも import できるよう標準ライブラリだけを使う。
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).parent
DATA = ROOT / "data"
RESULTS = ROOT / "results"
CHARACTER = "アリス"


def load_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def memories() -> list[dict]:
    return load_jsonl(DATA / "memories.jsonl")


def scenes() -> list[dict]:
    return load_jsonl(DATA / "scenes.jsonl")


def memory_text(m: dict) -> str:
    """記憶 1 件を、時期と関わる人物付きの 1 文にする。"""
    head = "・".join(x for x in [m["when"], "、".join(m["people"])] if x)
    return f"（{head}）{m['text']}" if head else m["text"]


def scene_text(s: dict) -> str:
    """場面・直前のやりとり・これから話す意図。検索文と判断モデルの状態の両方に使う。"""
    parts = [f"場面: {s['setting']}"]
    if s["lines"]:
        parts.append("直前のやりとり:\n" + "\n".join(f"{who}「{text}」" for who, text in s["lines"]))
    parts.append(f"{CHARACTER}が次に話すこと: {s['intent']}")
    return "\n".join(parts)


# --- EG2 ---


def query_texts(s: dict) -> dict[str, str]:
    """検索文の作り方の候補。full: 場面すべて / focus: 直前のセリフと意図だけ / intent: 意図だけ。"""
    last = "".join(f"{who}「{text}」" for who, text in s["lines"][-1:])
    return {
        "full": scene_text(s),
        "focus": f"{last}{CHARACTER}が次に話すこと: {s['intent']}" if last else f"{CHARACTER}が次に話すこと: {s['intent']}",
        "intent": f"{CHARACTER}が{s['intent']}",
    }


def query_prompt(text: str) -> str:
    return f"task: search result | query: {text}"


def document_prompt(m: dict) -> str:
    return f"title: {CHARACTER}の{m['kind']} | text: {memory_text(m)}"


# --- d1-3B ---


def judge_state(s: dict) -> str:
    return f"登場人物: {CHARACTER}（高校2年生）\n{scene_text(s)}"


def noul_question(m: dict) -> dict:
    return {
        "type": "noul",
        "instructions": (
            f"{CHARACTER}がこの場面で次のセリフを話すとき、次の記憶を思い出したり踏まえたりするのは自然か: "
            f"「{memory_text(m)}」"
        ),
    }


def score_question(m: dict) -> dict:
    return {
        "type": "score",
        "instructions": f"次の記憶は、この場面での{CHARACTER}の次のセリフにどのくらい関係するか: 「{memory_text(m)}」",
        "criteria": [
            "関係ない: この場面では思い出さない",
            "少し関係する: 背景として頭にある",
            "強く関係する: セリフの中身や言い方に直接表れる",
        ],
    }


def choice_question(ms: list[dict]) -> dict:
    """候補を並べて、いちばん関係するものを 1 つ選ばせる (確率が候補の順位になる)。"""
    criteria = {m["id"]: memory_text(m) for m in ms}
    criteria["none"] = "どれも関係しない"
    return {
        "type": "choice",
        "instructions": f"{CHARACTER}がこの場面で次のセリフを話すとき、いちばん強く思い出したり踏まえたりする記憶はどれか",
        "criteria": criteria,
    }
