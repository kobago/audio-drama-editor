import { useQuery } from '@tanstack/react-query'

/** 1 秒あたりの波形の本数。尺に比例した長さで描く。 */
const BARS_PER_SECOND = 10
const MAX_HEIGHT = 26

async function loadPeaks(src: string, bars: number): Promise<number[]> {
  const Ctx = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext
  if (!Ctx) return []
  const buf = await (await fetch(src)).arrayBuffer()
  const ctx = new Ctx()
  try {
    const audio = await ctx.decodeAudioData(buf)
    const data = audio.getChannelData(0)
    const step = Math.max(1, Math.floor(data.length / bars))
    const peaks: number[] = []
    for (let i = 0; i < bars; i++) {
      let max = 0
      for (let j = i * step; j < Math.min(data.length, (i + 1) * step); j++) max = Math.max(max, Math.abs(data[j]))
      peaks.push(max)
    }
    const top = Math.max(...peaks, 1e-6)
    return peaks.map((p) => p / top)
  } finally {
    void ctx.close()
  }
}

/**
 * テイクの波形。`maxDurationMs` に対する尺の割合で横幅が決まるので、
 * 同じセリフのテイク同士で長さを見比べられる。
 */
export function Waveform({
  src,
  durationMs,
  maxDurationMs,
  progress = 0,
}: {
  src: string
  durationMs: number
  maxDurationMs: number
  /** 再生済みの割合 (0〜1) */
  progress?: number
}) {
  const bars = Math.max(8, Math.round((durationMs / 1000) * BARS_PER_SECOND))
  const peaks = useQuery({ queryKey: ['peaks', src, bars], queryFn: () => loadPeaks(src, bars), staleTime: Infinity })
  const values = peaks.data?.length ? peaks.data : Array.from({ length: bars }, () => 0.15)
  const widthPct = maxDurationMs > 0 ? Math.max(12, (durationMs / maxDurationMs) * 100) : 100
  return (
    <div className="ade-take__wave" aria-hidden="true">
      <div style={{ display: 'flex', alignItems: 'center', gap: 2, height: '100%', width: `${widthPct}%` }}>
        {values.map((v, i) => (
          <i
            key={i}
            style={{
              flex: '1 1 0',
              width: 'auto',
              minWidth: 1,
              height: Math.max(3, Math.round(v * MAX_HEIGHT)),
              background: i / values.length < progress ? 'var(--ink)' : 'currentColor',
            }}
          />
        ))}
      </div>
    </div>
  )
}
