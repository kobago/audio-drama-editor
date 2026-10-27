import { getRouteApi, Link, useNavigate } from '@tanstack/react-router'
import { Pencil, Plus, Upload, X } from 'lucide-react'
import { useId, useRef, useState } from 'react'
import { fileUrl, uploadFile } from '../../api/client'
import {
  toCharacterInput,
  useAddMemory,
  useAddPortrait,
  useCharacter,
  useCharacters,
  useDeleteCharacter,
  useDeleteMemory,
  useDeletePortrait,
  useMemories,
  usePortraits,
  useUpdateCharacter,
  useUpdateMemory,
} from '../../api/hooks'
import type { Character, CharacterInput, LengthClass, Memory, MemoryInput, MemoryKind } from '../../api/types'
import { CountBadge, LengthBadge } from '../../components/badges'
import { DeleteSection } from '../../components/DeleteSection'
import { kindLabel, MEMORY_KINDS, memoryMeta, parsePeople } from '../../lib/memory'
import { characterColors } from '../../lib/script'

export type CharacterTab = 'basic' | 'speech' | 'memories' | 'voice'

const route = getRouteApi('/projects/$projectId/cast/$characterId')

const TAB_LABEL: Record<CharacterTab, string> = {
  basic: '基本',
  speech: '話し方',
  memories: '記憶',
  voice: '声',
}

export function CharacterPage() {
  const { projectId, characterId } = route.useParams()
  const { tab = 'basic' } = route.useSearch()
  const character = useCharacter(characterId)
  const characters = useCharacters(projectId)
  const navigate = useNavigate()

  if (character.isError) return <main className="page">キャラクターを読み込めません: {character.error.message}</main>
  if (!character.data) return <main className="page" aria-busy="true" />

  const color = characterColors(characters.data ?? []).get(characterId)
  const setTab = (t: CharacterTab) =>
    navigate({ to: '/projects/$projectId/cast/$characterId', params: { projectId, characterId }, search: { tab: t } })

  return (
    <CharacterEditor key={character.data.id} character={character.data} color={color} tab={tab} onTab={setTab}>
      {tab === 'memories' && <MemoriesTab character={character.data} />}
    </CharacterEditor>
  )
}

function CharacterEditor({
  character,
  color,
  tab,
  onTab,
  children,
}: {
  character: Character
  color?: string
  tab: CharacterTab
  onTab: (t: CharacterTab) => void
  children?: React.ReactNode
}) {
  const [draft, setDraft] = useState<CharacterInput>(() => toCharacterInput(character))
  const update = useUpdateCharacter(character)
  const portraits = usePortraits(character.id)
  const memories = useMemories(character.id)
  const dirty = JSON.stringify(draft) !== JSON.stringify(toCharacterInput(character))
  const set = <K extends keyof CharacterInput>(k: K, v: CharacterInput[K]) => setDraft((d) => ({ ...d, [k]: v }))
  const firstPortrait = portraits.data?.[0]

  return (
    <main className="page" style={{ maxWidth: 1240, '--char': color } as React.CSSProperties}>
      <div className="char-head">
        {firstPortrait ? (
          <img className="char-head__portrait" src={fileUrl(firstPortrait.path)} alt="" />
        ) : (
          <div className="char-head__portrait" role="img" aria-label="立ち絵 未設定" />
        )}
        <div className="char-head__name">
          <Link to="/projects/$projectId/cast" params={{ projectId: character.project_id }} className="page__back">
            キャスト一覧
          </Link>
          <h1 className="page__title">{draft.name || '名前未設定'}</h1>
        </div>
        <div className="row">
          {update.isError && <span className="error-text">保存できません: {update.error.message}</span>}
          {dirty && <span className="hint">保存していない変更があります</span>}
          <button
            className="ade-btn ade-btn--primary"
            type="button"
            disabled={!dirty || !draft.name.trim() || update.isPending}
            onClick={() => update.mutate(draft)}
          >
            保存
          </button>
        </div>
      </div>

      <div role="tablist" aria-label="キャラクター設定" className="ade-tabs">
        {(Object.keys(TAB_LABEL) as CharacterTab[]).map((t) => (
          <button key={t} role="tab" type="button" className="ade-tab" aria-selected={tab === t} onClick={() => onTab(t)}>
            {TAB_LABEL[t]}
            {t === 'memories' && <CountBadge count={memories.data?.length ?? 0} unit="件" variant="muted" />}
          </button>
        ))}
      </div>

      <div role="tabpanel" aria-label={TAB_LABEL[tab]}>
        {tab === 'basic' && <BasicTab draft={draft} set={set} characterId={character.id} />}
        {tab === 'speech' && <SpeechTab draft={draft} set={set} />}
        {tab === 'voice' && <VoiceTab draft={draft} set={set} />}
        {children}
      </div>
      {tab === 'basic' && <DeleteCharacter character={character} />}
    </main>
  )
}

function DeleteCharacter({ character }: { character: Character }) {
  const remove = useDeleteCharacter(character)
  const navigate = useNavigate()
  return (
    <DeleteSection
      label="キャラクターを削除"
      consequences={`「${character.name}」の記憶・立ち絵・参照音声を削除します。このキャラクターのセリフは話者なしで台本に残ります。`}
      pending={remove.isPending}
      error={remove.error}
      onDelete={() =>
        remove.mutate(undefined, {
          onSuccess: () => navigate({ to: '/projects/$projectId/cast', params: { projectId: character.project_id } }),
        })
      }
    />
  )
}

type Setter = <K extends keyof CharacterInput>(k: K, v: CharacterInput[K]) => void

function BasicTab({ draft, set, characterId }: { draft: CharacterInput; set: Setter; characterId: number }) {
  return (
    <div className="split">
      <div className="split__main">
        <div className="form-row">
          <div className="ade-field">
            <label htmlFor="c-name">名前</label>
            <input id="c-name" className="ade-input" value={draft.name} onChange={(e) => set('name', e.target.value)} />
          </div>
          <div className="ade-field">
            <label htmlFor="c-reading">読み</label>
            <input
              id="c-reading"
              className="ade-input"
              value={draft.reading}
              onChange={(e) => set('reading', e.target.value)}
            />
          </div>
        </div>
        <div className="ade-field">
          <label htmlFor="c-profile">プロフィール</label>
          <textarea
            id="c-profile"
            className="ade-input"
            rows={5}
            value={draft.profile}
            onChange={(e) => set('profile', e.target.value)}
          />
          <small>何者かを短く書きます。経験や知っていることは「記憶」に 1 件ずつ書いてください。</small>
        </div>
      </div>
      <div className="split__side">
        <PortraitManager characterId={characterId} />
      </div>
    </div>
  )
}

function PortraitManager({ characterId }: { characterId: number }) {
  const portraits = usePortraits(characterId)
  const add = useAddPortrait(characterId)
  const remove = useDeletePortrait(characterId)
  const [label, setLabel] = useState('')
  const [error, setError] = useState<string | null>(null)
  const fileInput = useRef<HTMLInputElement>(null)
  const id = useId()

  const onFile = async (file: File | undefined) => {
    if (!file) return
    setError(null)
    try {
      const path = await uploadFile(file)
      await add.mutateAsync({ label: label.trim() || '通常', path })
      setLabel('')
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      if (fileInput.current) fileInput.current.value = ''
    }
  }

  return (
    <section className="ade-panel">
      <header className="ade-panel__head">
        <h3>立ち絵</h3>
      </header>
      <div className="ade-panel__body">
        <div className="portrait-grid">
          {portraits.data?.map((p) => (
            <div key={p.id} className="portrait-tile">
              <img src={fileUrl(p.path)} alt={`立ち絵 ${p.label}`} />
              <div className="row row--between">
                <span className="hint">{p.label}</span>
                <button
                  className="ade-icon-btn"
                  type="button"
                  aria-label={`立ち絵「${p.label}」を削除`}
                  onClick={() => remove.mutate(p.id)}
                >
                  <X aria-hidden="true" />
                </button>
              </div>
            </div>
          ))}
        </div>
        <div className="ade-field">
          <label htmlFor={`${id}-label`}>表情の名前</label>
          <input
            id={`${id}-label`}
            className="ade-input"
            placeholder="通常、不満、驚き など"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
        </div>
        <input
          ref={fileInput}
          id={`${id}-file`}
          type="file"
          accept="image/png,image/jpeg,image/webp,image/gif"
          className="visually-hidden"
          onChange={(e) => onFile(e.target.files?.[0])}
        />
        <div className="row">
          <label htmlFor={`${id}-file`} className="ade-btn">
            <Upload aria-hidden="true" />
            立ち絵を追加
          </label>
        </div>
        <p className="hint">4:5 で上端に合わせて切り抜きます。背景を透過した PNG が向いています。</p>
        {error && <p className="error-text">アップロードできません: {error}</p>}
      </div>
    </section>
  )
}

function SpeechTab({ draft, set }: { draft: CharacterInput; set: Setter }) {
  const [len, setLen] = useState<LengthClass>('M')
  const [text, setText] = useState('')
  const samples = draft.sample_lines

  return (
    <div className="split__main" style={{ maxWidth: 860 }}>
      <div className="ade-field">
        <label htmlFor="c-style">話し方 (システムプロンプトに入る)</label>
        <textarea
          id="c-style"
          className="ade-input"
          rows={6}
          value={draft.speech_style}
          onChange={(e) => set('speech_style', e.target.value)}
        />
        <small>
          どう話すかだけを書きます: 一人称、相手の呼び方、語尾、距離感、使わない言い回し。経歴や設定は「記憶」に書いてください。
        </small>
      </div>

      <section className="stack">
        <div className="row row--between">
          <p className="section-label">サンプルセリフ (長さ区分ごとのお手本)</p>
          <p className="hint">各区分に 2〜3 本あると、長さが安定します。</p>
        </div>
        <div className="list-box">
          {samples.map((s, i) => (
            <div key={i} className="list-row">
              <LengthBadge text={s.text} lengthClass={s.length} />
              <p className="script-text">{s.text}</p>
              <button
                className="ade-icon-btn"
                type="button"
                aria-label={`サンプル「${s.text}」を削除`}
                onClick={() =>
                  set(
                    'sample_lines',
                    samples.filter((_, j) => j !== i),
                  )
                }
              >
                <X aria-hidden="true" />
              </button>
            </div>
          ))}
          <form
            className="list-row"
            style={{ gridTemplateColumns: '5rem minmax(0, 1fr) auto' }}
            onSubmit={(e) => {
              e.preventDefault()
              if (!text.trim()) return
              set('sample_lines', [...samples, { length: len, text: text.trim() }])
              setText('')
            }}
          >
            <select
              className="ade-input"
              aria-label="追加するサンプルの長さ"
              value={len}
              onChange={(e) => setLen(e.target.value as LengthClass)}
            >
              <option>S</option>
              <option>M</option>
              <option>L</option>
            </select>
            <input
              className="ade-input ade-input--script"
              aria-label="追加するサンプルセリフ"
              placeholder="お手本のセリフ"
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
            <button className="ade-btn ade-btn--sm" type="submit" disabled={!text.trim()}>
              <Plus aria-hidden="true" />
              追加
            </button>
          </form>
        </div>
      </section>

      <section className="ade-panel">
        <header className="ade-panel__head">
          <h3>口調の LoRA</h3>
          <span className="hint">llama-server で書くときだけ</span>
        </header>
        <div className="ade-panel__body">
        <div className="form-row">
          <div className="ade-field">
            <label htmlFor="c-lora">アダプター ID</label>
            <input
              id="c-lora"
              className="ade-input"
              inputMode="numeric"
              placeholder="使わない"
              value={draft.lora_id ?? ''}
              onChange={(e) => set('lora_id', e.target.value === '' ? null : Number(e.target.value))}
            />
          </div>
          <div className="ade-field">
            <label htmlFor="c-scale">強さ {draft.lora_scale.toFixed(1)}</label>
            <input
              id="c-scale"
              type="range"
              min={0}
              max={1}
              step={0.1}
              value={draft.lora_scale}
              onChange={(e) => set('lora_scale', Number(e.target.value))}
              style={{ accentColor: 'var(--ai)' }}
            />
          </div>
        </div>
        <p className="hint">OpenCode Go で書くときは使いません。LoRA を使うときも、話し方の設定は短く残してください。</p>
        </div>
      </section>
    </div>
  )
}

function MemoriesTab({ character }: { character: Character }) {
  const memories = useMemories(character.id)
  const add = useAddMemory(character.id)
  const remove = useDeleteMemory(character.id)
  const [editing, setEditing] = useState<number | null>(null)
  const [filter, setFilter] = useState<MemoryKind | null>(null)
  const list = memories.data ?? []
  const shown = filter ? list.filter((m) => m.kind === filter) : list

  return (
    <div className="split__main" style={{ maxWidth: 900 }}>
      <p className="page__lead" style={{ marginTop: 0 }}>
        {character.name || 'このキャラクター'}の経験・思い出・知っていることを 1 件ずつ書きます。セリフを書くとき、
        場面に合う記憶を探して LLM に渡します。いつのことか・誰が関わるかを添えると、探すときの手がかりになります。
      </p>
      <section className="ade-panel" aria-label="記憶を追加">
        <header className="ade-panel__head">
          <h3>記憶を追加</h3>
        </header>
        <div className="ade-panel__body">
          <MemoryForm
            submitLabel="追加"
            busy={add.isPending}
            onSubmit={(input) => add.mutateAsync(input)}
          />
          {add.isError && <span className="error-text">追加できません: {add.error.message}</span>}
        </div>
      </section>

      <div role="group" aria-label="種類で絞り込む" className="ade-tabs ade-tabs--sm">
        <button type="button" className="ade-tab" aria-pressed={filter === null} onClick={() => setFilter(null)}>
          すべて
          <CountBadge count={list.length} unit="件" variant="muted" />
        </button>
        {MEMORY_KINDS.map(({ kind, label }) => {
          const n = list.filter((m) => m.kind === kind).length
          return (
            <button
              key={kind}
              type="button"
              className="ade-tab"
              aria-pressed={filter === kind}
              onClick={() => setFilter(kind)}
            >
              {label}
              <CountBadge count={n} unit="件" variant="muted" />
            </button>
          )
        })}
      </div>

      <div className="list-box" aria-label="記憶の一覧">
        {shown.length === 0 && (
          <p className="hint" style={{ padding: 'var(--space-4)' }}>
            {list.length === 0 ? 'まだ記憶がありません。' : 'この種類の記憶はありません。'}
          </p>
        )}
        {shown.map((m) =>
          editing === m.id ? (
            <div key={m.id} className="ade-memory ade-memory--editing">
              <EditMemory characterId={character.id} memory={m} onDone={() => setEditing(null)} />
            </div>
          ) : (
            <div key={m.id} className="ade-memory">
              <span className="ade-kind">{kindLabel(m.kind)}</span>
              <div className="ade-memory__body">
                {memoryMeta(m) && <span className="ade-memory__meta">{memoryMeta(m)}</span>}
                <span>{m.text}</span>
              </div>
              <div className="ade-memory__actions">
                <button
                  className="ade-icon-btn"
                  type="button"
                  aria-label={`「${m.text}」を編集`}
                  onClick={() => setEditing(m.id)}
                >
                  <Pencil aria-hidden="true" />
                </button>
                <button
                  className="ade-icon-btn"
                  type="button"
                  aria-label={`「${m.text}」を削除`}
                  onClick={() => remove.mutate(m.id)}
                >
                  <X aria-hidden="true" />
                </button>
              </div>
            </div>
          ),
        )}
      </div>
    </div>
  )
}

function EditMemory({ characterId, memory, onDone }: { characterId: number; memory: Memory; onDone: () => void }) {
  const update = useUpdateMemory(characterId)
  return (
    <>
      <MemoryForm
        initial={memory}
        submitLabel="保存"
        busy={update.isPending}
        onSubmit={async (input) => {
          await update.mutateAsync({ id: memory.id, input })
          onDone()
        }}
        onCancel={onDone}
      />
      {update.isError && <span className="error-text">保存できません: {update.error.message}</span>}
    </>
  )
}

function MemoryForm({
  initial,
  submitLabel,
  busy,
  onSubmit,
  onCancel,
}: {
  initial?: MemoryInput
  submitLabel: string
  busy: boolean
  onSubmit: (input: MemoryInput) => Promise<unknown>
  onCancel?: () => void
}) {
  const [kind, setKind] = useState<MemoryKind>(initial?.kind ?? 'recollection')
  const [period, setPeriod] = useState(initial?.period ?? '')
  const [people, setPeople] = useState(initial?.people.join('、') ?? '')
  const [text, setText] = useState(initial?.text ?? '')
  const id = useId()

  return (
    <form
      className="ade-memory-form"
      onSubmit={async (e) => {
        e.preventDefault()
        if (!text.trim()) return
        await onSubmit({ kind, period: period.trim(), people: parsePeople(people), text: text.trim() })
        // 追加フォームは続けて書けるよう空にする (種類はそのまま)
        if (!initial) {
          setPeriod('')
          setPeople('')
          setText('')
        }
      }}
    >
      <div className="form-row">
        <div className="ade-field">
          <label htmlFor={`${id}-kind`}>種類</label>
          <select
            id={`${id}-kind`}
            className="ade-input"
            value={kind}
            onChange={(e) => setKind(e.target.value as MemoryKind)}
          >
            {MEMORY_KINDS.map((k) => (
              <option key={k.kind} value={k.kind}>
                {k.label} — {k.hint}
              </option>
            ))}
          </select>
        </div>
        <div className="ade-field">
          <label htmlFor={`${id}-period`}>いつ</label>
          <input
            id={`${id}-period`}
            className="ade-input"
            placeholder="例: 小学4年の夏"
            value={period}
            onChange={(e) => setPeriod(e.target.value)}
          />
        </div>
        <div className="ade-field">
          <label htmlFor={`${id}-people`}>関わる人物</label>
          <input
            id={`${id}-people`}
            className="ade-input"
            placeholder="例: ボブ、母"
            value={people}
            onChange={(e) => setPeople(e.target.value)}
          />
        </div>
      </div>
      <div className="ade-field">
        <label htmlFor={`${id}-text`}>内容</label>
        <textarea
          id={`${id}-text`}
          className="ade-input"
          rows={2}
          placeholder="例: 夏祭りで、ボブと「来年も一緒に花火を見よう」と約束した。"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <small>1〜2 文で書きます。気持ちや、その後どうなったかも書くと、場面に合わせて思い出しやすくなります。</small>
      </div>
      <div className="row row--end">
        {onCancel && (
          <button className="ade-btn ade-btn--quiet ade-btn--sm" type="button" onClick={onCancel}>
            やめる
          </button>
        )}
        <button className="ade-btn ade-btn--sm" type="submit" disabled={!text.trim() || busy}>
          {!initial && <Plus aria-hidden="true" />}
          {submitLabel}
        </button>
      </div>
    </form>
  )
}

function VoiceTab({ draft, set }: { draft: CharacterInput; set: Setter }) {
  const [error, setError] = useState<string | null>(null)
  const [uploading, setUploading] = useState(false)
  const id = useId()

  return (
    <div className="split__main" style={{ maxWidth: 860 }}>
      <div className="ade-field">
        <label htmlFor="c-voice-caption">声の基本 (音声生成の指示の先頭に入る)</label>
        <input
          id="c-voice-caption"
          className="ade-input"
          placeholder="例: 若い女性の声"
          value={draft.voice_caption}
          onChange={(e) => set('voice_caption', e.target.value)}
        />
        <small>年齢・性別・声質だけを書きます。感情や話し方は、セリフごとの演技で指定します。</small>
      </div>
      <section className="ade-panel">
        <header className="ade-panel__head">
          <h3>参照音声</h3>
        </header>
        <div className="ade-panel__body">
          {draft.voice_ref_path ? (
            <>
              <audio controls src={fileUrl(draft.voice_ref_path)} style={{ width: '100%' }} />
              <div className="row">
                <button
                  className="ade-btn ade-btn--sm ade-btn--quiet"
                  type="button"
                  onClick={() => set('voice_ref_path', null)}
                >
                  参照音声を外す
                </button>
              </div>
            </>
          ) : (
            <p className="hint">
              参照音声はありません。声の基本の文章だけで声を作ります (セリフごとに声が揺れやすくなります)。
            </p>
          )}
          <input
            id={`${id}-ref`}
            type="file"
            accept="audio/wav,.wav"
            className="visually-hidden"
            onChange={async (e) => {
              const file = e.target.files?.[0]
              if (!file) return
              setError(null)
              setUploading(true)
              try {
                set('voice_ref_path', await uploadFile(file))
              } catch (err) {
                setError(err instanceof Error ? err.message : String(err))
              } finally {
                setUploading(false)
              }
            }}
          />
          <div className="row">
            <label htmlFor={`${id}-ref`} className="ade-btn" aria-disabled={uploading}>
              <Upload aria-hidden="true" />
              {draft.voice_ref_path ? '差し替える' : 'WAV をアップロード'}
            </label>
          </div>
          <p className="hint">5〜30 秒の WAV を使います。保存すると、次の音声生成から使います。</p>
          {error && <p className="error-text">アップロードできません: {error}</p>}
        </div>
      </section>
    </div>
  )
}
