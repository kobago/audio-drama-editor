import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { Job } from '../api/types'
import { CountBadge, JobPill, LengthBadge } from './badges'

const job = (state: Job['state'], error: string | null = null): Job => ({
  id: 'j1',
  line_id: 1,
  state,
  take_id: null,
  error,
})

describe('CountBadge', () => {
  it('renders nothing for zero', () => {
    const { container } = render(<CountBadge count={0} unit="行" />)
    expect(container).toBeEmptyDOMElement()
  })

  it('announces the count with its unit and caps the display at 99+', () => {
    render(<CountBadge count={120} unit="行" />)
    const badge = screen.getByLabelText('120 行')
    expect(badge).toHaveTextContent('99+')
  })
})

describe('LengthBadge', () => {
  it('flags text that is longer than its class', () => {
    render(<LengthBadge text={'あ'.repeat(20)} lengthClass="S" />)
    expect(screen.getByText(/20字/)).toHaveTextContent('S20字 長い')
  })

  it('shows only the count without a class', () => {
    render(<LengthBadge text="遅いよ" lengthClass={null} />)
    expect(screen.getByText('3字')).not.toHaveClass('ade-len--off')
  })
})

describe('JobPill', () => {
  it('labels each state in words', () => {
    const { rerender } = render(<JobPill job={job('queued')} />)
    expect(screen.getByText('待機中')).toBeInTheDocument()
    rerender(<JobPill job={job('running')} />)
    expect(screen.getByText('生成中')).toBeInTheDocument()
  })

  it('offers a retry for failed jobs and shows the error on hover', async () => {
    const onRetry = vi.fn()
    render(<JobPill job={job('failed', 'TTS engine failed to load')} onRetry={onRetry} />)
    const button = screen.getByRole('button', { name: '失敗 · 再試行' })
    expect(button).toHaveAttribute('title', 'TTS engine failed to load')
    await userEvent.click(button)
    expect(onRetry).toHaveBeenCalledOnce()
  })
})
