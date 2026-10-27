import { KeyRound, PlugZap } from 'lucide-react'
import { useId, useState } from 'react'
import { useLlmModels, useSaveApiKey, useSaveSettings, useSettings, useTtsStatus } from '../../api/hooks'
import type { AppSettings, AppSettingsInput, LlmProvider, SettingSource } from '../../api/types'
import { AppBar } from '../../components/AppBar'
import { ModelPicker } from './ModelPicker'

const SOURCE_LABEL: Record<SettingSource, string> = {
  saved: 'この画面で保存した値',
  env: '環境変数の値',
  default: '既定値',
}

const PROVIDERS: { value: LlmProvider; label: string; hint: string }[] = [
  { value: 'opencode', label: 'OpenCode Go', hint: 'クラウドの大きなモデルで書きます。API キーが必要です。' },
  { value: 'llama_server', label: 'llama-server', hint: '手元のモデルで書きます。キャラクターの LoRA を使えます。' },
]

function SourceNote({ source, env }: { source: SettingSource; env: string }) {
  return (
    <small>
      {SOURCE_LABEL[source]}
      {source === 'env' && ` (${env})`}
    </small>
  )
}

export function AppSettingsPage() {
  const settings = useSettings()
  return (
    <>
      <AppBar />
      <main className="page page--narrow">
        <div>
          <h1 className="page__title">アプリ設定</h1>
          <p className="page__lead">
            ここで保存した値は、環境変数より優先します。待ち受けアドレスとデータの置き場所は、起動時の環境変数で決めます。
          </p>
        </div>
        {settings.isError && <p className="error-text">設定を読み込めません: {settings.error.message}</p>}
        {settings.data && <LlmSection settings={settings.data} />}
        <TtsSection />
      </main>
    </>
  )
}

function toInput(s: AppSettings): AppSettingsInput {
  return {
    llm_provider: s.llm_provider,
    llama_server_url: s.llama_server_url.value,
    opencode_base_url: s.opencode_base_url.value,
    opencode_model: s.opencode_model.value,
  }
}

/**
 * セリフを書く LLM。接続先を選ぶと、その接続先の設定だけを出す。
 * 接続先とモデルは選んだ時点で保存する。URL とキーは打ち間違えやすいので、ボタンで保存する。
 */
function LlmSection({ settings }: { settings: AppSettings }) {
  const save = useSaveSettings()
  // 押したその場で切り替える。保存に失敗したら、保存されている接続先に戻す
  const [provider, setProvider] = useState(settings.llm_provider)
  const id = useId()
  const change = (patch: Partial<AppSettingsInput>) =>
    save.mutate(
      { ...toInput(settings), llm_provider: provider, ...patch },
      { onError: () => setProvider(settings.llm_provider) },
    )

  return (
    <section className="ade-panel" aria-label="セリフを書く LLM">
      <header className="ade-panel__head">
        <h2>セリフを書く LLM</h2>
        <ConnectionCheck key={provider} provider={provider} />
      </header>
      <div className="ade-panel__body" style={{ gap: 'var(--space-5)' }}>
        <ConnectionResult provider={provider} />
        <fieldset className="ade-fieldset">
          <legend className="visually-hidden">接続先</legend>
          <div className="ade-cards">
            {PROVIDERS.map((p) => (
              <label key={p.value} className="ade-card-choice">
                <input
                  type="radio"
                  name={`${id}-provider`}
                  checked={provider === p.value}
                  onChange={() => {
                    setProvider(p.value)
                    change({ llm_provider: p.value })
                  }}
                />
                <span className="stack" style={{ gap: 2 }}>
                  <span className="ade-card-choice__title">{p.label}</span>
                  <span className="hint">{p.hint}</span>
                </span>
              </label>
            ))}
          </div>
        </fieldset>

        {provider === 'opencode' ? (
          <>
            <ApiKeyField source={settings.opencode_api_key.source} />
            <OpencodeModel
              model={settings.opencode_model.value}
              disabled={save.isPending}
              onChange={(m) => change({ opencode_model: m })}
            />
            <details className="ade-details">
              <summary>詳細設定</summary>
              <UrlField
                key={settings.opencode_base_url.value}
                label="接続先 URL"
                value={settings.opencode_base_url.value}
                source={settings.opencode_base_url.source}
                env="ADE_OPENCODE_BASE_URL"
                pending={save.isPending}
                onSave={(url) => change({ opencode_base_url: url })}
              />
            </details>
          </>
        ) : (
          <UrlField
            key={settings.llama_server_url.value}
            label="URL"
            value={settings.llama_server_url.value}
            source={settings.llama_server_url.source}
            env="ADE_LLAMA_SERVER_URL"
            pending={save.isPending}
            onSave={(url) => change({ llama_server_url: url })}
          />
        )}
        {save.isError && <p className="error-text">保存できません: {save.error.message}</p>}
      </div>
    </section>
  )
}

/** 保存した設定で接続先につないでみる。成功すれば、取れたモデルの数を出す */
function ConnectionCheck({ provider }: { provider: LlmProvider }) {
  const check = useLlmModels(provider)
  const result = check.data
  return (
    <div className="row">
      {check.isFetching ? (
        <span className="ade-job ade-job--running">確認中</span>
      ) : result?.ok ? (
        <span className="ade-job ade-job--done">接続済み</span>
      ) : result ? (
        <span className="ade-job ade-job--failed">接続できません</span>
      ) : null}
      <button
        className="ade-btn ade-btn--sm"
        type="button"
        disabled={check.isFetching}
        onClick={() => void check.refetch()}
      >
        <PlugZap aria-hidden="true" />
        接続を確認
      </button>
    </div>
  )
}

function ConnectionResult({ provider }: { provider: LlmProvider }) {
  const check = useLlmModels(provider)
  const result = check.data
  if (check.isFetching || !result) return null
  return (
    <p className={result.ok ? 'hint' : 'error-text'} role="status">
      {result.ok ? `モデルを ${result.models.length} 件取得しました。` : result.error}
    </p>
  )
}

function OpencodeModel({
  model,
  disabled,
  onChange,
}: {
  model: string
  disabled: boolean
  onChange: (model: string) => void
}) {
  const models = useLlmModels('opencode')
  const id = useId()
  const result = models.data
  return (
    <div className="ade-field">
      <label htmlFor={id}>モデル</label>
      <ModelPicker
        id={id}
        label="モデル"
        value={model}
        models={result?.ok ? result.models : undefined}
        loading={models.isFetching}
        error={result && !result.ok ? result.error : models.error ? models.error.message : null}
        disabled={disabled}
        onOpen={() => {
          if (!models.data && !models.isFetching) void models.refetch()
        }}
        onChange={onChange}
      />
      <small>開くと、使えるモデルの一覧を OpenCode Go から取得します。</small>
    </div>
  )
}

function UrlField({
  label,
  value,
  source,
  env,
  pending,
  onSave,
}: {
  label: string
  value: string
  source: SettingSource
  env: string
  pending: boolean
  onSave: (url: string) => void
}) {
  const [draft, setDraft] = useState(value)
  const id = useId()
  return (
    <form
      className="ade-field"
      onSubmit={(e) => {
        e.preventDefault()
        onSave(draft)
      }}
    >
      <label htmlFor={id}>{label}</label>
      <div className="row" style={{ flexWrap: 'wrap' }}>
        <input
          id={id}
          className="ade-input ade-input--mono"
          style={{ flex: '1 1 16em', width: 'auto' }}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <button className="ade-btn" type="submit" disabled={draft === value || pending}>
          URL を保存
        </button>
      </div>
      <SourceNote source={source} env={env} />
    </form>
  )
}

function ApiKeyField({ source }: { source: 'saved' | 'env' | null }) {
  const save = useSaveApiKey()
  const [key, setKey] = useState('')
  const id = useId()
  const status =
    source === 'saved'
      ? '保存済みです'
      : source === 'env'
        ? '環境変数 ADE_OPENCODE_API_KEY のキーを使っています'
        : 'まだ保存されていません'

  return (
    <form
      className="ade-field"
      onSubmit={async (e) => {
        e.preventDefault()
        await save.mutateAsync(key)
        setKey('')
      }}
    >
      <label htmlFor={id}>API キー</label>
      <div className="row" style={{ flexWrap: 'wrap' }}>
        <input
          id={id}
          className="ade-input ade-input--mono"
          style={{ flex: '1 1 16em', width: 'auto' }}
          type="password"
          autoComplete="off"
          placeholder={source ? '新しいキーで置き換える' : 'OpenCode のコンソールで作ったキー'}
          value={key}
          onChange={(e) => setKey(e.target.value)}
        />
        <button className="ade-btn" type="submit" disabled={!key.trim() || save.isPending}>
          <KeyRound aria-hidden="true" />
          キーを保存
        </button>
        {source === 'saved' && (
          <button className="ade-btn ade-btn--quiet" type="button" onClick={() => save.mutate('')}>
            キーを消す
          </button>
        )}
      </div>
      <small>{status}。保存したキーは画面に表示しません。</small>
      {save.isError && <p className="error-text">保存できません: {save.error.message}</p>}
    </form>
  )
}

function TtsSection() {
  const status = useTtsStatus()
  const engine = status.data?.engine
  return (
    <section className="ade-panel" aria-label="音声合成">
      <header className="ade-panel__head">
        <h2>音声合成 (Irodori-TTS)</h2>
        {engine?.state === 'ready' && <span className="ade-job ade-job--done">準備完了 {engine.device.split('(')[0]}</span>}
        {engine?.state === 'loading' && <span className="ade-job ade-job--running">読み込み中</span>}
        {engine?.state === 'failed' && <span className="ade-job ade-job--failed">読み込み失敗</span>}
        {engine?.state === 'disabled' && <span className="ade-job">オフ</span>}
      </header>
      <div className="ade-panel__body">
        {engine?.state === 'failed' && <p className="error-text">モデルを読み込めませんでした: {engine.error}</p>}
        {engine?.state === 'disabled' && <p className="hint">ADE_TTS=off で起動しているため、音声合成を使いません。</p>}
        <p className="hint">
          使うモデルは、起動時の環境変数 ADE_TTS_CHECKPOINT と ADE_TTS_CODEC で変えます。モデルは Hugging Face のキャッシュから読み込みます。
        </p>
      </div>
    </section>
  )
}
