import type { Memory, MemoryKind } from '../api/types'

export const MEMORY_KINDS: { kind: MemoryKind; label: string; hint: string }[] = [
  { kind: 'experience', label: '経験', hint: 'したこと・起きたこと' },
  { kind: 'recollection', label: '思い出', hint: '心に残っている出来事' },
  { kind: 'knowledge', label: '知識', hint: '知っていること' },
  { kind: 'relation', label: '関係', hint: '人との関係・相手についての認識' },
  { kind: 'habit', label: '好み・習慣', hint: '好きなもの・いつもの振る舞い' },
]

export const kindLabel = (kind: MemoryKind) => MEMORY_KINDS.find((k) => k.kind === kind)?.label ?? kind

/** 「ボブ、母」「ボブ, 母」を人物の配列にする */
export function parsePeople(text: string): string[] {
  return text
    .split(/[、,，]/)
    .map((s) => s.trim())
    .filter(Boolean)
}

/** 時期と関わる人物を 1 行で (「小学4年の夏・ボブ、母」) */
export function memoryMeta(m: Pick<Memory, 'period' | 'people'>): string {
  return [m.period, m.people.join('、')].filter(Boolean).join('・')
}

/** 点数 (0〜2) を 0〜100 の割合に */
export const scorePercent = (score: number) => Math.round(Math.max(0, Math.min(2, score)) * 50)
