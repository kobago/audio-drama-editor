# audio-drama-editor

ドラマCD・ラジオドラマ (audio drama) 制作用エディター。キャラクターとシーンをテキストで設定し、
セリフを LLM で生成、音声を Irodori-TTS (Rust/candle 移植) で生成する。

## 構成

| パス | 内容 |
|---|---|
| `crates/server` | axum REST API + SQLite (`data/app.db`)、画像・音声は `data/files/` |
| `crates/llm` | OpenAI 互換 chat (OpenCode Go / llama-server) と llama-server `/completion` |
| `crates/irodori` | Irodori-TTS v3 / VoiceDesign の candle 推論。feature `metal` / `cuda` |
| `frontend` | React + Vite + TanStack Query |

## 開発

```sh
cargo run --features metal          # API: http://127.0.0.1:3000 (ADE_ADDR, ADE_DATA_DIR で変更)
cd frontend && npm install && npm run dev   # /api, /files は 3000 番にプロキシ
```

NVIDIA GPU のマシンでは `--features cuda`。起動時に TTS モデルを読み込む (`ADE_TTS=off` で無効化)。

### 音声生成 API

| | |
|---|---|
| `POST /api/tts/jobs` | `{"line_ids": [..], "seed"?: n, "params"?: {num_steps, cfg_scale_*, seconds, duration_scale}}` でセリフごとにジョブを積む |
| `GET /api/tts/events` | ジョブの状態変化 (queued → running → done / failed) を SSE で通知 |
| `GET /api/tts/status` | エンジンの状態 (loading / ready / failed / disabled) と実行中のジョブ |
| `GET /api/lines/{id}/caption` | 生成に使う caption のプレビュー (キャラの声 + 演技メタデータ、または手動上書き) |
| `GET /api/lines/{id}/takes`, `PUT /api/takes/{id}/adopt`, `DELETE /api/takes/{id}` | テイクの一覧・採用・削除。音声は `/files/takes/*.wav` |

### Irodori-TTS (crates/irodori)

重み (`Aratako/Irodori-TTS-600M-v3-VoiceDesign`, `Aratako/Semantic-DACVAE-Japanese-32dim`, `llm-jp/llm-jp-3-150m` の tokenizer) は
Hugging Face のキャッシュから読む。

```sh
# 1 セリフ合成
cargo run -p irodori --release --features metal --example synth -- \
  --text "こんにちは" --caption "明るい女性の声" [--ref-wav ref.wav] --out out.wav

# PyTorch 実装との数値比較 (Irodori-TTS の venv で基準テンソルを作ってから)
~/projects/github.com/irodori-tts/.venv/bin/python -I tools/irodori_reference/dump_reference.py \
  --irodori ~/projects/github.com/irodori-tts --ref-wav <48kHz の wav> \
  --out crates/irodori/testdata/reference.safetensors
cargo test -p irodori --release --features metal -- --nocapture
```

Metal では candle の broadcast 演算・転置コピー・conv_transpose1d が遅いため、`ops.rs` の自前カーネルと
matmul ベースの畳み込みを使う。

## キャラクター設計 (persona-lora の知見)

| 何を | どこに |
|---|---|
| どう話すか (一人称・呼び方・語尾・距離感、長さ別のサンプル台詞) | システムプロンプト (`speech_style`, `sample_lines`) |
| 何を知っているか (1 件 1 事実の短文) | RAG (`character_facts`) |
| 口調の土台 (llama-server のみ、任意) | リクエスト単位の LoRA (`lora_id`, `lora_scale`) |

システムプロンプトは「誰として・誰と・今どういう状況か」に絞る。セリフの長さはプロンプトの指示ではなく、
台詞計画の長さ区分 (S/M/L) と文字数検査で制御する。
