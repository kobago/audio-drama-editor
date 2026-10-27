import { Trash2 } from 'lucide-react'
import { useState } from 'react'

/**
 * 作品・キャラクター・シーンを消す欄。ボタンを押すと、何が一緒に消えるかを見せてから、もう一度押させる。
 * 消したものは元に戻せないので、確認は押したその場に出す (ダイアログにしない)。
 */
export function DeleteSection({
  label,
  consequences,
  pending,
  error,
  onDelete,
}: {
  /** ボタンの文言。例: 作品を削除 */
  label: string
  /** 確認のときに見せる、一緒に消えるものの説明 */
  consequences: string
  pending: boolean
  error: Error | null
  onDelete: () => void
}) {
  const [confirming, setConfirming] = useState(false)

  return (
    <section className="ade-delete" aria-label={label}>
      {confirming ? (
        <>
          <p className="ade-delete__what" role="alert">
            {consequences}元に戻せません。
          </p>
          <div className="row">
            <button className="ade-btn ade-btn--sm ade-btn--danger-solid" type="button" disabled={pending} onClick={onDelete}>
              <Trash2 aria-hidden="true" />
              削除する
            </button>
            <button className="ade-btn ade-btn--sm ade-btn--quiet" type="button" onClick={() => setConfirming(false)}>
              やめる
            </button>
          </div>
        </>
      ) : (
        <button className="ade-btn ade-btn--sm ade-btn--danger" type="button" onClick={() => setConfirming(true)}>
          <Trash2 aria-hidden="true" />
          {label}
        </button>
      )}
      {error && <p className="error-text">削除できません: {error.message}</p>}
    </section>
  )
}
