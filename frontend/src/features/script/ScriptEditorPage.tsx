import { getRouteApi, Link, useNavigate } from '@tanstack/react-router'
import { ArrowDown, ArrowUp, Mic, Sparkles, Trash2, Wand2 } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { fileUrl } from '../../api/client'
import {
  useCharacters,
  useCreateLine,
  useDeleteLine,
  useDirect,
  useLines,
  useProjectPortraits,
  useReorderLines,
  useScenes,
} from '../../api/hooks'
import { useJobs } from '../../api/jobs-context'
import type { Line } from '../../api/types'
import { CountBadge } from '../../components/badges'
import { characterColors } from '../../lib/script'
import { Composer } from './Composer'
import { GeneratePanel } from './GeneratePanel'
import { LineInspector } from './LineInspector'
import { SceneHeader, SceneSidebar } from './SceneParts'
import { ScriptLineRow, type Speaker } from './ScriptLineRow'

const route = getRouteApi('/projects/$projectId/')

export function ScriptEditorPage() {
  const { projectId } = route.useParams()
  const search = route.useSearch()
  const navigate = useNavigate()
  const scenes = useScenes(projectId)
  const characters = useCharacters(projectId)
  const portraits = useProjectPortraits(characters.data)
  const colors = useMemo(() => characterColors(characters.data ?? []), [characters.data])

  const scene = scenes.data?.find((s) => s.id === search.scene) ?? scenes.data?.[0]
  const goToScene = (id: number) =>
    navigate({ to: '/projects/$projectId', params: { projectId }, search: { scene: id } })

  if (scenes.isError) return <main className="page">シーンを読み込めません: {scenes.error.message}</main>

  return (
    <div className="editor">
      <SceneSidebar
        projectId={projectId}
        scenes={scenes.data ?? []}
        currentId={scene?.id}
        onCreated={(s) => goToScene(s.id)}
      />
      {scene ? (
        <SceneEditor
          key={scene.id}
          scene={scene}
          index={scenes.data?.indexOf(scene) ?? 0}
          characters={characters.data ?? []}
          portraits={portraits}
          colors={colors}
        />
      ) : (
        <main className="editor__script">
          <div className="editor__script-inner">
            {scenes.isSuccess && (
              <div className="empty">
                <strong>まだシーンがありません</strong>
                <span>左の「シーンを追加」から始める。</span>
                {characters.data?.length === 0 && (
                  <span>
                    先に<Link to="/projects/$projectId/cast" params={{ projectId }}>キャスト</Link>
                    を登録しておくと、セリフの話者を選べる。
                  </span>
                )}
              </div>
            )}
          </div>
        </main>
      )}
    </div>
  )
}

function SceneEditor({
  scene,
  index,
  characters,
  portraits,
  colors,
}: {
  scene: NonNullable<ReturnType<typeof useScenes>['data']>[number]
  index: number
  characters: NonNullable<ReturnType<typeof useCharacters>['data']>
  portraits: ReturnType<typeof useProjectPortraits>
  colors: Map<number, string>
}) {
  const lines = useLines(scene.id)
  const createLine = useCreateLine(scene.id)
  const deleteLine = useDeleteLine(scene.id)
  const reorder = useReorderLines(scene.id)
  const direct = useDirect(scene.id)
  const jobs = useJobs()
  const list = useMemo(() => lines.data ?? [], [lines.data])

  const [focusedId, setFocusedId] = useState<number | null>(null)
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [generateError, setGenerateError] = useState<string | null>(null)
  const [writing, setWriting] = useState(false)
  const listRef = useRef<HTMLDivElement>(null)

  const focused = list.find((l) => l.id === focusedId) ?? null
  const focusedIndex = focused ? list.indexOf(focused) : -1

  // 選択中のセリフ、なければ編集中のセリフを音声生成の対象にする
  const targets = useMemo(() => {
    const picked = list.filter((l) => l.kind === 'dialogue' && selected.has(l.id))
    if (picked.length) return picked
    return focused?.kind === 'dialogue' ? [focused] : []
  }, [list, selected, focused])

  // 演技を付ける対象: 選択中のセリフ、なければ演技がまだないセリフ全部
  const directTargets = useMemo(() => {
    const written = list.filter((l) => l.kind === 'dialogue' && l.text.trim() && l.gen_state !== 'generating')
    const picked = written.filter((l) => selected.has(l.id))
    if (picked.length) return picked
    return written.filter((l) => Object.keys(l.performance).length === 0)
  }, [list, selected])

  const speakerOf = useCallback(
    (line: Line): Speaker | undefined => {
      const c = characters.find((ch) => ch.id === line.character_id)
      if (!c) return undefined
      const ps = portraits.get(c.id) ?? []
      const p = ps.find((x) => x.id === line.portrait_id) ?? ps[0]
      return { name: c.name, color: colors.get(c.id), avatar: p ? fileUrl(p.path) : undefined }
    },
    [characters, portraits, colors],
  )

  const generate = useCallback(
    async (ids: number[]) => {
      if (!ids.length) return
      setGenerateError(null)
      try {
        await jobs.enqueue(ids)
        setSelected(new Set())
      } catch (e) {
        setGenerateError(e instanceof Error ? e.message : String(e))
      }
    },
    [jobs],
  )

  const move = (delta: -1 | 1) => {
    if (focusedIndex < 0) return
    const j = focusedIndex + delta
    if (j < 0 || j >= list.length) return
    const ids = list.map((l) => l.id)
    ;[ids[focusedIndex], ids[j]] = [ids[j], ids[focusedIndex]]
    reorder.mutate(ids)
  }

  const toggle = (id: number) =>
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

  // キーボード: ↑↓ で行移動、Space で選択、⌘/Ctrl+Enter で音声生成
  const onKeyDown = (e: React.KeyboardEvent) => {
    const target = e.target as HTMLElement
    if (target.closest('input:not([type=checkbox]), textarea, select')) return
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const next = list[Math.min(list.length - 1, Math.max(0, focusedIndex + (e.key === 'ArrowDown' ? 1 : -1)))]
      if (next) setFocusedId(next.id)
    } else if (e.key === ' ' && focused?.kind === 'dialogue') {
      e.preventDefault()
      toggle(focused.id)
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      void generate(targets.map((l) => l.id))
    }
  }

  // フォーカス行を画面内に保つ
  useEffect(() => {
    if (focusedId === null) return
    listRef.current
      ?.querySelector(`[data-line-id="${focusedId}"]`)
      ?.scrollIntoView?.({ block: 'nearest' })
  }, [focusedId])

  const activeJobs = [...jobs.byLine.values()].filter((j) => j.state === 'queued' || j.state === 'running').length
  const appearing = [...new Set(list.map((l) => l.character_id).filter((x): x is number => x !== null))]
    .map((id) => characters.find((c) => c.id === id)?.name)
    .filter((n): n is string => !!n)
  const lastSpeaker = [...list].reverse().find((l) => l.kind === 'dialogue')?.character_id ?? null

  return (
    <>
      <main className="editor__script">
        <div className="editor__script-inner">
          <SceneHeader scene={scene} index={index} characterNames={appearing} />

          <div role="toolbar" aria-label="台本の操作" className="toolbar">
            <div className="toolbar__group">
              <button
                className="ade-btn"
                type="button"
                aria-expanded={writing}
                onClick={() => setWriting((w) => !w)}
              >
                <Sparkles aria-hidden="true" />
                続きを書く
              </button>
              <button
                className="ade-btn"
                type="button"
                disabled={directTargets.length === 0 || direct.isPending}
                title={selected.size ? '選んだセリフに演技を付ける' : '演技がまだないセリフに演技を付ける'}
                onClick={() => direct.mutate(directTargets.map((l) => l.id))}
              >
                <Wand2 aria-hidden="true" />
                {direct.isPending ? '演技を考えています…' : '演技を付ける'}
                <CountBadge count={direct.isPending ? 0 : directTargets.length} unit="行" variant="muted" />
              </button>
              <span className="toolbar__sep" aria-hidden="true" />
              <button
                className="ade-btn ade-btn--primary"
                type="button"
                disabled={targets.length === 0}
                onClick={() => void generate(targets.map((l) => l.id))}
              >
                <Mic aria-hidden="true" />
                音声生成
                <CountBadge count={targets.length} unit="行" />
              </button>
              {selected.size > 0 && (
                <button className="ade-btn ade-btn--quiet ade-btn--sm" type="button" onClick={() => setSelected(new Set())}>
                  選択を解除
                </button>
              )}
              {activeJobs > 0 && (
                <span className="ade-job ade-job--running">
                  音声を生成中
                  <CountBadge count={activeJobs} unit="件" variant="muted" />
                </span>
              )}
            </div>
            <span className="toolbar__spacer" />
            {focused && (
              <div className="toolbar__group" aria-label="編集中の行">
                <button
                  className="ade-icon-btn"
                  type="button"
                  aria-label="編集中の行を上へ"
                  disabled={focusedIndex <= 0}
                  onClick={() => move(-1)}
                >
                  <ArrowUp aria-hidden="true" />
                </button>
                <button
                  className="ade-icon-btn"
                  type="button"
                  aria-label="編集中の行を下へ"
                  disabled={focusedIndex >= list.length - 1}
                  onClick={() => move(1)}
                >
                  <ArrowDown aria-hidden="true" />
                </button>
                {confirmDelete ? (
                  <>
                    <button
                      className="ade-btn ade-btn--sm ade-btn--danger"
                      type="button"
                      onClick={() => {
                        deleteLine.mutate(focused.id)
                        setFocusedId(null)
                        setConfirmDelete(false)
                      }}
                    >
                      この行を削除
                    </button>
                    <button
                      className="ade-btn ade-btn--sm ade-btn--quiet"
                      type="button"
                      onClick={() => setConfirmDelete(false)}
                    >
                      やめる
                    </button>
                  </>
                ) : (
                  <button
                    className="ade-icon-btn"
                    type="button"
                    aria-label="編集中の行を削除"
                    onClick={() => setConfirmDelete(true)}
                  >
                    <Trash2 aria-hidden="true" />
                  </button>
                )}
              </div>
            )}
          </div>
          {generateError && (
            <p className="error-text" role="alert">
              音声を生成できません: {generateError}
            </p>
          )}
          {direct.isError && (
            <p className="error-text" role="alert">
              {direct.error.message}
            </p>
          )}
          {writing && (
            <GeneratePanel
              sceneId={scene.id}
              after={focused}
              afterLabel={focused ? `${speakerOf(focused)?.name ?? 'ト書き'}「${focused.text}」` : undefined}
              onGenerated={(g) => {
                // 入れた最後の行を選ぶ (もう一度「書く」と、その続きになる)
                const last = g.lines[g.lines.length - 1]
                if (last) setFocusedId(last.id)
              }}
              onClose={() => setWriting(false)}
            />
          )}

          <div className="ade-sheet">
          <div
            ref={listRef}
            className="lines"
            role="list"
            aria-label="台本"
            tabIndex={0}
            onKeyDown={onKeyDown}
          >
            {list.map((line, i) => (
              <ScriptLineRow
                key={line.id}
                line={line}
                index={i}
                speaker={speakerOf(line)}
                focused={line.id === focusedId}
                selected={selected.has(line.id)}
                job={jobs.byLine.get(line.id)}
                onFocus={() => {
                  setFocusedId(line.id)
                  setConfirmDelete(false)
                  listRef.current?.focus({ preventScroll: true })
                }}
                onToggleSelect={() => toggle(line.id)}
                onRetry={() => void generate([line.id])}
              />
            ))}
            {lines.isSuccess && list.length === 0 && (
              <p className="hint" style={{ padding: 'var(--space-6) var(--space-4)' }}>
                まだセリフがない。下の欄に書いて Enter で足すか、「続きを書く」で LLM に書かせる。
              </p>
            )}
          </div>

          <Composer
            key={characters.map((c) => c.id).join()}
            characters={characters}
            lastSpeakerId={lastSpeaker}
            pending={createLine.isPending}
            nextNumber={list.length + 1}
            onAdd={async (input) => {
              const line = await createLine.mutateAsync(input)
              setFocusedId(line.id)
            }}
          />
          </div>
        </div>
      </main>

      <aside className="editor__inspector" aria-label="インスペクタ">
        {focused ? (
          <LineInspector
            key={focused.id}
            line={focused}
            position={focusedIndex + 1}
            total={list.length}
            scene={scene}
            characters={characters}
            portraits={portraits}
            colors={colors}
            job={jobs.byLine.get(focused.id)}
            onGenerate={() => void generate([focused.id])}
          />
        ) : (
          <div className="empty">
            <strong>行を選ぶと、ここで編集できる</strong>
            <span>セリフ・意図・記憶・演技・テイクをまとめて扱う。</span>
          </div>
        )}
      </aside>
    </>
  )
}
