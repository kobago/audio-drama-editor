export type LengthClass = 'S' | 'M' | 'L'

export interface Project {
  id: number
  title: string
  synopsis: string
  created_at: string
  updated_at: string
}

export type ProjectInput = Pick<Project, 'title' | 'synopsis'>

export interface SampleLine {
  length: LengthClass
  text: string
}

export interface Character {
  id: number
  project_id: number
  name: string
  reading: string
  /** 何者か (プロフィール) */
  profile: string
  /** どう話すか → システムプロンプト */
  speech_style: string
  sample_lines: SampleLine[]
  lora_id: number | null
  lora_scale: number
  voice_ref_path: string | null
  voice_caption: string
  voice_seed: number | null
  sort_order: number
  created_at: string
}

export type CharacterInput = Omit<Character, 'id' | 'project_id' | 'created_at'>

export type MemoryKind = 'experience' | 'recollection' | 'knowledge' | 'relation' | 'habit'

/** キャラクターの記憶 (RAG)。セリフを書くときに、場面に合うものを選んで LLM に渡す */
export interface Memory {
  id: number
  character_id: number
  kind: MemoryKind
  /** いつのことか (「小学4年の夏」など) */
  period: string
  /** 関わる人物 */
  people: string[]
  text: string
  /** 埋め込み済みか (検索の前に足りない分を作る) */
  embedded: boolean
  created_at: string
}

export type MemoryInput = Pick<Memory, 'kind' | 'period' | 'people' | 'text'>

export type MemoryMark = 'pin' | 'exclude'

export interface RecallCandidate {
  memory_id: number
  /** 検索文とのコサイン類似度 */
  similarity: number
  /** 判断モデルの 3 段階の期待値 (0〜1) */
  relevance: number
  /** 判断モデルが 1 つ選んだときの確率 */
  choice: number
  /** 並べ替えの点数 (relevance + choice、0〜2) */
  score: number
}

export interface RecallItem {
  memory: Memory
  /** 検索の点数。候補に入っていなければ null */
  candidate: RecallCandidate | null
  mark: MemoryMark | null
  /** セリフ生成に使うか */
  selected: boolean
}

export interface Recall {
  recall: {
    intent: string
    created_at: string
    timing: { embed_ms: number; judge_ms: number; none: number }
  } | null
  items: RecallItem[]
}

export interface Portrait {
  id: number
  character_id: number
  label: string
  path: string
}

export interface Scene {
  id: number
  project_id: number
  title: string
  setting: string
  summary: string
  background_path: string | null
  sort_order: number
}

export type SceneInput = Omit<Scene, 'id' | 'project_id'>

export interface Performance {
  emotion?: string
  /** 0〜1 */
  intensity?: number
  tempo?: string
  volume?: string
  distance?: string
  voice_quality?: string
  note?: string
}

export interface Line {
  id: number
  scene_id: number
  kind: 'dialogue' | 'direction'
  character_id: number | null
  text: string
  tts_text: string | null
  length_class: LengthClass | null
  performance: Performance
  caption_override: string | null
  pause_before_ms: number
  pause_after_ms: number
  sort_order: number
  portrait_id: number | null
  /** 何を言うか (生成の計画。書き直しと記憶の検索に使う) */
  intent: string
  /** LLM が書いているところ / 書けなかった */
  gen_state: 'generating' | 'failed' | null
  gen_error: string | null
  /** 「続きを書く」で入れた行なら、その生成 */
  generation_id: number | null
  /** 書き直す前の案の数 */
  version_count: number
  adopted_take_id: number | null
  adopted_take_path: string | null
  adopted_duration_ms: number | null
  take_count: number
}

export type LineInput = Pick<
  Line,
  | 'kind'
  | 'character_id'
  | 'text'
  | 'tts_text'
  | 'length_class'
  | 'performance'
  | 'caption_override'
  | 'pause_before_ms'
  | 'pause_after_ms'
  | 'portrait_id'
  | 'intent'
> & { sort_order?: number }

export interface Generated {
  generation_id: number
  lines: Line[]
}

/** 書き直す前の案 */
export interface LineVersion {
  id: number
  line_id: number
  text: string
  intent: string
  length_class: LengthClass | null
  performance: Performance
  created_at: string
}

export interface SynthesisParams {
  num_steps: number
  cfg_scale_text: number
  cfg_scale_caption: number
  cfg_scale_speaker: number
  seconds: number | null
  duration_scale: number
}

export interface Take {
  id: number
  line_id: number
  path: string
  seed: number | null
  caption: string
  params: SynthesisParams
  adopted: boolean
  created_at: string
  duration_ms: number
}

export type JobState = 'queued' | 'running' | 'done' | 'failed'

export interface Job {
  id: string
  line_id: number
  state: JobState
  take_id: number | null
  error: string | null
}

export type EngineStatus =
  | { state: 'disabled' }
  | { state: 'loading' }
  | { state: 'ready'; device: string }
  | { state: 'failed'; error: string }

export interface TtsStatus {
  engine: EngineStatus
  jobs: Job[]
}

export interface CaptionPreview {
  caption: string
  source: 'override' | 'composed'
  text: string
}

export type SettingSource = 'saved' | 'env' | 'default'

export interface Resolved {
  value: string
  source: SettingSource
}

export interface AppSettings {
  /** セリフの生成に使う接続先 */
  llm_provider: LlmProvider
  llama_server_url: Resolved
  opencode_base_url: Resolved
  opencode_model: Resolved
  /** キーそのものは返らない。どこから来たかだけ */
  opencode_api_key: { source: 'saved' | 'env' | null }
}

export interface AppSettingsInput {
  llm_provider: LlmProvider
  llama_server_url: string
  opencode_base_url: string
  opencode_model: string
}

export type LlmProvider = 'llama_server' | 'opencode'

export interface ConnectionCheck {
  ok: boolean
  models: string[]
  error: string | null
}
