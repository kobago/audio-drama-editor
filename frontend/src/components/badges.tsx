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

export function SpeakerChip({
  name,
  color,
  avatar,
  pill,
}: {
  name: string
  color?: string
  /** 顔アイコンにする立ち絵の URL */
  avatar?: string
  pill?: boolean
}) {
  const cls = ['ade-speaker', avatar ? 'ade-speaker--avatar' : '', pill ? 'ade-speaker--pill' : ''].filter(Boolean).join(' ')
  return (
    <span className={cls} style={{ '--char': color } as React.CSSProperties}>
      {avatar && <img className="ade-avatar" src={avatar} alt="" />}
      {name}
    </span>
  )
}

export function PerformanceTags({ performance }: { performance: Performance }) {
  return (
    <>
      {performanceTags(performance).map((t) => (
        <span key={t.key} className="ade-perf">
          {t.strong && <b>{t.strong}</b>}
          {t.label}
        </span>
      ))}
    </>
  )
}

export function LengthBadge({ text, lengthClass }: { text: string; lengthClass: LengthClass | null }) {
  const n = countChars(text)
  const check = checkLength(text, lengthClass)
  const off = check === 'short' || check === 'long'
  return (
    <span className={off ? 'ade-len ade-len--off' : 'ade-len'}>
      {lengthClass && <b>{lengthClass}</b>}
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
      <button
        type="button"
        className="ade-job ade-job--failed"
        style={{ border: 0, cursor: 'pointer' }}
        title={job.error ?? undefined}
        onClick={(e) => {
          e.stopPropagation()
          onRetry()
        }}
      >
        失敗 · 再試行
      </button>
    )
  }
  return (
    <span className={`ade-job ade-job--${job.state}`} title={job.error ?? undefined}>
      {JOB_LABEL[job.state]}
    </span>
  )
}
