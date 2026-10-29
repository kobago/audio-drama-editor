<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/brand/logo-dark.svg">
  <img src="docs/brand/logo-light.svg" alt="Audio Drama Editor" height="48">
</picture>

# audio-drama-editor

ドラマCD・ラジオドラマ (audio drama) 制作用エディター。キャラクターとシーンをテキストで設定し、
セリフを LLM で生成、音声を Irodori-TTS (Rust/candle 移植) で生成する。

## Mac mini / Linux サーバーで常駐させる

Rust (rustup) と Bun を入れ、Irodori-TTS の重みを Hugging Face のキャッシュに落としてから、リポジトリで次を実行する。

```sh
make install     # 画面をビルドしてバイナリに埋め込み、~/.cargo/bin/audio-drama-editor に入れる
make service     # macOS は launchd、Linux は systemd のユーザーサービスとして登録・起動 (0.0.0.0:3333)
```

GPU は自動で選ぶ (macOS は `metal`、`nvidia-smi` のある Linux は `cuda`)。`make install FEATURES=` で CPU のみ。
待ち受けアドレスは `make service ADDR=100.x.y.z:3333` のように変えられる。Linux で初回だけ `sudo loginctl enable-linger $USER` が要る。

| コマンド | 内容 |
|---|---|
| `git pull && make install && make restart` | 更新 |
| `make stop` / `make start` | 一時停止 / 再開 (止めている間はログインしても起動しない) |
| `make status` / `make logs` | 状態の確認 / ログを追う (macOS は `~/Library/Logs/audio-drama-editor.log`) |
| `make uninstall` | サービスを止めて登録解除 (`~/.audio-drama-editor` のデータは残る) |

データは `~/.audio-drama-editor/` に保存される (`ADE_HOME` で変更可)。DB (`audio-drama-editor.db`) と
アップロードした画像・生成した音声 (`media/`) があり、このディレクトリをコピーすればバックアップになる。
LLM の接続先と API キーは画面の「アプリ設定」から保存する (DB に入る。キーは画面に表示しない)。
セリフの生成 (続きを書く・書き直す・演技を付ける) には、アプリ設定で選んだ LLM (OpenCode Go か llama-server) を使う。
OpenCode Go には、アプリの User-Agent (`audio-drama-editor/<version>`) とシーンごとのセッション ID (`x-opencode-session`) を付けて送る。

## 環境変数と設定

| 環境変数 | 既定値 | 意味 |
|---|---|---|
| `ADE_HOME` | `~/.audio-drama-editor` | データと設定の置き場所 |
| `ADE_ADDR` | `127.0.0.1:3000` | 待ち受けアドレス。`--addr` が優先 |
| `ADE_TTS` | (有効) | `off` で TTS モデルを読み込まずに起動する |
| `ADE_TTS_CHECKPOINT` | `Aratako/Irodori-TTS-600M-v3-VoiceDesign` | Irodori-TTS のチェックポイント |
| `ADE_TTS_CODEC` | `Aratako/Semantic-DACVAE-Japanese-32dim` | DACVAE コーデック |
| `ADE_RAG` | (有効) | `off` で記憶の検索 (EmbeddingGemma 2 + d1-3B) を使わずに起動する。有効なら初めて探すときに読み込む |
| `ADE_LLAMA_SERVER_URL` | `http://127.0.0.1:8080` | 設定画面の初期値。画面で保存した値が優先 |
| `ADE_OPENCODE_BASE_URL` | `https://opencode.ai/zen/go/v1` | 設定画面の初期値。画面で保存した値が優先 |
| `ADE_OPENCODE_API_KEY` | なし | 画面でキーを保存していないときに使う |

```
audio-drama-editor serve [--addr HOST:PORT]   サーバーを起動する
```

- 画面から変えられる設定は「画面で保存した値 → 環境変数 → 既定値」の順に決まる。`GET /api/settings` は値とその出どころ (`saved` / `env` / `default`) を返し、API キーは出どころだけを返す。
- 待ち受けアドレスとデータの置き場所は起動前に決まる必要があるので、画面には置かない。設定ファイルは作らない。

## 構成

| パス | 内容 |
|---|---|
| `crates/server` | `audio-drama-editor` バイナリ。axum REST API + SQLite、音声生成ワーカー、埋め込んだ画面の配信 |
| `crates/llm` | OpenAI 互換 chat (OpenCode Go / llama-server) と llama-server `/completion` |
| `crates/irodori` | Irodori-TTS v3 / VoiceDesign の candle 推論。feature `metal` / `cuda` |
| `frontend` | React + Vite + TanStack Query / Router (Bun) |
| `deploy` | systemd / launchd のサービス定義 (Makefile が使う) |

## 開発

```sh
cargo run --features metal -- serve        # API (127.0.0.1:3000)。データは ~/.audio-drama-editor
cd frontend && bun install && bun run dev  # 画面 (/api と /media は 3000 に転送。ADE_API で変更)
```

開発中のデータを分けたいときは `ADE_HOME=/tmp/ade cargo run -- serve`。

| コマンド | 内容 |
|---|---|
| `cargo test --workspace` | Rust のテスト。API は `crates/server/tests/api.rs` |
| `bun run typecheck` / `bun run lint` (frontend) | tsc / oxlint |
| `bun run test` (frontend) | Vitest (ユニット・コンポーネント) |
| `bun run test:e2e` (frontend) | Playwright。TTS 無効のサーバーを一時データで起動して実行する |
| `make test` | 上のテストをまとめて実行 |

画面の見た目は `frontend/src/styles/tokens.css`・`components.css` (デザインシステムから取り込み) と `app.css` (画面レイアウト)。

### 主な API

| | |
|---|---|
| `POST /api/tts/jobs` | `{"line_ids": [..], "seed"?: n, "params"?: {num_steps, cfg_scale_*, seconds, duration_scale}}` でセリフごとにジョブを積む |
| `GET /api/tts/events` | ジョブの状態変化 (queued → running → done / failed) を SSE で通知 |
| `GET /api/tts/status` | エンジンの状態 (loading / ready / failed / disabled) と実行中のジョブ |
| `GET` / `PUT /api/settings`, `PUT /api/settings/opencode-api-key`, `POST /api/settings/check` | アプリ設定、API キーの保存 (空で削除)、接続の確認 (`/models` を呼ぶ) |
| `GET /api/lines/{id}/caption` | 生成に使う caption のプレビュー (キャラの声 + 演技メタデータ、または手動上書き) |
| `GET /api/lines/{id}/takes`, `PUT /api/takes/{id}/adopt`, `DELETE /api/takes/{id}` | テイクの一覧・採用・削除。音声は `/media/takes/*.wav` |

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

CUDA (`--features cuda`) は candle 標準の演算と matmul + softmax の attention を使う。CUDA Toolkit (`nvcc`) が要る。
candle-kernels 0.11.0 は CUDA 12.2 以降 + sm_80 未満 (RTX 20 系など) でビルドできないため、修正を当てた版を
`vendor/candle-kernels` に置き `[patch.crates-io]` で差し替えている (candle の次のリリースで外す)。
RTX 2060 SUPER (sm_75) / CUDA 13.4 で PyTorch 基準と一致し、`examples/profile` は DiT forward (B=4) 110ms/step、
40 ステップのサンプリング 2.9s、8.4 秒分のデコード 0.5s。flash-attn は sm_80 以降が必要なので使っていない。

## キャラクター設計 (persona-lora の知見)

| 何を | どこに |
|---|---|
| どう話すか (一人称・呼び方・語尾・距離感、長さ別のサンプル台詞) | システムプロンプト (`speech_style`, `sample_lines`) |
| 何を知っているか (1 件 1 事実の短文) | RAG (`character_facts`) |
| 口調の土台 (llama-server のみ、任意) | リクエスト単位の LoRA (`lora_id`, `lora_scale`) |

システムプロンプトは「誰として・誰と・今どういう状況か」に絞る。セリフの長さはプロンプトの指示ではなく、
台詞計画の長さ区分 (S/M/L) と文字数検査で制御する。
