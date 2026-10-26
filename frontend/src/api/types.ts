export type LengthClass = 'S' | 'M' | 'L'

export interface Project {
  id: number
  title: string
  synopsis: string
  created_at: string
  updated_at: string
}

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

export interface Scene {
  id: number
  project_id: number
  title: string
  setting: string
  summary: string
  background_path: string | null
  sort_order: number
}

export interface Performance {
  emotion?: string
  intensity?: number
  tempo?: string
  volume?: string
  distance?: '近い' | '普通' | '遠い'
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
}
