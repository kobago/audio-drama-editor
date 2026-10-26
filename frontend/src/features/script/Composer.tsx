import { CornerDownLeft } from 'lucide-react'
import { useState } from 'react'
import type { Character, LineInput } from '../../api/types'

const DIRECTION = 'direction'

/**
 * 台本の末尾に行を足す入力欄。Enter で追加、Shift+Enter で改行。
 * 追加後は同じ話者を残さず、会話の相手に切り替える (2 人の掛け合いを速く書くため)。
 */
export function Composer({
  characters,
  lastSpeakerId,
  onAdd,
  pending,
}: {
  characters: Character[]
  lastSpeakerId: number | null
  onAdd: (input: Partial<LineInput>) => Promise<unknown>
  pending: boolean
}) {
  const nextSpeaker = (after: number | null) => {
    const others = characters.filter((c) => c.id !== after)
    return String(others[0]?.id ?? characters[0]?.id ?? DIRECTION)
  }
  const [speaker, setSpeaker] = useState<string>(() => nextSpeaker(lastSpeakerId))
  const [text, setText] = useState('')
  const [error, setError] = useState<string | null>(null)

  const submit = async () => {
    const value = text.trim()
    if (!value) return
    const isDirection = speaker === DIRECTION
    // 送信を待たずに空にする (待っている間に打った次のセリフを消さないため)。失敗したら戻す。
    setText('')
    setError(null)
    if (!isDirection && characters.length === 2) setSpeaker(nextSpeaker(Number(speaker)))
    try {
      await onAdd(
        isDirection
          ? { kind: 'direction', text: value }
          : { kind: 'dialogue', character_id: Number(speaker), text: value },
      )
    } catch (e) {
      setText((current) => current || value)
      setSpeaker(speaker)
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  return (
    <form
      className="composer"
      onSubmit={(e) => {
        e.preventDefault()
        void submit()
      }}
    >
      <select
        className="ade-input"
        aria-label="追加する行の話者"
        value={speaker}
        onChange={(e) => setSpeaker(e.target.value)}
      >
        {characters.map((c) => (
          <option key={c.id} value={c.id}>
            {c.name}
          </option>
        ))}
        <option value={DIRECTION}>ト書き</option>
      </select>
      <textarea
        className="ade-input ade-input--script"
        aria-label="追加するセリフ"
        rows={1}
        placeholder={speaker === DIRECTION ? '（ト書き・SE の指示）' : 'セリフを入力して Enter'}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault()
            void submit()
          }
        }}
      />
      <button className="ade-btn" type="submit" disabled={!text.trim() || pending}>
        <CornerDownLeft aria-hidden="true" />
        追加
      </button>
      {error && (
        <p className="error-text" role="alert" style={{ gridColumn: '1 / -1' }}>
          追加できません: {error}
        </p>
      )}
    </form>
  )
}
