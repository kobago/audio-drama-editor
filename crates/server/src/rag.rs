//! 記憶の検索に使うモデルのワーカー: EmbeddingGemma 2 (埋め込み) と d1-3B (判断)。
//!
//! どちらも初めて使うときに専用スレッドで読み込み、リクエストを 1 件ずつ順に処理する。
//! 読み込みに失敗したら、次のリクエストでもう一度読み込みを試す。

use std::sync::{Arc, RwLock};

use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

pub const EMBEDDING_MODEL: &str = eg2::DEFAULT_MODEL;
pub const JUDGE_MODEL: &str = d1::DEFAULT_MODEL;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ModelStatus {
    Disabled,
    /// まだ読み込んでいない (初めて使うときに読み込む)
    Idle,
    Loading,
    Ready {
        device: String,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct RagStatus {
    pub embedder: ModelStatus,
    pub judge: ModelStatus,
}

type Vectors = anyhow::Result<Vec<Vec<f32>>>;

enum Task {
    Embed(Vec<String>, oneshot::Sender<Vectors>),
    Judge(String, Vec<d1::Question>, oneshot::Sender<Vectors>),
}

pub struct RagEngine {
    tx: mpsc::UnboundedSender<Task>,
    status: Arc<RwLock<RagStatus>>,
}

/// 読み込みを試し、結果を状態に書く。
fn ensure<T>(
    slot: &mut Option<T>,
    status: &RwLock<RagStatus>,
    which: fn(&mut RagStatus) -> &mut ModelStatus,
    load: impl FnOnce(&candle_core::Device) -> anyhow::Result<T>,
) -> anyhow::Result<()> {
    if slot.is_some() {
        return Ok(());
    }
    *which(&mut status.write().unwrap()) = ModelStatus::Loading;
    let loaded = (|| {
        let device = d1::select_device()?;
        Ok::<_, anyhow::Error>((load(&device)?, format!("{device:?}")))
    })();
    match loaded {
        Ok((model, device)) => {
            tracing::info!("loaded on {device}");
            *which(&mut status.write().unwrap()) = ModelStatus::Ready { device };
            *slot = Some(model);
            Ok(())
        }
        Err(e) => {
            tracing::error!("failed to load: {e:?}");
            *which(&mut status.write().unwrap()) = ModelStatus::Failed {
                error: format!("{e:#}"),
            };
            Err(e)
        }
    }
}

impl RagEngine {
    pub fn spawn() -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Task>();
        let status = Arc::new(RwLock::new(RagStatus {
            embedder: ModelStatus::Idle,
            judge: ModelStatus::Idle,
        }));
        let st = status.clone();
        std::thread::Builder::new()
            .name("rag".into())
            .spawn(move || {
                let mut embedder: Option<eg2::EmbeddingGemma2> = None;
                let mut judge: Option<d1::D1> = None;
                while let Some(task) = rx.blocking_recv() {
                    match task {
                        Task::Embed(texts, reply) => {
                            let out =
                                ensure(&mut embedder, &st, |s| &mut s.embedder, eg2::load_default)
                                    .and_then(|_| {
                                        let refs: Vec<&str> =
                                            texts.iter().map(String::as_str).collect();
                                        embedder.as_ref().unwrap().embed(&refs)
                                    });
                            let _ = reply.send(out);
                        }
                        Task::Judge(state, questions, reply) => {
                            let out = ensure(&mut judge, &st, |s| &mut s.judge, d1::load_default)
                                .and_then(|_| {
                                    judge.as_ref().unwrap().probabilities(&state, &questions)
                                });
                            let _ = reply.send(out);
                        }
                    }
                }
            })
            .expect("spawn rag worker");
        Self { tx, status }
    }

    pub fn disabled() -> Self {
        let (tx, _) = mpsc::unbounded_channel();
        Self {
            tx,
            status: Arc::new(RwLock::new(RagStatus {
                embedder: ModelStatus::Disabled,
                judge: ModelStatus::Disabled,
            })),
        }
    }

    pub fn status(&self) -> RagStatus {
        self.status.read().unwrap().clone()
    }

    pub fn enabled(&self) -> bool {
        !matches!(self.status().embedder, ModelStatus::Disabled)
    }

    /// 文の埋め込み (L2 正規化済み)。プレフィックスは呼び出し側で付ける。
    pub async fn embed(&self, texts: Vec<String>) -> Vectors {
        self.send(|reply| Task::Embed(texts, reply)).await
    }

    /// 状態に対する質問ごとの選択肢の確率。
    pub async fn judge(&self, state: String, questions: Vec<d1::Question>) -> Vectors {
        self.send(|reply| Task::Judge(state, questions, reply))
            .await
    }

    async fn send(&self, task: impl FnOnce(oneshot::Sender<Vectors>) -> Task) -> Vectors {
        if !self.enabled() {
            anyhow::bail!("memory search is disabled (ADE_RAG=off)");
        }
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(task(reply))
            .map_err(|_| anyhow::anyhow!("rag worker stopped"))?;
        rx.await?
    }
}
