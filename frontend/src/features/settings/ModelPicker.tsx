import { Check, ChevronDown } from 'lucide-react'
import { useEffect, useId, useRef, useState } from 'react'

/**
 * モデルを選ぶ欄。押すと、接続先から取ったモデルの一覧を検索付きで開く。
 * 一覧にないモデルは、検索欄に ID を打てば「〜を使う」として選べる (選択と自由入力を 1 つの欄にまとめる)。
 */
export function ModelPicker({
  id,
  label,
  value,
  models,
  loading,
  error,
  disabled,
  onOpen,
  onChange,
}: {
  id: string
  label: string
  value: string
  /** 接続先から取ったモデル。まだ取っていなければ undefined */
  models: string[] | undefined
  loading: boolean
  error: string | null
  disabled?: boolean
  /** 一覧を開いたとき。モデルをまだ取っていなければ取りに行く */
  onOpen: () => void
  onChange: (model: string) => void
}) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const root = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLButtonElement>(null)
  const list = useRef<HTMLUListElement>(null)
  const uid = useId()

  const q = query.trim()
  const matches = (models ?? []).filter((m) => m.toLowerCase().includes(q.toLowerCase()))
  const options = q && !(models ?? []).includes(q) ? [...matches, q] : matches
  const custom = (m: string) => m === q && !(models ?? []).includes(q)

  useEffect(() => {
    if (!open) return
    const close = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', close)
    return () => document.removeEventListener('pointerdown', close)
  }, [open])

  useEffect(() => {
    list.current?.querySelector('[data-active="true"]')?.scrollIntoView({ block: 'nearest' })
  }, [active])

  const show = () => {
    setQuery('')
    setActive(Math.max(0, (models ?? []).indexOf(value)))
    setOpen(true)
    onOpen()
  }
  const pick = (m: string) => {
    setOpen(false)
    trigger.current?.focus()
    if (m !== value) onChange(m)
  }

  return (
    <div className="ade-picker" ref={root}>
      <button
        id={id}
        ref={trigger}
        type="button"
        className="ade-input ade-picker__trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-labelledby={`${uid}-label ${uid}-value`}
        disabled={disabled}
        onClick={() => (open ? setOpen(false) : show())}
      >
        <span id={`${uid}-label`} className="visually-hidden">
          {label}
        </span>
        <span id={`${uid}-value`} className={value ? 'ade-picker__value' : 'ade-picker__placeholder'}>
          {value || '未選択'}
        </span>
        <ChevronDown aria-hidden="true" />
      </button>
      {open && (
        <div className="ade-picker__popover">
          <input
            className="ade-input ade-input--mono"
            role="combobox"
            aria-label={`${label}を検索、または ID を入力`}
            aria-expanded="true"
            aria-controls={`${uid}-list`}
            aria-autocomplete="list"
            aria-activedescendant={options[active] !== undefined ? `${uid}-o${active}` : undefined}
            placeholder="検索、または ID を入力"
            autoFocus
            value={query}
            onChange={(e) => {
              setQuery(e.target.value)
              setActive(0)
            }}
            onKeyDown={(e) => {
              if (e.key === 'ArrowDown') {
                e.preventDefault()
                setActive((i) => Math.min(i + 1, options.length - 1))
              } else if (e.key === 'ArrowUp') {
                e.preventDefault()
                setActive((i) => Math.max(i - 1, 0))
              } else if (e.key === 'Enter') {
                e.preventDefault()
                if (options[active] !== undefined) pick(options[active])
              } else if (e.key === 'Escape') {
                e.preventDefault()
                setOpen(false)
                trigger.current?.focus()
              }
            }}
          />
          <ul ref={list} id={`${uid}-list`} role="listbox" aria-label={label} className="ade-picker__list">
            {options.map((m, i) => (
              <li
                key={m}
                id={`${uid}-o${i}`}
                role="option"
                aria-selected={m === value}
                data-active={i === active}
                className="ade-picker__option"
                onPointerDown={(e) => e.preventDefault()}
                onPointerEnter={() => setActive(i)}
                onClick={() => pick(m)}
              >
                {custom(m) ? (
                  <span>
                    <code>{m}</code> を使う
                  </span>
                ) : (
                  <code>{m}</code>
                )}
                {m === value && <Check aria-hidden="true" />}
              </li>
            ))}
          </ul>
          <p className="ade-picker__note" role="status">
            {loading
              ? 'モデルの一覧を取得しています…'
              : error
                ? `一覧を取得できません: ${error}。ID を入力すれば使えます。`
                : models
                  ? q
                    ? matches.length === 0
                      ? '一覧に一致するモデルがありません。この ID をそのまま使えます。'
                      : `${models.length} 件中 ${matches.length} 件が一致します。`
                    : `${models.length} 件。一覧にないモデルも、ID を入力すれば使えます。`
                  : 'ID を入力してください。'}
          </p>
        </div>
      )}
    </div>
  )
}
