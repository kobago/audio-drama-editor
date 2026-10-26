import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { Line } from '../../api/types'
import { ScriptLineRow } from './ScriptLineRow'

const line = (over: Partial<Line> = {}): Line => ({
  id: 1,
  scene_id: 1,
  kind: 'dialogue',
  character_id: 1,
  text: '遅いよ、もう。',
  tts_text: null,
  length_class: 'S',
  performance: { emotion: '不満', intensity: 0.2 },
  caption_override: null,
  pause_before_ms: 0,
  pause_after_ms: 300,
  sort_order: 0,
  portrait_id: null,
  adopted_take_id: null,
  adopted_take_path: null,
  adopted_duration_ms: null,
  take_count: 0,
  ...over,
})

function setup(over: Partial<Line> = {}, props: Partial<Parameters<typeof ScriptLineRow>[0]> = {}) {
  const handlers = { onFocus: vi.fn(), onToggleSelect: vi.fn(), onRetry: vi.fn() }
  render(
    <ScriptLineRow
      line={line(over)}
      index={0}
      speaker={{ name: 'アリス', color: 'var(--char-1)' }}
      focused={false}
      selected={false}
      {...handlers}
      {...props}
    />,
  )
  return handlers
}

describe('ScriptLineRow', () => {
  it('shows the speaker, text, performance tags and length', () => {
    setup()
    expect(screen.getByText('アリス')).toBeInTheDocument()
    expect(screen.getByText('遅いよ、もう。')).toBeInTheDocument()
    expect(screen.getByText('不満')).toBeInTheDocument()
    expect(screen.getByText(/7字/)).toBeInTheDocument()
    expect(screen.getByText('未生成')).toBeInTheDocument()
  })

  it('focuses on click but only toggles selection from the checkbox', async () => {
    const h = setup()
    await userEvent.click(screen.getByText('遅いよ、もう。'))
    expect(h.onFocus).toHaveBeenCalledOnce()
    await userEvent.click(screen.getByRole('checkbox'))
    expect(h.onToggleSelect).toHaveBeenCalledOnce()
    expect(h.onFocus).toHaveBeenCalledOnce()
  })

  it('marks focus and selection as separate states', () => {
    setup({}, { focused: true, selected: false })
    const row = screen.getByRole('listitem')
    expect(row).toHaveAttribute('data-focused', 'true')
    expect(row).toHaveAttribute('aria-selected', 'false')
  })

  it('shows the adopted take duration', () => {
    setup({ adopted_take_id: 3, adopted_duration_ms: 8400, take_count: 2 })
    expect(screen.getByText('00:08.40')).toBeInTheDocument()
    expect(screen.getByText('採用済み')).toBeInTheDocument()
  })

  it('prefers a running job over the adopted take', () => {
    setup(
      { adopted_take_id: 3, adopted_duration_ms: 8400 },
      { job: { id: 'j', line_id: 1, state: 'running', take_id: null, error: null } },
    )
    expect(screen.getByText('生成中')).toBeInTheDocument()
    expect(screen.queryByText('採用済み')).not.toBeInTheDocument()
  })

  it('retries a failed job without focusing the line', async () => {
    const h = setup({}, { job: { id: 'j', line_id: 1, state: 'failed', take_id: null, error: 'boom' } })
    await userEvent.click(screen.getByRole('button', { name: '失敗 · 再試行' }))
    expect(h.onRetry).toHaveBeenCalledOnce()
    expect(h.onFocus).not.toHaveBeenCalled()
  })

  it('renders directions without a checkbox', () => {
    setup({ kind: 'direction', text: '（風の音）', character_id: null })
    expect(screen.getByText('（風の音）')).toBeInTheDocument()
    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument()
  })
})
