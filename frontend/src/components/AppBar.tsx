import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { useTtsStatus } from '../api/hooks'
import type { EngineStatus } from '../api/types'

function engineLabel(engine: EngineStatus): { cls: string; text: string } {
  switch (engine.state) {
    case 'ready':
      // "Metal(MetalDevice(DeviceId(1)))" → "Metal"
      return { cls: 'done', text: `TTS 準備完了 · ${engine.device.split('(')[0]}` }
    case 'loading':
      return { cls: 'running', text: 'TTS 読み込み中' }
    case 'failed':
      return { cls: 'failed', text: 'TTS 読み込み失敗' }
    case 'disabled':
      return { cls: 'queued', text: 'TTS 無効' }
  }
}

export function TtsStatusPill() {
  const status = useTtsStatus()
  if (!status.data) return null
  const { cls, text } = engineLabel(status.data.engine)
  const title = status.data.engine.state === 'failed' ? status.data.engine.error : undefined
  return (
    <span className={`ade-job ade-job--${cls}`} title={title}>
      {text}
    </span>
  )
}

export function AppBar({ project, children }: { project?: string; children?: ReactNode }) {
  return (
    <header className="appbar">
      <div className="appbar__brand">
        <Link to="/" className="appbar__home">
          Audio Drama Editor
        </Link>
        {project && (
          <>
            <span aria-hidden="true" style={{ color: 'var(--ink-muted)' }}>
              /
            </span>
            <span className="appbar__project">{project}</span>
          </>
        )}
      </div>
      <div className="appbar__nav">{children}</div>
      <TtsStatusPill />
    </header>
  )
}
