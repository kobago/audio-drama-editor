import { getRouteApi, Link, useNavigate } from '@tanstack/react-router'
import { Plus } from 'lucide-react'
import { useState } from 'react'
import { fileUrl } from '../../api/client'
import { useCharacters, useCreateCharacter, useProjectPortraits } from '../../api/hooks'
import { SpeakerChip } from '../../components/badges'
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
          <p className="page__lead">識別色は追加した順に決まる。8 人目以降は灰色。</p>
        </div>
        <form
          style={{ display: 'flex', gap: 8 }}
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
          <span>名前を入れて追加すると、話し方・知っていること・声を設定できる。</span>
        </div>
      )}

      <div className="card-grid">
        {characters.data?.map((c) => {
          const first = portraits.get(c.id)?.[0]
          return (
            <Link
              key={c.id}
              to="/projects/$projectId/cast/$characterId"
              params={{ projectId, characterId: c.id }}
              className="ade-character"
              style={{ '--char': colors.get(c.id), color: 'var(--ink)', textDecoration: 'none' } as React.CSSProperties}
            >
              {first ? (
                <img className="ade-character__portrait" src={fileUrl(first.path)} alt="" />
              ) : (
                <div className="ade-character__portrait" role="img" aria-label="立ち絵 未設定" />
              )}
              <div className="ade-character__body">
                <p className="ade-character__name">
                  <SpeakerChip name={c.name} color={colors.get(c.id)} />
                </p>
                <p className="ade-character__voice">
                  {c.voice_caption || '声の基本 未設定'} · 参照音声{c.voice_ref_path ? 'あり' : 'なし'}
                </p>
                <p
                  className="ade-character__voice"
                  style={c.speech_style ? undefined : { color: 'var(--warning)' }}
                >
                  {c.speech_style ? `話し方 設定済み · サンプル ${c.sample_lines.length}` : '話し方 未設定'}
                </p>
              </div>
            </Link>
          )
        })}
      </div>
    </main>
  )
}
