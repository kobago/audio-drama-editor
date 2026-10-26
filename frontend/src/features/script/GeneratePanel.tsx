import { RotateCcw, Sparkles, X } from 'lucide-react'
import { useId, useState } from 'react'
import { useGenerate, useRedoGeneration } from '../../api/hooks'
import type { Generated, Line } from '../../api/types'

const COUNTS = [1, 2, 3, 4, 5, 6, 8]

/**
 * 続きを書く: LLM が次の数行の計画 (話者・意図・長さ) を立てて台本に入れ、セリフと演技を 1 行ずつ書く。
 * 入れた行は気に入らなければ、まとめて作り直すか、1 行ずつインスペクタで書き直す。
 */
export function GeneratePanel({
  sceneId,
  after,
  afterLabel,
  onGenerated,
  onClose,
}: {
  sceneId: number
  /** この行の後ろに入れる。null なら末尾 */
  after: Line | null
  afterLabel?: string
  onGenerated: (g: Generated) => void
  onClose: () => void
}) {
  const generate = useGenerate(sceneId)
  const redo = useRedoGeneration(sceneId)
  const [count, setCount] = useState(4)
  const [direction, setDirection] = useState('')
  const [last, setLast] = useState<Generated | null>(null)
  const id = useId()
  const busy = generate.isPending || redo.isPending
  const error = generate.error ?? redo.error

  const run = async () => {
    const g = await generate.mutateAsync({ after_line_id: after?.id ?? null, count, direction: direction.trim() })
    setLast(g)
    onGenerated(g)
  }

  return (
    <form
      className="ade-generate"
      aria-label="続きを書く"
      onSubmit={(e) => {
        e.preventDefault()
        if (!busy) void run().catch(() => {})
      }}
    >
      <div className="ade-field">
        <label htmlFor={`${id}-count`}>行数</label>
        <select
          id={`${id}-count`}
          className="ade-input"
          value={count}
          onChange={(e) => setCount(Number(e.target.value))}
        >
          {COUNTS.map((n) => (
            <option key={n} value={n}>
              {n} 行
            </option>
          ))}
        </select>
      </div>
      <div className="ade-field">
        <label htmlFor={`${id}-direction`}>次に起きること</label>
        <input
          id={`${id}-direction`}
          className="ade-input"
          placeholder="空なら流れに任せる。例: ボブが転校のことを打ち明ける"
          value={direction}
          onChange={(e) => setDirection(e.target.value)}
        />
      </div>
      <div className="row" style={{ flexWrap: 'nowrap' }}>
        <button className="ade-btn ade-btn--primary" type="submit" disabled={busy}>
          <Sparkles aria-hidden="true" />
          書く
        </button>
        <button className="ade-icon-btn" type="button" aria-label="続きを書くのを閉じる" onClick={onClose}>
          <X aria-hidden="true" />
        </button>
      </div>
      <div className="ade-generate__foot">
        <p className="hint" role="status" style={{ flex: '1 1 20em' }}>
          {busy
            ? '流れを考えています。行を入れたら、セリフと演技を 1 行ずつ書く (記憶も 1 行ごとに探す)。'
            : after
              ? `「${afterLabel ?? after.text}」の後ろに入れる。`
              : 'シーンの末尾に入れる。行を選ぶと、その後ろに入れる。'}
        </p>
        {last && !busy && (
          <button
            className="ade-btn ade-btn--sm"
            type="button"
            title={`さっき入れた ${last.lines.length} 行を消して、同じ条件で書き直す`}
            onClick={() =>
              void redo
                .mutateAsync(last.generation_id)
                .then((g) => {
                  setLast(g)
                  onGenerated(g)
                })
                .catch(() => {})
            }
          >
            <RotateCcw aria-hidden="true" />
            まとめて作り直す
          </button>
        )}
      </div>
      {error && (
        <p className="error-text" role="alert" style={{ gridColumn: '1 / -1' }}>
          {error.message}
        </p>
      )}
    </form>
  )
}
