import type { Job, LengthClass, Performance } from '../api/types'
import { checkLength, countChars, performanceTags } from '../lib/script'

/** 件数のバッジ。0 件なら何も出さない。 */
export function CountBadge({
  count,
  unit,
  variant = 'default',
}: {
  count: number
  /** 読み上げ用の単位 (「行」「本」) */
  unit: string
  variant?: 'default' | 'muted' | 'corner'
}) {
  if (count <= 0) return null
  const cls = variant === 'default' ? 'ade-count' : `ade-count ade-count--${variant}`
  return (
    <span className={cls} aria-label={`${count} ${unit}`}>
      {count > 99 ? '99+' : count}
    </span>
  )
}

/** 話者名。識別色の文字で示し、長い名前は 1 行で省略する (title で全体を読める)。 */
export function SpeakerChip({
  name,
  color,
  avatar,
}: {
  name: string
  color?: string
  /** 顔アイコンにする立ち絵の URL */
  avatar?: string
}) {
  return (
    <span className="ade-speaker" style={{ '--char': color } as React.CSSProperties} title={name}>
      {avatar && <img className="ade-avatar" src={avatar} alt="" />}
      <span className="ade-speaker__name">{name}</span>
    </span>
  )
}

/** 演技指導の書き込み (朱)。`withNote` ならメモも続けて書く。 */
export function PerformanceTags({ performance, withNote }: { performance: Performance; withNote?: boolean }) {
  const tags = performanceTags(performance)
  const note = withNote ? performance.note?.trim() : undefined
  if (!tags.length && !note) return null
  return (
    <span className="ade-perf-line">
      {tags.map((t) => (
        <span key={t.key} className="ade-perf">
          {t.strong && <b>{t.strong}</b>}
          {t.strong && t.label ? ' ' : ''}
          {t.label}
        </span>
      ))}
      {note && <span className="ade-perf ade-perf--note">{note}</span>}
    </span>
  )
}

export function LengthBadge({ text, lengthClass }: { text: string; lengthClass: LengthClass | null }) {
  const n = countChars(text)
  const check = checkLength(text, lengthClass)
  const off = check === 'short' || check === 'long'
  return (
    <span className={off ? 'ade-len ade-len--off' : 'ade-len'}>
      {lengthClass && <b>{lengthClass} </b>}
      {n}字{check === 'long' && ' 長い'}
      {check === 'short' && ' 短い'}
    </span>
  )
}

const JOB_LABEL: Record<Job['state'], string> = {
  queued: '待機中',
  running: '生成中',
  done: '完了',
  failed: '失敗',
}

export function JobPill({ job, onRetry }: { job: Job; onRetry?: () => void }) {
  if (job.state === 'failed' && onRetry) {
    return (
      <span className="ade-job ade-job--failed" title={job.error ?? undefined}>
        生成に失敗
        <button
          type="button"
          className="ade-btn ade-btn--sm"
          title={job.error ?? undefined}
          onClick={(e) => {
            e.stopPropagation()
            onRetry()
          }}
        >
          再試行
        </button>
      </span>
    )
  }
  return (
    <span className={`ade-job ade-job--${job.state}`} title={job.error ?? undefined}>
      {JOB_LABEL[job.state]}
    </span>
  )
}
