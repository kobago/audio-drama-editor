import { Link } from '@tanstack/react-router'
import { Ban, Pin, Search } from 'lucide-react'
import { useState } from 'react'
import { useMarkMemory, useMemories, useRecall, useRunRecall } from '../../api/hooks'
import type { Character, MemoryMark, RecallItem } from '../../api/types'
import { CountBadge } from '../../components/badges'
import { kindLabel, memoryMeta, scorePercent } from '../../lib/memory'

/** 最初に見せる候補の数。残りは「すべて表示」で */
const SHOWN = 6

/**
 * このセリフで使う記憶: 2 段階の検索の結果と、必ず使う / 使わないの指定。
 * セリフの「意図」で探す (空ならセリフの本文)。LLM で書くときも、同じ方法で毎回探し直す。
 */
export function MemoryPanel({
  lineId,
  character,
  intent,
  disabled,
}: {
  lineId: number
  character: Character
  intent: string
  /** 書いている途中は探さない (書き終わると、書くときに探した結果が出る) */
  disabled?: boolean
}) {
  const recall = useRecall(lineId)
  const run = useRunRecall(lineId)
  const mark = useMarkMemory(lineId)
  const memories = useMemories(character.id)
  const [all, setAll] = useState(false)

  const items = recall.data?.items ?? []
  const used = items.filter((i) => i.selected)
  const rest = items.filter((i) => !i.selected)
  const shownRest = all ? rest : rest.slice(0, Math.max(0, SHOWN - used.length))
  const hidden = rest.length - shownRest.length
  const noMemories = memories.data?.length === 0
  const last = recall.data?.recall

  const toggle = (item: RecallItem, m: MemoryMark) =>
    mark.mutate({ memoryId: item.memory.id, mark: item.mark === m ? null : m })

  return (
    <section className="ade-panel" aria-label="このセリフで使う記憶">
      <header className="ade-panel__head">
        <h3 style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          使う記憶
          <CountBadge count={used.length} unit="件" variant="muted" />
        </h3>
        <button
          className="ade-btn ade-btn--sm"
          type="button"
          disabled={run.isPending || noMemories || disabled}
          onClick={() => run.mutate(intent.trim())}
        >
          <Search aria-hidden="true" />
          {last ? 'もう一度探す' : '記憶を探す'}
        </button>
      </header>
      <div className="ade-panel__body" style={{ gap: 8 }}>
        {noMemories ? (
          <p className="ade-memory__empty">
            {character.name}の記憶がまだない。
            <Link
              to="/projects/$projectId/cast/$characterId"
              params={{ projectId: character.project_id, characterId: character.id }}
              search={{ tab: 'memories' }}
            >
              キャストの「記憶」
            </Link>
            で追加すると、場面に合うものを探せる。
          </p>
        ) : (
          !last &&
          !run.isPending && (
            <p className="ade-memory__status">
              {intent.trim() ? `意図「${intent.trim()}」で探す。` : 'セリフの本文で探す。意図を書くと、それで探す。'}
            </p>
          )
        )}

        {run.isPending && (
          <p className="ade-memory__status" role="status">
            記憶を探しています… 候補ごとに判断するので数秒かかる。初回はモデルの読み込みも入る。
          </p>
        )}
        {run.isError && <span className="error-text">{run.error.message}</span>}
        {last && !run.isPending && (
          <p className="ade-memory__status">
            「{last.intent}」で探した · {((last.timing.embed_ms + last.timing.judge_ms) / 1000).toFixed(1)} 秒
          </p>
        )}

        {items.length > 0 && (
          <ul className="ade-recall" aria-label="記憶の候補">
            {[...used, ...shownRest].map((item) => (
              <RecallRow key={item.memory.id} item={item} onToggle={(m) => toggle(item, m)} busy={mark.isPending} />
            ))}
          </ul>
        )}
        {hidden > 0 && (
          <button className="ade-btn ade-btn--quiet ade-btn--sm" type="button" onClick={() => setAll(true)}>
            ほかの候補も表示
            <CountBadge count={hidden} unit="件" variant="muted" />
          </button>
        )}
        {last && items.length === 0 && <p className="ade-memory__empty">候補になる記憶がなかった。</p>}
      </div>
    </section>
  )
}

function RecallRow({
  item,
  onToggle,
  busy,
}: {
  item: RecallItem
  onToggle: (m: MemoryMark) => void
  busy: boolean
}) {
  const { memory: m, candidate: c } = item
  const meta = memoryMeta(m)
  const cls = ['ade-recall__item', item.selected && 'ade-recall__item--used', item.mark === 'exclude' && 'ade-recall__item--excluded']
    .filter(Boolean)
    .join(' ')
  return (
    <li className={cls}>
      <div className="ade-recall__body">
        <span className="ade-recall__head">
          <span className="ade-memory__kind">{kindLabel(m.kind)}</span>
          {meta && <span className="ade-memory__meta">{meta}</span>}
          {item.selected && <span className="ade-recall__used">使う</span>}
        </span>
        <span className="ade-recall__text">{m.text}</span>
        {c ? (
          <span
            className="ade-score"
            title={`関係の強さ ${c.relevance.toFixed(2)} · いちばん近い記憶に選ばれた確率 ${c.choice.toFixed(2)} · 類似度 ${c.similarity.toFixed(2)}`}
          >
            <span className="ade-score__bar" aria-hidden="true">
              <span style={{ width: `${scorePercent(c.score)}%` }} />
            </span>
            <span className="ade-score__value">{scorePercent(c.score)}</span>
          </span>
        ) : (
          <span className="ade-memory__meta">{item.mark === 'pin' ? '候補外だが必ず使う' : '探す対象から外している'}</span>
        )}
      </div>
      <div className="ade-recall__actions">
        <button
          className="ade-icon-btn"
          type="button"
          aria-pressed={item.mark === 'pin'}
          aria-label={item.mark === 'pin' ? '必ず使うのをやめる' : '必ず使う'}
          title="必ず使う"
          disabled={busy}
          onClick={() => onToggle('pin')}
        >
          <Pin aria-hidden="true" />
        </button>
        <button
          className="ade-icon-btn ade-icon-btn--danger"
          type="button"
          aria-pressed={item.mark === 'exclude'}
          aria-label={item.mark === 'exclude' ? '使わないのをやめる' : '使わない'}
          title="使わない"
          disabled={busy}
          onClick={() => onToggle('exclude')}
        >
          <Ban aria-hidden="true" />
        </button>
      </div>
    </li>
  )
}
