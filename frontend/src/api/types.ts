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

export interface Fact {
  id: number
  character_id: number
  text: string
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
> & { sort_order?: number }

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
  llama_server_url: Resolved
  opencode_base_url: Resolved
  opencode_model: Resolved
  /** キーそのものは返らない。どこから来たかだけ */
  opencode_api_key: { source: 'saved' | 'env' | null }
}

export interface AppSettingsInput {
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
