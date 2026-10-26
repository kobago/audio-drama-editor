import { CircleCheck, Pause, Play, Trash2 } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { fileUrl } from '../api/client'
import type { Take } from '../api/types'
import { formatDuration } from '../lib/script'
import { Waveform } from './Waveform'

export function TakeItem({
  take,
  index,
  color,
  maxDurationMs,
  onAdopt,
  onDelete,
}: {
  take: Take
  /** 1 始まりのテイク番号 (古い順) */
  index: number
  color?: string
  maxDurationMs: number
  onAdopt: () => void
  onDelete: () => void
}) {
  const audio = useRef<HTMLAudioElement>(null)
  const [playing, setPlaying] = useState(false)
  const [progress, setProgress] = useState(0)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const src = fileUrl(take.path)

  useEffect(() => {
    const el = audio.current
    if (!el) return
    const onTime = () => setProgress(el.duration ? el.currentTime / el.duration : 0)
    const onEnd = () => {
      setPlaying(false)
      setProgress(0)
    }
    el.addEventListener('timeupdate', onTime)
    el.addEventListener('ended', onEnd)
    el.addEventListener('pause', () => setPlaying(false))
    el.addEventListener('play', () => setPlaying(true))
    return () => {
      el.removeEventListener('timeupdate', onTime)
      el.removeEventListener('ended', onEnd)
    }
  }, [])

  const toggle = () => {
    const el = audio.current
    if (!el) return
    if (el.paused) void el.play()
    else el.pause()
  }

  return (
    <div
      className={take.adopted ? 'ade-take ade-take--adopted' : 'ade-take'}
      style={{ '--char': color } as React.CSSProperties}
      data-testid="take"
    >
      <audio ref={audio} src={src} preload="none" />
      <button
        className="ade-take__play"
        type="button"
        aria-label={`テイク ${index} を${playing ? '一時停止' : '再生'}`}
        onClick={toggle}
      >
        {playing ? <Pause aria-hidden="true" /> : <Play aria-hidden="true" fill="currentColor" />}
      </button>
      <Waveform src={src} durationMs={take.duration_ms} maxDurationMs={maxDurationMs} progress={progress} />
      <div className="ade-take__info">
        {take.duration_ms > 0 && <span className="ade-timecode">{formatDuration(take.duration_ms)}</span>}
        {take.adopted ? (
          <span className="ade-adopted">
            <CircleCheck aria-hidden="true" />
            採用
          </span>
        ) : (
          <button className="ade-btn ade-btn--sm" type="button" onClick={onAdopt}>
            採用する
          </button>
        )}
        {confirmDelete ? (
          <>
            <button className="ade-btn ade-btn--sm ade-btn--danger" type="button" onClick={onDelete}>
              削除する
            </button>
            <button className="ade-btn ade-btn--sm ade-btn--quiet" type="button" onClick={() => setConfirmDelete(false)}>
              やめる
            </button>
          </>
        ) : (
          <button
            className="ade-icon-btn"
            type="button"
            aria-label={`テイク ${index} を削除`}
            onClick={() => setConfirmDelete(true)}
          >
            <Trash2 aria-hidden="true" />
          </button>
        )}
      </div>
    </div>
  )
}
