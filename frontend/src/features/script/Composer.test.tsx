import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { Character } from '../../api/types'
import { Composer } from './Composer'

const ch = (id: number, name: string) => ({ id, name, sort_order: 0 }) as Character
const cast = [ch(1, 'アリス'), ch(2, 'ボブ')]

describe('Composer', () => {
  it('adds a line on Enter and switches to the other speaker of a two-person scene', async () => {
    const onAdd = vi.fn().mockResolvedValue(undefined)
    render(<Composer characters={cast} lastSpeakerId={2} onAdd={onAdd} pending={false} />)
    const speaker = screen.getByLabelText('追加する行の話者')
    expect(speaker).toHaveValue('1')

    await userEvent.type(screen.getByLabelText('追加するセリフ'), '遅いよ{Enter}')
    expect(onAdd).toHaveBeenCalledWith({ kind: 'dialogue', character_id: 1, text: '遅いよ' })
    await waitFor(() => expect(speaker).toHaveValue('2'))
    expect(screen.getByLabelText('追加するセリフ')).toHaveValue('')
  })

  it('keeps text typed while the previous line is still being saved', async () => {
    let resolve!: () => void
    const onAdd = vi.fn().mockReturnValueOnce(new Promise<void>((r) => (resolve = r)))
    render(<Composer characters={cast} lastSpeakerId={null} onAdd={onAdd} pending={false} />)
    const input = screen.getByLabelText('追加するセリフ')
    await userEvent.type(input, '一行目{Enter}')
    await userEvent.type(input, '二行目')
    resolve()
    await waitFor(() => expect(onAdd).toHaveBeenCalledOnce())
    expect(input).toHaveValue('二行目')
  })

  it('restores the text and shows why when adding fails', async () => {
    const onAdd = vi.fn().mockRejectedValue(new Error('scene not found'))
    render(<Composer characters={cast} lastSpeakerId={null} onAdd={onAdd} pending={false} />)
    const input = screen.getByLabelText('追加するセリフ')
    await userEvent.type(input, '一行目{Enter}')
    expect(await screen.findByRole('alert')).toHaveTextContent('追加できません: scene not found')
    expect(input).toHaveValue('一行目')
  })

  it('does not submit while an IME composition is in progress', () => {
    const onAdd = vi.fn()
    render(<Composer characters={cast} lastSpeakerId={null} onAdd={onAdd} pending={false} />)
    const input = screen.getByLabelText('追加するセリフ')
    fireEvent.change(input, { target: { value: 'おそい' } })
    fireEvent.keyDown(input, { key: 'Enter', isComposing: true })
    expect(onAdd).not.toHaveBeenCalled()
  })

  it('adds directions without a speaker', async () => {
    const onAdd = vi.fn().mockResolvedValue(undefined)
    render(<Composer characters={cast} lastSpeakerId={null} onAdd={onAdd} pending={false} />)
    await userEvent.selectOptions(screen.getByLabelText('追加する行の話者'), 'ト書き')
    await userEvent.type(screen.getByLabelText('追加するセリフ'), '（風の音）{Enter}')
    expect(onAdd).toHaveBeenCalledWith({ kind: 'direction', text: '（風の音）' })
  })
})
