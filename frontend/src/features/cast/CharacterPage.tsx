import { getRouteApi, Link, useNavigate } from '@tanstack/react-router'
import { Plus, Upload, X } from 'lucide-react'
import { useId, useRef, useState } from 'react'
import { fileUrl, uploadFile } from '../../api/client'
import {
  toCharacterInput,
  useAddFact,
  useAddPortrait,
  useCharacter,
  useCharacters,
  useDeleteFact,
  useDeletePortrait,
  useFacts,
  usePortraits,
  useUpdateCharacter,
} from '../../api/hooks'
import type { Character, CharacterInput, LengthClass } from '../../api/types'
import { CountBadge, LengthBadge } from '../../components/badges'
import { characterColors } from '../../lib/script'

export type CharacterTab = 'basic' | 'speech' | 'knowledge' | 'voice'

const route = getRouteApi('/projects/$projectId/cast/$characterId')

const TAB_LABEL: Record<CharacterTab, string> = {
  basic: '基本',
  speech: '話し方',
  knowledge: '知っていること',
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
      {tab === 'knowledge' && <KnowledgeTab characterId={characterId} />}
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
  const facts = useFacts(character.id)
  const dirty = JSON.stringify(draft) !== JSON.stringify(toCharacterInput(character))
  const set = <K extends keyof CharacterInput>(k: K, v: CharacterInput[K]) => setDraft((d) => ({ ...d, [k]: v }))
  const firstPortrait = portraits.data?.[0]

  return (
    <main className="page" style={{ maxWidth: 1240, gap: 20 }}>
      <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 16 }}>
        {firstPortrait ? (
          <img
            className="ade-character__portrait"
            src={fileUrl(firstPortrait.path)}
            alt=""
            style={{ '--char': color, width: 72, height: 90 } as React.CSSProperties}
          />
        ) : (
          <div
            className="ade-character__portrait"
            role="img"
            aria-label="立ち絵 未設定"
            style={{ '--char': color, width: 72, height: 90 } as React.CSSProperties}
          />
        )}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4, flexGrow: 1, minWidth: 0 }}>
          <Link to="/projects/$projectId/cast" params={{ projectId: character.project_id }} style={{ fontSize: 12 }}>
            ← キャスト
          </Link>
          <h1 className="page__title">{draft.name || '名前未設定'}</h1>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          {update.isError && <span className="error-text">保存できません: {update.error.message}</span>}
          {dirty && <span style={{ fontSize: 12, color: 'var(--ink-muted)' }}>未保存の変更あり</span>}
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

      <div role="tablist" aria-label="キャラクター設定" className="tabs">
        {(Object.keys(TAB_LABEL) as CharacterTab[]).map((t) => (
          <button key={t} role="tab" type="button" aria-selected={tab === t} onClick={() => onTab(t)}>
            {TAB_LABEL[t]}
            {t === 'knowledge' && <CountBadge count={facts.data?.length ?? 0} unit="件" variant="muted" />}
          </button>
        ))}
      </div>

      <div role="tabpanel" aria-label={TAB_LABEL[tab]}>
        {tab === 'basic' && <BasicTab draft={draft} set={set} characterId={character.id} />}
        {tab === 'speech' && <SpeechTab draft={draft} set={set} />}
        {tab === 'voice' && <VoiceTab draft={draft} set={set} />}
        {children}
      </div>
    </main>
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
          <small>何者かを短く。細かい事実は「知っていること」に 1 件ずつ書く。</small>
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
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                <span style={{ fontSize: 12 }}>{p.label}</span>
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
        <label htmlFor={`${id}-file`} className="ade-btn" style={{ alignSelf: 'flex-start' }}>
          <Upload aria-hidden="true" />
          立ち絵を追加
        </label>
        <small style={{ fontSize: 11, color: 'var(--ink-muted)' }}>
          4:5 で上端に合わせて切り抜く。背景を透過した PNG が向いている。
        </small>
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
          どう話すかだけを書く: 一人称、相手の呼び方、語尾、距離感、使わない言い回し。経歴や設定は「知っていること」へ。
        </small>
      </div>

      <section style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', gap: 8 }}>
          <p className="ade-section-label">サンプルセリフ (長さ区分ごとのお手本)</p>
          <span style={{ fontSize: 12, color: 'var(--ink-muted)' }}>各区分 2〜3 個あると長さが安定する</span>
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
            style={{ gridTemplateColumns: '72px minmax(0, 1fr) auto' }}
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

      <section
        style={{ display: 'flex', flexDirection: 'column', gap: 12, padding: 16, border: '1px solid var(--line)', borderRadius: 'var(--radius-md)' }}
      >
        <p className="ade-section-label">口調の LoRA (llama-server のときだけ)</p>
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
        <small style={{ fontSize: 11, color: 'var(--ink-muted)' }}>
          OpenCode Go で生成するときは使われない。LoRA を使うときも、話し方の設定は短く残す。
        </small>
      </section>
    </div>
  )
}

function KnowledgeTab({ characterId }: { characterId: number }) {
  const facts = useFacts(characterId)
  const add = useAddFact(characterId)
  const remove = useDeleteFact(characterId)
  const [text, setText] = useState('')

  return (
    <div className="split__main" style={{ maxWidth: 860 }}>
      <p className="page__lead" style={{ margin: 0 }}>
        このキャラクターが知っていることを 1 件 1 事実で書く。セリフを書くときに関係する事実だけを LLM に渡す。
      </p>
      <div className="list-box">
        {facts.data?.map((f) => (
          <div key={f.id} className="list-row" style={{ gridTemplateColumns: 'minmax(0, 1fr) auto' }}>
            <span>{f.text}</span>
            <button
              className="ade-icon-btn"
              type="button"
              aria-label={`「${f.text}」を削除`}
              onClick={() => remove.mutate(f.id)}
            >
              <X aria-hidden="true" />
            </button>
          </div>
        ))}
        <form
          className="list-row"
          style={{ gridTemplateColumns: 'minmax(0, 1fr) auto' }}
          onSubmit={async (e) => {
            e.preventDefault()
            if (!text.trim()) return
            await add.mutateAsync(text.trim())
            setText('')
          }}
        >
          <input
            className="ade-input"
            aria-label="追加する事実"
            placeholder="例: ボブとは小学校からの幼なじみ"
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
          <button className="ade-btn ade-btn--sm" type="submit" disabled={!text.trim() || add.isPending}>
            <Plus aria-hidden="true" />
            追加
          </button>
        </form>
      </div>
    </div>
  )
}

function VoiceTab({ draft, set }: { draft: CharacterInput; set: Setter }) {
  const [error, setError] = useState<string | null>(null)
  const [uploading, setUploading] = useState(false)
  const id = useId()

  return (
    <div className="split__main" style={{ maxWidth: 860 }}>
      <div className="ade-field">
        <label htmlFor="c-voice-caption">声の基本 (VoiceDesign の caption の先頭に入る)</label>
        <input
          id="c-voice-caption"
          className="ade-input"
          placeholder="例: 若い女性の声"
          value={draft.voice_caption}
          onChange={(e) => set('voice_caption', e.target.value)}
        />
        <small>年齢・性別・声質だけを書く。感情や話し方はセリフごとの演技で指定する。</small>
      </div>
      <section className="ade-panel">
        <header className="ade-panel__head">
          <h3>参照音声</h3>
        </header>
        <div className="ade-panel__body">
          {draft.voice_ref_path ? (
            <>
              <audio controls src={fileUrl(draft.voice_ref_path)} style={{ width: '100%' }} />
              <button
                className="ade-btn ade-btn--sm ade-btn--quiet"
                type="button"
                style={{ alignSelf: 'flex-start' }}
                onClick={() => set('voice_ref_path', null)}
              >
                参照音声を外す
              </button>
            </>
          ) : (
            <p style={{ margin: 0, color: 'var(--ink-muted)' }}>
              参照音声なし。声の基本の文章だけで声を作る (セリフごとに声が揺れやすい)。
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
          <label htmlFor={`${id}-ref`} className="ade-btn" style={{ alignSelf: 'flex-start' }} aria-disabled={uploading}>
            <Upload aria-hidden="true" />
            {draft.voice_ref_path ? '差し替える' : 'WAV をアップロード'}
          </label>
          <small style={{ fontSize: 11, color: 'var(--ink-muted)' }}>
            5〜30 秒の WAV。保存すると、次の音声生成から使われる。
          </small>
          {error && <p className="error-text">アップロードできません: {error}</p>}
        </div>
      </section>
    </div>
  )
}
