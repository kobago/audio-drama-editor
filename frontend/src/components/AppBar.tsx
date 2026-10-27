import { Link } from '@tanstack/react-router'
import { Settings } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTtsStatus } from '../api/hooks'
import type { EngineStatus } from '../api/types'
import { Logo, LogoMark } from './Logo'

function engineLabel(engine: EngineStatus): { cls: string; text: string } {
  switch (engine.state) {
    case 'ready':
      // "Metal(MetalDevice(DeviceId(1)))" → "Metal"
      return { cls: 'done', text: `音声合成 ${engine.device.split('(')[0]}` }
    case 'loading':
      return { cls: 'running', text: '音声合成を準備中' }
    case 'failed':
      return { cls: 'failed', text: '音声合成 読み込み失敗' }
    case 'disabled':
      return { cls: 'queued', text: '音声合成 オフ' }
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
        <Link to="/" className="appbar__home" aria-label="作品一覧へ">
          <Logo className="logo-full" />
          <LogoMark className="logo-mark" />
        </Link>
        {project && <span className="appbar__project">{project}</span>}
      </div>
      {children}
      <div className="appbar__tools">
        <TtsStatusPill />
        <Link to="/settings" className="ade-icon-btn" aria-label="アプリ設定" title="アプリ設定">
          <Settings aria-hidden="true" />
        </Link>
      </div>
    </header>
  )
}
