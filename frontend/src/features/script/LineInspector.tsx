import { Mic } from 'lucide-react'
import { useEffect, useId, useRef, useState } from 'react'
import { fileUrl } from '../../api/client'
import { toLineInput, useAdoptTake, useCaption, useDeleteTake, useTakes, useUpdateLine } from '../../api/hooks'
import type { Character, Job, Line, LineInput, Performance, Portrait, Scene } from '../../api/types'
import { CountBadge, JobPill, SpeakerChip } from '../../components/badges'
import { TakeItem } from '../../components/TakeItem'
import { MemoryPanel } from './MemoryPanel'
import { countChars, intensityLabel, LENGTH_RANGES } from '../../lib/script'

/** 入力が止まってから保存するまでの時間 */
const SAVE_DELAY_MS = 600

export function LineInspector({
  line,
  position,
  total,
  scene,
  characters,
  portraits,
  colors,
  job,
  onGenerate,
}: {
  line: Line
  position: number
  total: number
  scene: Scene
  characters: Character[]
  portraits: Map<number, Portrait[]>
  colors: Map<number, string>
  job?: Job
  onGenerate: () => void
}) {
  const update = useUpdateLine(scene.id)
  const [draft, setDraft] = useState<LineInput>(() => toLineInput(line))
  const latest = useRef(draft)
  latest.current = draft

  // 入力が止まったら保存する。保存済みの内容と同じなら何もしない。
  const saved = JSON.stringify(toLineInput(line))
  const current = JSON.stringify(draft)
  useEffect(() => {
    if (current === saved) return
    const t = setTimeout(() => update.mutate({ id: line.id, input: latest.current }), SAVE_DELAY_MS)
    return () => clearTimeout(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current])

  // 保存待ちのまま別の行に移ったら、その場で保存する
  const savedRef = useRef(saved)
  savedRef.current = saved
  const mutate = useRef(update.mutate)
  mutate.current = update.mutate
  useEffect(
    () => () => {
      if (JSON.stringify(latest.current) !== savedRef.current) mutate.current({ id: line.id, input: latest.current })
    },
    [line.id],
  )

  const set = <K extends keyof LineInput>(k: K, v: LineInput[K]) => setDraft((d) => ({ ...d, [k]: v }))
  const setPerf = <K extends keyof Performance>(k: K, v: Performance[K]) =>
    setDraft((d) => ({ ...d, performance: { ...d.performance, [k]: v } }))

  const character = characters.find((c) => c.id === draft.character_id)
  const charPortraits = draft.character_id ? (portraits.get(draft.character_id) ?? []) : []
  const portrait = charPortraits.find((p) => p.id === draft.portrait_id) ?? charPortraits[0]
  const color = draft.character_id ? colors.get(draft.character_id) : undefined
  const id = useId()

  return (
    <>
      {draft.kind === 'dialogue' && (scene.background_path || portrait) && (
        <div className="stage-block">
          <div className="ade-stage" style={{ '--char': color } as React.CSSProperties}>
            {scene.background_path && <img className="ade-stage__bg" src={fileUrl(scene.background_path)} alt="" />}
            {portrait && (
              <img
                className="ade-stage__portrait"
                src={fileUrl(portrait.path)}
                alt={`${character?.name ?? ''} (${portrait.label})`}
              />
            )}
          </div>
          <div className="stage-caption">
            {character && <SpeakerChip name={character.name} color={color} />}
            <p>{draft.text}</p>
          </div>
        </div>
      )}

      <section className="ade-panel" aria-label="セリフの設定">
        <header className="ade-panel__head">
          <h3>
            {draft.kind === 'direction' ? 'ト書き' : 'セリフ'}{' '}
            <span className="ade-timecode">
              {position}/{total}
            </span>
          </h3>
          {update.isPending ? (
            <span style={{ fontSize: 12, color: 'var(--ink-muted)' }}>保存中…</span>
          ) : update.isError ? (
            <span className="error-text">保存できません</span>
          ) : null}
        </header>
        <div className="ade-panel__body">
          {draft.kind === 'dialogue' && (
            <div className="ade-field">
              <label htmlFor={`${id}-speaker`}>話者</label>
              <select
                id={`${id}-speaker`}
                className="ade-input"
                value={draft.character_id ?? ''}
                onChange={(e) => {
                  set('character_id', e.target.value === '' ? null : Number(e.target.value))
                  set('portrait_id', null)
                }}
              >
                <option value="">(話者なし)</option>
                {characters.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </div>
          )}

          {draft.kind === 'dialogue' && charPortraits.length > 1 && (
            <div className="ade-field">
              <span className="ade-section-label">表情</span>
              <div className="ade-portrait-pick" role="group" aria-label="表情">
                {charPortraits.map((p) => (
                  <button
                    key={p.id}
                    type="button"
                    aria-pressed={portrait?.id === p.id}
                    onClick={() => set('portrait_id', p.id)}
                  >
                    <img src={fileUrl(p.path)} alt="" />
                    <span>{p.label}</span>
                  </button>
                ))}
              </div>
            </div>
          )}

          <div className="ade-field">
            <label htmlFor={`${id}-text`}>{draft.kind === 'direction' ? 'ト書き' : 'セリフ'}</label>
            <AutoGrowTextarea
              id={`${id}-text`}
              className="ade-input ade-input--script"
              value={draft.text}
              onChange={(v) => set('text', v)}
            />
          </div>

          {draft.kind === 'dialogue' && (
            <>
              <div className="ade-field">
                <label htmlFor={`${id}-tts`}>読み上げテキスト</label>
                <input
                  id={`${id}-tts`}
                  className="ade-input"
                  placeholder="空ならセリフと同じ"
                  value={draft.tts_text ?? ''}
                  onChange={(e) => set('tts_text', e.target.value || null)}
                />
                <small>読みの修正や、笑い・息づかいの絵文字を入れるとき。</small>
              </div>
              <fieldset style={{ border: 0, margin: 0, padding: 0, display: 'flex', flexDirection: 'column', gap: 4 }}>
                <legend className="ade-section-label" style={{ padding: '0 0 4px' }}>
                  長さ区分
                </legend>
                <div style={{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
                  {(['S', 'M', 'L'] as const).map((c) => (
                    <label key={c} style={{ display: 'inline-flex', gap: 4, alignItems: 'center' }}>
                      <input
                        type="radio"
                        name={`${id}-len`}
                        checked={draft.length_class === c}
                        onChange={() => set('length_class', c)}
                        style={{ accentColor: 'var(--ai)' }}
                      />
                      {c}
                    </label>
                  ))}
                  <label style={{ display: 'inline-flex', gap: 4, alignItems: 'center' }}>
                    <input
                      type="radio"
                      name={`${id}-len`}
                      checked={draft.length_class === null}
                      onChange={() => set('length_class', null)}
                      style={{ accentColor: 'var(--ai)' }}
                    />
                    なし
                  </label>
                  <span className="ade-timecode">
                    {countChars(draft.text)}字
                    {draft.length_class &&
                      ` · ${draft.length_class} は ${LENGTH_RANGES[draft.length_class][0]}〜${LENGTH_RANGES[draft.length_class][1]} 字`}
                  </span>
                </div>
              </fieldset>
              <div className="form-row">
                <div className="ade-field">
                  <label htmlFor={`${id}-pre`}>前の間 (ms)</label>
                  <input
                    id={`${id}-pre`}
                    className="ade-input"
                    type="number"
                    min={0}
                    step={100}
                    value={draft.pause_before_ms}
                    onChange={(e) => set('pause_before_ms', Number(e.target.value) || 0)}
                  />
                </div>
                <div className="ade-field">
                  <label htmlFor={`${id}-post`}>後の間 (ms)</label>
                  <input
                    id={`${id}-post`}
                    className="ade-input"
                    type="number"
                    min={0}
                    step={100}
                    value={draft.pause_after_ms}
                    onChange={(e) => set('pause_after_ms', Number(e.target.value) || 0)}
                  />
                </div>
              </div>
            </>
          )}
        </div>
      </section>

      {draft.kind === 'dialogue' && character && <MemoryPanel lineId={line.id} character={character} />}

      {draft.kind === 'dialogue' && (
        <PerformancePanel
          lineId={line.id}
          performance={draft.performance}
          captionOverride={draft.caption_override}
          dirty={current !== saved}
          setPerf={setPerf}
          setOverride={(v) => set('caption_override', v)}
        />
      )}

      {draft.kind === 'dialogue' && (
        <TakesPanel line={line} sceneId={scene.id} color={color} job={job} onGenerate={onGenerate} />
      )}
    </>
  )
}

function AutoGrowTextarea({
  id,
  className,
  value,
  onChange,
}: {
  id: string
  className: string
  value: string
  onChange: (v: string) => void
}) {
  const ref = useRef<HTMLTextAreaElement>(null)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${el.scrollHeight + 2}px`
  }, [value])
  return (
    <textarea
      ref={ref}
      id={id}
      className={className}
      rows={2}
      value={value}
      style={{ resize: 'none', overflow: 'hidden' }}
      onChange={(e) => onChange(e.target.value)}
    />
  )
}

function PerformancePanel({
  lineId,
  performance: p,
  captionOverride,
  dirty,
  setPerf,
  setOverride,
}: {
  lineId: number
  performance: Performance
  captionOverride: string | null
  dirty: boolean
  setPerf: <K extends keyof Performance>(k: K, v: Performance[K]) => void
  setOverride: (v: string | null) => void
}) {
  const caption = useCaption(lineId)
  const id = useId()
  const overriding = captionOverride !== null

  const select = (k: 'distance' | 'tempo' | 'volume', label: string, options: string[]) => (
    <div className="ade-field">
      <label htmlFor={`${id}-${k}`}>{label}</label>
      <select
        id={`${id}-${k}`}
        className="ade-input"
        value={p[k] ?? ''}
        onChange={(e) => setPerf(k, e.target.value || undefined)}
      >
        <option value="">指定なし</option>
        {options.map((o) => (
          <option key={o}>{o}</option>
        ))}
      </select>
    </div>
  )

  return (
    <section className="ade-panel" aria-label="演技">
      <header className="ade-panel__head">
        <h3>演技</h3>
      </header>
      <div className="ade-panel__body">
        <div className="form-row">
          <div className="ade-field">
            <label htmlFor={`${id}-emotion`}>感情</label>
            <input
              id={`${id}-emotion`}
              className="ade-input"
              placeholder="例: 不満、動揺"
              value={p.emotion ?? ''}
              onChange={(e) => setPerf('emotion', e.target.value || undefined)}
            />
          </div>
          <div className="ade-field">
            <label htmlFor={`${id}-intensity`}>強さ {intensityLabel(p.intensity) ?? '(指定なし)'}</label>
            <input
              id={`${id}-intensity`}
              type="range"
              min={0}
              max={1}
              step={0.1}
              value={p.intensity ?? 0.5}
              onChange={(e) => setPerf('intensity', Number(e.target.value))}
              style={{ accentColor: 'var(--ai)', marginTop: 8, opacity: p.intensity === undefined ? 0.4 : 1 }}
            />
          </div>
          {select('distance', '距離', ['近い', '普通', '遠い'])}
          {select('tempo', 'テンポ', ['ゆっくりした', '速い'])}
          {select('volume', '声量', ['小さい', '大きい'])}
          <div className="ade-field">
            <label htmlFor={`${id}-vq`}>声質</label>
            <input
              id={`${id}-vq`}
              className="ade-input"
              placeholder="例: 震えた、囁く"
              value={p.voice_quality ?? ''}
              onChange={(e) => setPerf('voice_quality', e.target.value || undefined)}
            />
          </div>
        </div>
        <div className="ade-field">
          <label htmlFor={`${id}-note`}>メモ</label>
          <input
            id={`${id}-note`}
            className="ade-input"
            placeholder="例: 語尾は消え入るように"
            value={p.note ?? ''}
            onChange={(e) => setPerf('note', e.target.value || undefined)}
          />
        </div>
        <div className="ade-field">
          <span className="ade-section-label">生成に使う caption {overriding ? '(手動)' : '(自動)'}</span>
          {overriding ? (
            <textarea
              className="ade-input"
              aria-label="caption の手動指定"
              rows={3}
              value={captionOverride}
              onChange={(e) => setOverride(e.target.value)}
            />
          ) : (
            <span className="ade-perf ade-perf--caption" aria-live="polite">
              {dirty ? '保存後に更新…' : caption.data?.caption || '(指定なし。声の基本だけで生成する)'}
            </span>
          )}
          <button
            className="ade-btn ade-btn--quiet ade-btn--sm"
            type="button"
            style={{ alignSelf: 'flex-start' }}
            onClick={() => setOverride(overriding ? null : (caption.data?.caption ?? ''))}
          >
            {overriding ? '自動に戻す' : '手動で上書き'}
          </button>
        </div>
      </div>
    </section>
  )
}

function TakesPanel({
  line,
  sceneId,
  color,
  job,
  onGenerate,
}: {
  line: Line
  sceneId: number
  color?: string
  job?: Job
  onGenerate: () => void
}) {
  const takes = useTakes(line.id)
  const adopt = useAdoptTake(sceneId, line.id)
  const remove = useDeleteTake(sceneId, line.id)
  const list = takes.data ?? []
  const maxDuration = Math.max(0, ...list.map((t) => t.duration_ms))
  const busy = job?.state === 'queued' || job?.state === 'running'

  return (
    <section className="ade-panel" aria-label="テイク">
      <header className="ade-panel__head">
        <h3 style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          テイク
          <CountBadge count={list.length} unit="本" variant="muted" />
        </h3>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          {job && job.state !== 'done' && <JobPill job={job} />}
          <button className="ade-btn ade-btn--sm" type="button" onClick={onGenerate} disabled={busy}>
            <Mic aria-hidden="true" />
            {list.length ? 'もう一度生成' : '音声生成'}
          </button>
        </div>
      </header>
      <div className="ade-panel__body" style={{ gap: 8 }}>
        {list.length === 0 && (
          <p style={{ margin: 0, fontSize: 12, color: 'var(--ink-muted)' }}>
            まだテイクがない。生成すると新しいものから上に並ぶ。
          </p>
        )}
        {list.map((t, i) => (
          <TakeItem
            key={t.id}
            take={t}
            index={list.length - i}
            color={color}
            maxDurationMs={maxDuration}
            onAdopt={() => adopt.mutate(t.id)}
            onDelete={() => remove.mutate(t.id)}
          />
        ))}
        {list.find((t) => t.adopted)?.seed != null && (
          <span className="ade-timecode">
            採用中: seed {list.find((t) => t.adopted)?.seed} · {list.find((t) => t.adopted)?.params.num_steps} steps
          </span>
        )}
      </div>
    </section>
  )
}
