import { getRouteApi, Link, useNavigate } from '@tanstack/react-router'
import { Plus } from 'lucide-react'
import { useState } from 'react'
import { fileUrl } from '../../api/client'
import { useCharacters, useCreateCharacter, useProjectPortraits } from '../../api/hooks'
import { characterColors } from '../../lib/script'

const route = getRouteApi('/projects/$projectId/cast')

export function CastPage() {
  const { projectId } = route.useParams()
  const characters = useCharacters(projectId)
  const portraits = useProjectPortraits(characters.data)
  const colors = characterColors(characters.data ?? [])
  const create = useCreateCharacter(projectId)
  const navigate = useNavigate()
  const [name, setName] = useState('')

  return (
    <main className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">キャスト</h1>
          <p className="page__lead">話者の色は追加した順に決まります。8 人目からは灰色になります。</p>
        </div>
        <form
          className="row"
          onSubmit={async (e) => {
            e.preventDefault()
            if (!name.trim()) return
            const c = await create.mutateAsync({ name: name.trim() })
            setName('')
            await navigate({
              to: '/projects/$projectId/cast/$characterId',
              params: { projectId, characterId: c.id },
            })
          }}
        >
          <label htmlFor="new-character" className="visually-hidden">
            キャラクター名
          </label>
          <input
            id="new-character"
            className="ade-input"
            style={{ width: '14em' }}
            placeholder="キャラクター名"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <button className="ade-btn ade-btn--primary" type="submit" disabled={!name.trim() || create.isPending}>
            <Plus aria-hidden="true" />
            追加
          </button>
        </form>
      </div>

      {characters.data?.length === 0 && (
        <div className="empty">
          <strong>まだキャラクターがいません</strong>
          <span>名前を入れて追加すると、話し方・記憶・声を設定できます。</span>
        </div>
      )}

      {!!characters.data?.length && (
        <div className="cast-list">
          {characters.data.map((c) => {
            const first = portraits.get(c.id)?.[0]
            return (
              <Link
                key={c.id}
                to="/projects/$projectId/cast/$characterId"
                params={{ projectId, characterId: c.id }}
                className="cast-row"
                style={{ '--char': colors.get(c.id) } as React.CSSProperties}
              >
                {first ? (
                  <img className="cast-row__portrait" src={fileUrl(first.path)} alt="" />
                ) : (
                  <div className="cast-row__portrait" role="img" aria-label="立ち絵 未設定" />
                )}
                <span className="cast-row__name">
                  <strong>{c.name}</strong>
                  {c.reading && <span className="hint">{c.reading}</span>}
                </span>
                <p className="cast-row__profile">{c.profile || 'プロフィール 未設定'}</p>
                <span className="cast-row__status">
                  <span className={c.speech_style ? undefined : 'is-missing'}>
                    {c.speech_style ? `話し方 サンプル ${c.sample_lines.length} 本` : '話し方 未設定'}
                  </span>
                  <span className={c.voice_caption || c.voice_ref_path ? undefined : 'is-missing'}>
                    {c.voice_ref_path ? '声 参照音声あり' : c.voice_caption ? '声 説明のみ' : '声 未設定'}
                  </span>
                </span>
              </Link>
            )
          })}
        </div>
      )}
    </main>
  )
}
