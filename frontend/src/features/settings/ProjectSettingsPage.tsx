import { getRouteApi } from '@tanstack/react-router'
import { useState } from 'react'
import { useProject, useUpdateProject } from '../../api/hooks'
import type { Project } from '../../api/types'

const route = getRouteApi('/projects/$projectId/settings')

export function ProjectSettingsPage() {
  const { projectId } = route.useParams()
  const project = useProject(projectId)
  return (
    <main className="page page--narrow">
      <h1 className="page__title">作品情報</h1>
      {project.isError && <p className="error-text">作品を読み込めません: {project.error.message}</p>}
      {project.data && <ProjectForm key={project.data.id} project={project.data} />}
    </main>
  )
}

function ProjectForm({ project }: { project: Project }) {
  const update = useUpdateProject(project.id)
  const [title, setTitle] = useState(project.title)
  const [synopsis, setSynopsis] = useState(project.synopsis)

  return (
    <form
      className="stack"
      style={{ gap: 'var(--space-4)' }}
      onSubmit={(e) => {
        e.preventDefault()
        update.mutate({ title: title.trim(), synopsis })
      }}
    >
      <div className="ade-field">
        <label htmlFor="p-title">タイトル</label>
        <input id="p-title" className="ade-input" value={title} onChange={(e) => setTitle(e.target.value)} required />
      </div>
      <div className="ade-field">
        <label htmlFor="p-synopsis">あらすじ</label>
        <textarea
          id="p-synopsis"
          className="ade-input"
          rows={6}
          value={synopsis}
          onChange={(e) => setSynopsis(e.target.value)}
        />
        <small>作品全体の前提。LLM が続きを書くときに渡す。</small>
      </div>
      {update.isError && <p className="error-text">保存できません: {update.error.message}</p>}
      <div className="row">
        <button className="ade-btn ade-btn--primary" type="submit" disabled={!title.trim() || update.isPending}>
          保存
        </button>
        {update.isSuccess && !update.isPending && <span className="ade-adopted">保存しました</span>}
      </div>
    </form>
  )
}
