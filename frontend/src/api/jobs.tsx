import { useQueryClient } from '@tanstack/react-query'
import { type ReactNode, useCallback, useEffect, useMemo, useState } from 'react'
import { keys, useCreateJobs } from './hooks'
import { JobsContext, type JobsContextValue } from './jobs-context'
import type { Job } from './types'

/** 完了したジョブの表示を残す時間。その後は採用テイクの表示に切り替わる。 */
const DONE_LINGER_MS = 4000

/** `/api/tts/events` (SSE) を購読して、音声生成ジョブの状態を行ごとに持つ。 */
export function JobsProvider({ children }: { children: ReactNode }) {
  const qc = useQueryClient()
  const [byLine, setByLine] = useState<Map<number, Job>>(new Map())
  const create = useCreateJobs()

  const upsert = useCallback((job: Job) => {
    setByLine((prev) => {
      const current = prev.get(job.line_id)
      // 古いジョブの通知で新しいジョブの表示を上書きしない
      if (current && current.id !== job.id && current.state !== 'done' && current.state !== 'failed') {
        if (job.state === 'done' || job.state === 'failed') return prev
      }
      const next = new Map(prev)
      next.set(job.line_id, job)
      return next
    })
  }, [])

  useEffect(() => {
    if (typeof EventSource === 'undefined') return
    const source = new EventSource('/api/tts/events')
    source.addEventListener('job', (e) => {
      const job = JSON.parse((e as MessageEvent).data) as Job
      upsert(job)
      if (job.state === 'done' || job.state === 'failed') {
        qc.invalidateQueries({ queryKey: keys.takes(job.line_id) })
        qc.invalidateQueries({ queryKey: ['scenes'] })
        qc.invalidateQueries({ queryKey: keys.ttsStatus })
      }
      if (job.state === 'done') {
        setTimeout(() => {
          setByLine((prev) => {
            if (prev.get(job.line_id)?.id !== job.id) return prev
            const next = new Map(prev)
            next.delete(job.line_id)
            return next
          })
        }, DONE_LINGER_MS)
      }
    })
    return () => source.close()
  }, [qc, upsert])

  const enqueue = useCallback<JobsContextValue['enqueue']>(
    async (lineIds, options) => {
      const jobs = await create.mutateAsync({ line_ids: lineIds, ...options })
      jobs.forEach(upsert)
    },
    [create, upsert],
  )

  const value = useMemo(() => ({ byLine, enqueue, enqueueError: create.error }), [byLine, enqueue, create.error])
  return <JobsContext.Provider value={value}>{children}</JobsContext.Provider>
}
