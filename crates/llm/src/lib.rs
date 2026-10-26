//! LLM ドライバ。
//!
//! - [`ChatClient`]: OpenAI 互換の `/v1/chat/completions` (OpenCode Go, llama-server 共通)
//! - [`CompletionClient`]: llama-server の `/completion`。台本形式の続きを書かせる用途で、
//!   GBNF grammar やリクエスト単位の LoRA 切り替えも使える。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("unexpected response: {0}")]
    Response(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

/// llama-server でリクエスト単位に適用する LoRA アダプター。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraScale {
    pub id: u32,
    pub scale: f32,
}

#[derive(Debug, Clone, Default)]
pub struct SamplingParams {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stop: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ChatClient {
    http: reqwest::Client,
    /// 例: `https://opencode.ai/zen/go/v1`, `http://localhost:8080/v1`
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl ChatClient {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            api_key,
            model: model.into(),
        }
    }

    /// `extra` はリクエストボディにそのまま追加される (llama-server の `lora`, `json_schema` など)。
    pub async fn chat(
        &self,
        messages: &[Message],
        params: &SamplingParams,
        extra: Map<String, Value>,
    ) -> Result<String> {
        let mut body = json!({ "model": self.model, "messages": messages });
        let obj = body.as_object_mut().unwrap();
        if let Some(t) = params.temperature {
            obj.insert("temperature".into(), t.into());
        }
        if let Some(n) = params.max_tokens {
            obj.insert("max_tokens".into(), n.into());
        }
        if !params.stop.is_empty() {
            obj.insert("stop".into(), json!(params.stop));
        }
        obj.extend(extra);

        let mut req = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let res: Value = req.send().await?.error_for_status()?.json().await?;
        res["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::Response(res.to_string()))
    }
}

#[derive(Debug, Clone, Default)]
pub struct CompletionOptions {
    pub sampling: SamplingParams,
    pub grammar: Option<String>,
    pub lora: Vec<LoraScale>,
}

#[derive(Debug, Clone)]
pub struct CompletionClient {
    http: reqwest::Client,
    /// llama-server のルート。例: `http://localhost:8080`
    base_url: String,
}

impl CompletionClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
        }
    }

    pub async fn complete(&self, prompt: &str, opts: &CompletionOptions) -> Result<String> {
        let mut body = json!({ "prompt": prompt, "cache_prompt": true });
        let obj = body.as_object_mut().unwrap();
        if let Some(t) = opts.sampling.temperature {
            obj.insert("temperature".into(), t.into());
        }
        if let Some(n) = opts.sampling.max_tokens {
            obj.insert("n_predict".into(), n.into());
        }
        if !opts.sampling.stop.is_empty() {
            obj.insert("stop".into(), json!(opts.sampling.stop));
        }
        if let Some(g) = &opts.grammar {
            obj.insert("grammar".into(), g.clone().into());
        }
        if !opts.lora.is_empty() {
            obj.insert("lora".into(), json!(opts.lora));
        }

        let res: Value = self
            .http
            .post(format!("{}/completion", self.base_url))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        res["content"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::Response(res.to_string()))
    }
}
