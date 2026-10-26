import { getRouteApi, Link, useNavigate } from '@tanstack/react-router'
import { ArrowDown, ArrowUp, Mic, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { fileUrl } from '../../api/client'
import {
  useCharacters,
  useCreateLine,
  useDeleteLine,
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
  characters,
  portraits,
  colors,
}: {
  scene: NonNullable<ReturnType<typeof useScenes>['data']>[number]
  characters: NonNullable<ReturnType<typeof useCharacters>['data']>
  portraits: ReturnType<typeof useProjectPortraits>
  colors: Map<number, string>
}) {
  const lines = useLines(scene.id)
  const createLine = useCreateLine(scene.id)
  const deleteLine = useDeleteLine(scene.id)
  const reorder = useReorderLines(scene.id)
  const jobs = useJobs()
  const list = useMemo(() => lines.data ?? [], [lines.data])

  const [focusedId, setFocusedId] = useState<number | null>(null)
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [generateError, setGenerateError] = useState<string | null>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const focused = list.find((l) => l.id === focusedId) ?? null
  const focusedIndex = focused ? list.indexOf(focused) : -1

  // 選択中のセリフ、なければ編集中のセリフを音声生成の対象にする
  const targets = useMemo(() => {
    const picked = list.filter((l) => l.kind === 'dialogue' && selected.has(l.id))
    if (picked.length) return picked
    return focused?.kind === 'dialogue' ? [focused] : []
  }, [list, selected, focused])

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
          <SceneHeader scene={scene} characterNames={appearing} />

          <div role="toolbar" aria-label="台本の操作" className="toolbar">
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
            <span className="toolbar__spacer" />
            {focused && (
              <>
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
              </>
            )}
            {activeJobs > 0 && (
              <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, fontSize: 12, fontWeight: 700 }}>
                生成キュー
                <CountBadge count={activeJobs} unit="件" variant="muted" />
              </span>
            )}
          </div>
          {generateError && (
            <p className="error-text" role="alert">
              音声を生成できません: {generateError}
            </p>
          )}

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
              <div className="empty">
                <strong>まだセリフがありません</strong>
                <span>下の欄に書いて Enter で追加する。</span>
              </div>
            )}
          </div>

          <Composer
            key={characters.map((c) => c.id).join()}
            characters={characters}
            lastSpeakerId={lastSpeaker}
            pending={createLine.isPending}
            onAdd={async (input) => {
              const line = await createLine.mutateAsync(input)
              setFocusedId(line.id)
            }}
          />
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
            <span>セリフ・演技・テイクをまとめて扱う。</span>
          </div>
        )}
      </aside>
    </>
  )
}
