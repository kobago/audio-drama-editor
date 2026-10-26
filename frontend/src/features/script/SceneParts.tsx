import { Link } from '@tanstack/react-router'
import { ImagePlus, Pencil, Plus, X } from 'lucide-react'
import { useId, useState } from 'react'
import { fileUrl, uploadFile } from '../../api/client'
import { toSceneInput, useCreateScene, useUpdateScene } from '../../api/hooks'
import type { Scene } from '../../api/types'

export function SceneSidebar({
  projectId,
  scenes,
  currentId,
  onCreated,
}: {
  projectId: number
  scenes: Scene[]
  currentId: number | undefined
  onCreated: (scene: Scene) => void
}) {
  const create = useCreateScene(projectId)
  const [title, setTitle] = useState('')
  const [adding, setAdding] = useState(false)

  return (
    <aside className="editor__scenes" aria-label="シーン">
      <p className="ade-section-label" style={{ padding: '0 8px 8px' }}>
        シーン
      </p>
      <nav style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
        {scenes.map((s, i) => (
          <Link
            key={s.id}
            to="/projects/$projectId"
            params={{ projectId }}
            search={{ scene: s.id }}
            className="scene-item"
            data-current={s.id === currentId}
          >
            {s.background_path ? (
              <img className="scene-item__thumb" src={fileUrl(s.background_path)} alt="" />
            ) : (
              <span className="scene-item__thumb" />
            )}
            <span className="scene-item__title">
              {i + 1}. {s.title || '無題のシーン'}
            </span>
          </Link>
        ))}
      </nav>
      {adding ? (
        <form
          style={{ display: 'flex', flexDirection: 'column', gap: 8, padding: 8 }}
          onSubmit={async (e) => {
            e.preventDefault()
            const scene = await create.mutateAsync({ title: title.trim(), sort_order: scenes.length })
            setTitle('')
            setAdding(false)
            onCreated(scene)
          }}
        >
          <label htmlFor="new-scene" className="visually-hidden">
            シーン名
          </label>
          <input
            id="new-scene"
            className="ade-input"
            placeholder="シーン名 (例: 放課後の屋上)"
            value={title}
            autoFocus
            onChange={(e) => setTitle(e.target.value)}
          />
          <div style={{ display: 'flex', gap: 8 }}>
            <button className="ade-btn ade-btn--sm ade-btn--primary" type="submit" disabled={create.isPending}>
              追加
            </button>
            <button className="ade-btn ade-btn--sm ade-btn--quiet" type="button" onClick={() => setAdding(false)}>
              キャンセル
            </button>
          </div>
        </form>
      ) : (
        <button
          className="ade-btn ade-btn--quiet ade-btn--sm"
          type="button"
          style={{ alignSelf: 'flex-start', marginTop: 8 }}
          onClick={() => setAdding(true)}
        >
          <Plus aria-hidden="true" />
          シーンを追加
        </button>
      )}
    </aside>
  )
}

/** シーンの見出し。背景画像があれば敷き、文字は必ず札の上に置く。編集ボタンで設定を開く。 */
export function SceneHeader({ scene, characterNames }: { scene: Scene; characterNames: string[] }) {
  const [editing, setEditing] = useState(false)
  const meta = [scene.setting, characterNames.join('、')].filter(Boolean).join(' · ')

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <header className={scene.background_path ? 'ade-scene ade-scene--image' : 'ade-scene'}>
        {scene.background_path && <img className="ade-scene__bg" src={fileUrl(scene.background_path)} alt="" />}
        <div className="ade-scene__caption">
          <h1 className="ade-scene__title">{scene.title || '無題のシーン'}</h1>
          {meta && <p className="ade-scene__setting">{meta}</p>}
        </div>
        <button
          className="ade-btn ade-btn--sm"
          type="button"
          style={{ position: 'absolute', top: 12, right: 12 }}
          aria-expanded={editing}
          onClick={() => setEditing((v) => !v)}
        >
          <Pencil aria-hidden="true" />
          シーン設定
        </button>
      </header>
      {editing && <SceneSettings key={scene.id} scene={scene} onClose={() => setEditing(false)} />}
    </div>
  )
}

function SceneSettings({ scene, onClose }: { scene: Scene; onClose: () => void }) {
  const update = useUpdateScene(scene)
  const [draft, setDraft] = useState(() => toSceneInput(scene))
  const [uploadError, setUploadError] = useState<string | null>(null)
  const id = useId()

  return (
    <form
      className="ade-panel"
      onSubmit={async (e) => {
        e.preventDefault()
        await update.mutateAsync(draft)
        onClose()
      }}
    >
      <div className="ade-panel__body">
        <div className="form-row">
          <div className="ade-field">
            <label htmlFor={`${id}-title`}>シーン名</label>
            <input
              id={`${id}-title`}
              className="ade-input"
              value={draft.title}
              onChange={(e) => setDraft({ ...draft, title: e.target.value })}
            />
          </div>
          <div className="ade-field">
            <label htmlFor={`${id}-setting`}>時間・場所・状況</label>
            <input
              id={`${id}-setting`}
              className="ade-input"
              placeholder="例: 夕方・晴れ"
              value={draft.setting}
              onChange={(e) => setDraft({ ...draft, setting: e.target.value })}
            />
          </div>
        </div>
        <div className="ade-field">
          <label htmlFor={`${id}-summary`}>このシーンで起きること</label>
          <textarea
            id={`${id}-summary`}
            className="ade-input"
            rows={3}
            value={draft.summary}
            onChange={(e) => setDraft({ ...draft, summary: e.target.value })}
          />
          <small>LLM が続きを書くときの前提になる。</small>
        </div>
        <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 8 }}>
          <input
            id={`${id}-bg`}
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            className="visually-hidden"
            onChange={async (e) => {
              const file = e.target.files?.[0]
              if (!file) return
              setUploadError(null)
              try {
                setDraft({ ...draft, background_path: await uploadFile(file) })
              } catch (err) {
                setUploadError(err instanceof Error ? err.message : String(err))
              }
            }}
          />
          <label htmlFor={`${id}-bg`} className="ade-btn ade-btn--sm">
            <ImagePlus aria-hidden="true" />
            {draft.background_path ? '背景を差し替える' : '背景画像を設定'}
          </label>
          {draft.background_path && (
            <button
              className="ade-btn ade-btn--sm ade-btn--quiet"
              type="button"
              onClick={() => setDraft({ ...draft, background_path: null })}
            >
              <X aria-hidden="true" />
              背景を外す
            </button>
          )}
          <span style={{ fontSize: 11, color: 'var(--ink-muted)' }}>16:9 で切り抜く</span>
        </div>
        {uploadError && <p className="error-text">アップロードできません: {uploadError}</p>}
        {update.isError && <p className="error-text">保存できません: {update.error.message}</p>}
        <div style={{ display: 'flex', gap: 8 }}>
          <button className="ade-btn ade-btn--primary" type="submit" disabled={update.isPending}>
            保存
          </button>
          <button className="ade-btn ade-btn--quiet" type="button" onClick={onClose}>
            キャンセル
          </button>
        </div>
      </div>
    </form>
  )
}
