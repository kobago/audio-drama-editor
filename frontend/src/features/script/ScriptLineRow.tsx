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
  const no = <span className="ade-line__no">{index + 1}</span>

  if (line.kind === 'direction') {
    return (
      <div
        className="ade-line ade-line--direction"
        data-focused={focused}
        data-line-id={line.id}
        onClick={onFocus}
        role="listitem"
      >
        {no}
        <span />
        <div className="ade-line__body">
          <p className="ade-line__text">{stripParens(line.text) || 'ト書き (空)'}</p>
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
      onClick={onFocus}
    >
      {no}
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
          <SpeakerChip name={speaker.name} color={speaker.color} />
        ) : (
          <span className="hint" style={{ color: 'var(--warn)' }}>
            話者なし
          </span>
        )}
      </div>
      <div className="ade-line__body">
        {line.gen_state === 'generating' ? (
          <p className="ade-line__writing" aria-live="polite">
            <span className="ade-writing-dot" aria-hidden="true" />
            書いています…{line.intent && <span className="ade-line__intent">{line.intent}</span>}
          </p>
        ) : (
          <p className="ade-line__text">{line.text || <span className="ade-line__empty">(空のセリフ)</span>}</p>
        )}
        {line.gen_state === 'failed' && <p className="error-text">書けませんでした: {line.gen_error}</p>}
        <div className="ade-line__meta">
          <PerformanceTags performance={line.performance} />
          {line.caption_override?.trim() && <span className="ade-perf">音声指示 手動</span>}
          {line.gen_state !== 'generating' && <LengthBadge text={line.text} lengthClass={line.length_class} />}
        </div>
      </div>
      <div className="ade-line__side">
        <LineStatus line={line} job={job} onRetry={onRetry} />
      </div>
    </div>
  )
}

/** ト書きのかっこは表示側で付けるので、本文に書かれたかっこは外す */
function stripParens(text: string) {
  return text.trim().replace(/^[（(]/, '').replace(/[）)]$/, '')
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
  if (line.take_count > 0) return <span className="ade-job">未採用</span>
  return <span className="ade-job">未生成</span>
}
