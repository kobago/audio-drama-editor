import type { Job, Line } from '../../api/types'
import { JobPill, LengthBadge, PerformanceTags, SpeakerChip } from '../../components/badges'
import { formatDuration } from '../../lib/script'

export interface Speaker {
  name: string
  color?: string
  avatar?: string
}

/**
 * 台本の 1 行。クリックで編集対象 (focused) にし、チェックで音声生成の対象 (selected) に加える。
 * 2 つは別の状態で、見た目も分ける: focused は太い藍の枠、selected は藍の地とチェック。
 */
export function ScriptLineRow({
  line,
  index,
  speaker,
  focused,
  selected,
  job,
  onFocus,
  onToggleSelect,
  onRetry,
}: {
  line: Line
  index: number
  speaker?: Speaker
  focused: boolean
  selected: boolean
  job?: Job
  onFocus: () => void
  onToggleSelect: () => void
  onRetry: () => void
}) {
  if (line.kind === 'direction') {
    return (
      <div
        className="ade-line ade-line--direction"
        data-focused={focused}
        data-line-id={line.id}
        onClick={onFocus}
        role="listitem"
      >
        <span />
        <span />
        <div className="ade-line__body">
          <p className="ade-line__text">{line.text || 'ト書き (空)'}</p>
        </div>
        <span />
      </div>
    )
  }

  return (
    <div
      className="ade-line"
      role="listitem"
      aria-selected={selected}
      data-focused={focused}
      data-line-id={line.id}
      style={{ '--char': speaker?.color } as React.CSSProperties}
      onClick={onFocus}
    >
      <input
        className="ade-check"
        type="checkbox"
        checked={selected}
        aria-label={`${index + 1} 行目を音声生成の対象にする`}
        onClick={(e) => e.stopPropagation()}
        onChange={onToggleSelect}
      />
      <div className="ade-line__speaker">
        {speaker ? (
          <SpeakerChip name={speaker.name} color={speaker.color} avatar={speaker.avatar} />
        ) : (
          <span style={{ fontSize: 12, color: 'var(--warning)' }}>話者なし</span>
        )}
      </div>
      <div className="ade-line__body">
        {line.gen_state === 'generating' ? (
          <p className="ade-line__text ade-line__writing" aria-live="polite">
            <span className="ade-writing-dot" aria-hidden="true" />
            書いています…{line.intent && <span className="ade-line__intent">{line.intent}</span>}
          </p>
        ) : (
          <p className="ade-line__text">{line.text || <span style={{ color: 'var(--ink-muted)' }}>(空のセリフ)</span>}</p>
        )}
        {line.gen_state === 'failed' && (
          <span className="error-text" style={{ fontSize: 12 }}>
            書けませんでした: {line.gen_error}
          </span>
        )}
        <div className="ade-line__meta">
          <PerformanceTags performance={line.performance} />
          {line.caption_override?.trim() && <span className="ade-perf">caption 手動</span>}
          <LengthBadge text={line.text} lengthClass={line.length_class} />
        </div>
      </div>
      <div className="ade-line__side">
        <LineStatus line={line} job={job} onRetry={onRetry} />
      </div>
    </div>
  )
}

function LineStatus({ line, job, onRetry }: { line: Line; job?: Job; onRetry: () => void }) {
  if (line.gen_state === 'generating') return <span className="ade-job ade-job--running">執筆中</span>
  if (job && job.state !== 'done') return <JobPill job={job} onRetry={onRetry} />
  if (line.adopted_take_id !== null) {
    return (
      <>
        {!!line.adopted_duration_ms && <span className="ade-timecode">{formatDuration(line.adopted_duration_ms)}</span>}
        <span className="ade-job ade-job--done">{job ? '完了' : '採用済み'}</span>
      </>
    )
  }
  if (line.take_count > 0) return <span style={{ fontSize: 12, color: 'var(--ink-muted)' }}>未採用</span>
  return <span style={{ fontSize: 12, color: 'var(--ink-muted)' }}>未生成</span>
}
