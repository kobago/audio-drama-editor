import type { Character, LengthClass, Performance } from '../api/types'

/** 長さ区分ごとの文字数の目安 (両端を含む)。 */
export const LENGTH_RANGES: Record<LengthClass, [number, number]> = {
  S: [5, 15],
  M: [20, 50],
  L: [60, 150],
}

/** セリフの文字数。空白と改行は数えない。 */
export function countChars(text: string): number {
  return [...text.replace(/\s/g, '')].length
}

export type LengthCheck = 'ok' | 'short' | 'long'

export function checkLength(text: string, cls: LengthClass | null): LengthCheck | null {
  if (!cls) return null
  const n = countChars(text)
  const [min, max] = LENGTH_RANGES[cls]
  if (n < min) return 'short'
  if (n > max) return 'long'
  return 'ok'
}

/** ミリ秒を `00:08.40` 形式にする。 */
export function formatDuration(ms: number): string {
  const totalCs = Math.round(ms / 10)
  const minutes = Math.floor(totalCs / 6000)
  const seconds = Math.floor((totalCs % 6000) / 100)
  const cs = totalCs % 100
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(minutes)}:${pad(seconds)}.${pad(cs)}`
}

/** 感情の強さ (0〜1) を 3 段階の言葉にする。サーバーの caption 組み立てと同じ閾値。 */
export function intensityLabel(intensity: number | undefined): string | null {
  if (intensity === undefined) return null
  if (intensity < 0.34) return '弱'
  if (intensity >= 0.67) return '強'
  return '中'
}

export interface PerformanceTag {
  key: string
  /** 感情の名前など、太字にする部分 */
  strong?: string
  label: string
}

/** 演技メタデータを表示用のタグにする。空の項目は出さない。 */
export function performanceTags(p: Performance): PerformanceTag[] {
  const tags: PerformanceTag[] = []
  const text = (v: string | undefined) => v?.trim() || undefined
  const emotion = text(p.emotion)
  if (emotion) tags.push({ key: 'emotion', strong: emotion, label: intensityLabel(p.intensity) ?? '' })
  const pairs: [keyof Performance, string][] = [
    ['distance', '距離'],
    ['tempo', 'テンポ'],
    ['volume', '声量'],
    ['voice_quality', '声質'],
  ]
  for (const [k, name] of pairs) {
    const v = text(p[k] as string | undefined)
    if (v) tags.push({ key: k, label: `${name} ${v}` })
  }
  return tags
}

/** キャラクターの識別色。作成順に char-1〜char-7、8 人目以降は char-8。 */
export function characterColors(characters: Character[]): Map<number, string> {
  const sorted = [...characters].sort((a, b) => a.sort_order - b.sort_order || a.id - b.id)
  return new Map(sorted.map((c, i) => [c.id, `var(--char-${Math.min(i + 1, 8)})`]))
}
