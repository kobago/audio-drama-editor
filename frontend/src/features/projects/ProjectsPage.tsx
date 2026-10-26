import { Link, useNavigate } from '@tanstack/react-router'
import { Plus } from 'lucide-react'
import { useState } from 'react'
import { useCreateProject, useProjects } from '../../api/hooks'
import { AppBar } from '../../components/AppBar'

export function ProjectsPage() {
  const projects = useProjects()
  const [creating, setCreating] = useState(false)

  return (
    <>
      <AppBar />
      <main className="page">
        <div className="page__head">
          <div>
            <h1 className="page__title">作品</h1>
            <p className="page__lead">ドラマ 1 本ごとに、キャスト・シーン・台本・テイクをまとめて持つ。</p>
          </div>
        </div>

        {creating && <NewProjectForm onCancel={() => setCreating(false)} />}
        {projects.isError && <p className="error-text">作品を読み込めません: {projects.error.message}</p>}

        <div className="covers">
          {projects.data?.map((p) => (
            <Link key={p.id} to="/projects/$projectId" params={{ projectId: p.id }} className="cover">
              <span className="cover__label">台本</span>
              <h2 className="cover__title">{p.title}</h2>
              {p.synopsis && <p className="cover__synopsis">{p.synopsis}</p>}
              <span className="cover__foot ade-timecode">{p.updated_at.slice(0, 10).replaceAll('-', '.')} 更新</span>
            </Link>
          ))}
          {!creating && (
            <button className="cover cover--new" type="button" onClick={() => setCreating(true)}>
              <Plus aria-hidden="true" />
              新しい作品
            </button>
          )}
        </div>
      </main>
    </>
  )
}

function NewProjectForm({ onCancel }: { onCancel: () => void }) {
  const create = useCreateProject()
  const navigate = useNavigate()
  const [title, setTitle] = useState('')
  const [synopsis, setSynopsis] = useState('')

  return (
    <form
      className="ade-panel"
      onSubmit={async (e) => {
        e.preventDefault()
        if (!title.trim()) return
        const p = await create.mutateAsync({ title: title.trim(), synopsis: synopsis.trim() })
        await navigate({ to: '/projects/$projectId', params: { projectId: p.id } })
      }}
    >
      <header className="ade-panel__head">
        <h3>新しい作品</h3>
      </header>
      <div className="ade-panel__body">
        <div className="ade-field">
          <label htmlFor="new-title">タイトル</label>
          <input
            id="new-title"
            className="ade-input"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            autoFocus
            required
          />
        </div>
        <div className="ade-field">
          <label htmlFor="new-synopsis">あらすじ</label>
          <textarea
            id="new-synopsis"
            className="ade-input"
            rows={3}
            value={synopsis}
            onChange={(e) => setSynopsis(e.target.value)}
          />
          <small>LLM がセリフを書くときの前提になる。あとから「設定」で直せる。</small>
        </div>
        {create.isError && <p className="error-text">作成できません: {create.error.message}</p>}
        <div className="row">
          <button className="ade-btn ade-btn--primary" type="submit" disabled={!title.trim() || create.isPending}>
            作成
          </button>
          <button className="ade-btn ade-btn--quiet" type="button" onClick={onCancel}>
            キャンセル
          </button>
        </div>
      </div>
    </form>
  )
}
