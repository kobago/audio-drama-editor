import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { api } from './api/client'
import type { Project } from './api/types'

export default function App() {
  const qc = useQueryClient()
  const projects = useQuery({ queryKey: ['projects'], queryFn: () => api<Project[]>('/projects') })
  const [title, setTitle] = useState('')
  const create = useMutation({
    mutationFn: (title: string) => api<Project>('/projects', { method: 'POST', body: JSON.stringify({ title }) }),
    onSuccess: () => {
      setTitle('')
      qc.invalidateQueries({ queryKey: ['projects'] })
    },
  })

  return (
    <main className="container">
      <h1>Audio Drama Editor</h1>
      <form
        onSubmit={(e) => {
          e.preventDefault()
          if (title.trim()) create.mutate(title.trim())
        }}
      >
        <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="作品タイトル" />
        <button type="submit" disabled={create.isPending}>
          新規作成
        </button>
      </form>
      {projects.isError && <p role="alert">{projects.error.message}</p>}
      <ul>
        {projects.data?.map((p) => (
          <li key={p.id}>{p.title}</li>
        ))}
      </ul>
    </main>
  )
}
