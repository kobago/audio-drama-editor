import { createContext, useContext } from 'react'
import type { Job, SynthesisParams } from './types'

export interface JobsContextValue {
  /** line_id ごとの最新ジョブ */
  byLine: Map<number, Job>
  enqueue: (lineIds: number[], options?: { seed?: number; params?: Partial<SynthesisParams> }) => Promise<void>
  enqueueError: Error | null
}

export const JobsContext = createContext<JobsContextValue | null>(null)

export function useJobs(): JobsContextValue {
  const ctx = useContext(JobsContext)
  if (!ctx) throw new Error('useJobs must be used inside JobsProvider')
  return ctx
}
