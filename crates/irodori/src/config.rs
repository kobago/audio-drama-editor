use serde::Deserialize;

/// `irodori_tts.config.ModelConfig` のうち推論に必要な項目。
/// チェックポイント (safetensors) の `config_json` メタデータから読む。
#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub latent_dim: usize,
    pub latent_patch_size: usize,
    pub model_dim: usize,
    pub num_layers: usize,
    pub num_heads: usize,
    pub mlp_ratio: f64,
    pub text_mlp_ratio: Option<f64>,
    pub speaker_mlp_ratio: Option<f64>,
    pub text_vocab_size: usize,
    pub text_tokenizer_repo: String,
    pub text_add_bos: bool,
    pub text_dim: usize,
    pub text_layers: usize,
    pub text_heads: usize,
    #[serde(default)]
    pub use_caption_condition: bool,
    pub use_speaker_condition: Option<bool>,
    pub caption_vocab_size: Option<usize>,
    pub caption_tokenizer_repo: Option<String>,
    pub caption_add_bos: Option<bool>,
    pub caption_dim: Option<usize>,
    pub caption_layers: Option<usize>,
    pub caption_heads: Option<usize>,
    pub caption_mlp_ratio: Option<f64>,
    pub speaker_dim: usize,
    pub speaker_layers: usize,
    pub speaker_heads: usize,
    pub speaker_patch_size: usize,
    pub timestep_embed_dim: usize,
    pub adaln_rank: usize,
    pub norm_eps: f64,
    #[serde(default)]
    pub use_duration_predictor: bool,
    pub duration_hidden_dim: usize,
    pub duration_layers: usize,
    pub duration_architecture: String,
    pub max_text_len: Option<usize>,
    pub max_caption_len: Option<usize>,
}

impl ModelConfig {
    pub fn patched_latent_dim(&self) -> usize {
        self.latent_dim * self.latent_patch_size
    }

    pub fn use_speaker_condition(&self) -> bool {
        self.use_speaker_condition.unwrap_or(!self.use_caption_condition)
    }

    pub fn text_mlp_ratio(&self) -> f64 {
        self.text_mlp_ratio.unwrap_or(self.mlp_ratio)
    }

    pub fn speaker_mlp_ratio(&self) -> f64 {
        self.speaker_mlp_ratio.unwrap_or(self.mlp_ratio)
    }

    pub fn caption_vocab_size(&self) -> usize {
        self.caption_vocab_size.unwrap_or(self.text_vocab_size)
    }

    pub fn caption_tokenizer_repo(&self) -> &str {
        self.caption_tokenizer_repo.as_deref().unwrap_or(&self.text_tokenizer_repo)
    }

    pub fn caption_add_bos(&self) -> bool {
        self.caption_add_bos.unwrap_or(self.text_add_bos)
    }

    pub fn caption_dim(&self) -> usize {
        self.caption_dim.unwrap_or(self.text_dim)
    }

    pub fn caption_layers(&self) -> usize {
        self.caption_layers.unwrap_or(self.text_layers)
    }

    pub fn caption_heads(&self) -> usize {
        self.caption_heads.unwrap_or(self.text_heads)
    }

    pub fn caption_mlp_ratio(&self) -> f64 {
        self.caption_mlp_ratio.unwrap_or_else(|| self.text_mlp_ratio())
    }

    pub fn max_text_len(&self) -> usize {
        self.max_text_len.unwrap_or(256)
    }

    pub fn max_caption_len(&self) -> usize {
        self.max_caption_len.unwrap_or_else(|| self.max_text_len())
    }
}
