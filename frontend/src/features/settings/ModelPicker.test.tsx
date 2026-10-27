import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ModelPicker } from './ModelPicker'

const MODELS = ['deepseek-v4.1-flash', 'deepseek-v4.1', 'kimi-k3', 'glm-5']

function setup(props: Partial<Parameters<typeof ModelPicker>[0]> = {}) {
  const onChange = vi.fn()
  const onOpen = vi.fn()
  render(
    <ModelPicker
      id="m"
      label="モデル"
      value="kimi-k3"
      models={MODELS}
      loading={false}
      error={null}
      onOpen={onOpen}
      onChange={onChange}
      {...props}
    />,
  )
  return { onChange, onOpen }
}

describe('ModelPicker', () => {
  it('shows the current model and narrows the list as you type', async () => {
    const { onChange, onOpen } = setup()
    const trigger = screen.getByRole('button', { name: 'モデル kimi-k3' })
    await userEvent.click(trigger)
    expect(onOpen).toHaveBeenCalled()
    expect(screen.getAllByRole('option')).toHaveLength(4)
    expect(screen.getByRole('option', { name: 'kimi-k3' })).toHaveAttribute('aria-selected', 'true')

    await userEvent.type(screen.getByRole('combobox'), 'deep')
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([
      'deepseek-v4.1-flash',
      'deepseek-v4.1',
      // 一覧にない ID もいつでも使えるよう、最後に置く
      'deep を使う',
    ])
    await userEvent.keyboard('{ArrowDown}{Enter}')
    expect(onChange).toHaveBeenCalledWith('deepseek-v4.1')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
    expect(trigger).toHaveFocus()
  })

  it('uses an ID that is not in the list', async () => {
    const { onChange } = setup()
    await userEvent.click(screen.getByRole('button', { name: 'モデル kimi-k3' }))
    await userEvent.type(screen.getByRole('combobox'), 'my-model')
    expect(screen.getByRole('status')).toHaveTextContent('一覧に一致するモデルがありません')
    await userEvent.click(screen.getByRole('option', { name: 'my-model を使う' }))
    expect(onChange).toHaveBeenCalledWith('my-model')
  })

  it('explains why the list is missing and still accepts an ID', async () => {
    const { onChange } = setup({ value: '', models: undefined, error: 'API キーが保存されていません' })
    await userEvent.click(screen.getByRole('button', { name: 'モデル 未選択' }))
    expect(screen.getByRole('status')).toHaveTextContent('一覧を取得できません: API キーが保存されていません。')
    await userEvent.type(screen.getByRole('combobox'), 'glm-5{Enter}')
    expect(onChange).toHaveBeenCalledWith('glm-5')
  })

  it('closes on Escape without changing the model', async () => {
    const { onChange } = setup()
    await userEvent.click(screen.getByRole('button', { name: 'モデル kimi-k3' }))
    await userEvent.keyboard('{Escape}')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
    expect(onChange).not.toHaveBeenCalled()
  })
})
