import { KeyRound, PlugZap } from 'lucide-react'
import { useId, useState } from 'react'
import { useCheckConnection, useSaveApiKey, useSaveSettings, useSettings, useTtsStatus } from '../../api/hooks'
import type { AppSettings, LlmProvider, SettingSource } from '../../api/types'
import { AppBar } from '../../components/AppBar'

const SOURCE_LABEL: Record<SettingSource, string> = {
  saved: 'この画面で保存',
  env: '環境変数',
  default: '既定値',
}

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
      <main className="page" style={{ maxWidth: 860 }}>
        <div>
          <h1 className="page__title">アプリ設定</h1>
          <p className="page__lead">
            ここで保存した値は環境変数より優先する。待ち受けアドレスとデータの置き場所は起動時の環境変数で決める。
          </p>
        </div>
        {settings.isError && <p className="error-text">設定を読み込めません: {settings.error.message}</p>}
        {settings.data && <LlmSettingsForm key={JSON.stringify(settings.data)} settings={settings.data} />}
        <TtsSection />
      </main>
    </>
  )
}

function LlmSettingsForm({ settings }: { settings: AppSettings }) {
  const save = useSaveSettings()
  const [llama, setLlama] = useState(settings.llama_server_url.value)
  const [opencode, setOpencode] = useState(settings.opencode_base_url.value)
  const [model, setModel] = useState(settings.opencode_model.value)
  const [provider, setProvider] = useState<LlmProvider>(settings.llm_provider)
  // 「接続を確認」で取れたモデル。モデル欄の候補にする
  const [models, setModels] = useState<string[]>([])
  const id = useId()
  const dirty =
    provider !== settings.llm_provider ||
    llama !== settings.llama_server_url.value ||
    opencode !== settings.opencode_base_url.value ||
    model !== settings.opencode_model.value

  return (
    <form
      style={{ display: 'flex', flexDirection: 'column', gap: 16 }}
      onSubmit={(e) => {
        e.preventDefault()
        save.mutate({ llm_provider: provider, llama_server_url: llama, opencode_base_url: opencode, opencode_model: model })
      }}
    >
      <fieldset className="ade-panel" style={{ margin: 0 }}>
        <legend className="visually-hidden">セリフの生成に使う LLM</legend>
        <div className="ade-panel__head">
          <h3>セリフの生成に使う LLM</h3>
        </div>
        <div className="ade-panel__body" style={{ gap: 6 }}>
          {(
            [
              ['opencode', 'OpenCode Go', 'クラウドの大きなモデルで書く。API キーとモデルの設定が要る'],
              ['llama_server', 'llama-server', '手元のモデルで書く。キャラクターの LoRA を使える'],
            ] as const
          ).map(([value, label, hint]) => (
            <label key={value} style={{ display: 'flex', gap: 8, alignItems: 'baseline' }}>
              <input
                type="radio"
                name={`${id}-provider`}
                checked={provider === value}
                onChange={() => setProvider(value)}
                style={{ accentColor: 'var(--ai)' }}
              />
              <span>
                <b>{label}</b> <small style={{ color: 'var(--ink-muted)' }}>{hint}</small>
              </span>
            </label>
          ))}
        </div>
      </fieldset>

      <section className="ade-panel" aria-label="llama-server">
        <header className="ade-panel__head">
          <h3>llama-server</h3>
          <CheckButton provider="llama_server" disabled={dirty} />
        </header>
        <div className="ade-panel__body">
          <div className="ade-field">
            <label htmlFor={`${id}-llama`}>URL</label>
            <input
              id={`${id}-llama`}
              className="ade-input"
              style={{ fontFamily: 'var(--font-mono)', fontSize: 13 }}
              value={llama}
              onChange={(e) => setLlama(e.target.value)}
            />
            <SourceNote source={settings.llama_server_url.source} env="ADE_LLAMA_SERVER_URL" />
          </div>
          <small style={{ fontSize: 11, color: 'var(--ink-muted)' }}>
            キャラクターごとの LoRA が使える。
          </small>
        </div>
      </section>

      <section className="ade-panel" aria-label="OpenCode Go">
        <header className="ade-panel__head">
          <h3>OpenCode Go</h3>
          <CheckButton
            provider="opencode"
            disabled={dirty}
            onModels={(m) => {
              setModels(m)
              if (!model && m[0]) setModel(m[0])
            }}
          />
        </header>
        <div className="ade-panel__body">
          <div className="ade-field">
            <label htmlFor={`${id}-opencode`}>URL</label>
            <input
              id={`${id}-opencode`}
              className="ade-input"
              style={{ fontFamily: 'var(--font-mono)', fontSize: 13 }}
              value={opencode}
              onChange={(e) => setOpencode(e.target.value)}
            />
            <SourceNote source={settings.opencode_base_url.source} env="ADE_OPENCODE_BASE_URL" />
          </div>
          <ApiKeyField source={settings.opencode_api_key.source} />
          <div className="ade-field">
            <label htmlFor={`${id}-model`}>モデル</label>
            <input
              id={`${id}-model`}
              className="ade-input"
              list={`${id}-models`}
              placeholder="「接続を確認」で使えるモデルが候補に出る"
              value={model}
              onChange={(e) => setModel(e.target.value)}
            />
            <datalist id={`${id}-models`}>
              {models.map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
          </div>
        </div>
      </section>

      {save.isError && <p className="error-text">保存できません: {save.error.message}</p>}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <button className="ade-btn ade-btn--primary" type="submit" disabled={!dirty || save.isPending}>
          接続先を保存
        </button>
        {dirty && <span style={{ fontSize: 12, color: 'var(--ink-muted)' }}>保存してから接続を確認できる</span>}
      </div>
    </form>
  )
}

function CheckButton({
  provider,
  disabled,
  onModels,
}: {
  provider: LlmProvider
  disabled: boolean
  onModels?: (models: string[]) => void
}) {
  const check = useCheckConnection()
  const result = check.data
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      {check.isPending && <span className="ade-job ade-job--running">確認中</span>}
      {!check.isPending && result?.ok && (
        <span className="ade-job ade-job--done">
          接続 OK{result.models.length > 0 && ` · モデル ${result.models.length}`}
        </span>
      )}
      {!check.isPending && result && !result.ok && (
        <span className="ade-job ade-job--failed" title={result.error ?? undefined}>
          接続できない
        </span>
      )}
      <button
        className="ade-btn ade-btn--sm"
        type="button"
        disabled={disabled || check.isPending}
        onClick={async () => {
          const r = await check.mutateAsync(provider)
          if (r.ok) onModels?.(r.models)
        }}
      >
        <PlugZap aria-hidden="true" />
        接続を確認
      </button>
      {!check.isPending && result && !result.ok && (
        <span className="visually-hidden" role="status">
          {result.error}
        </span>
      )}
    </div>
  )
}

function ApiKeyField({ source }: { source: 'saved' | 'env' | null }) {
  const save = useSaveApiKey()
  const [key, setKey] = useState('')
  const id = useId()
  const status =
    source === 'saved'
      ? '保存済み'
      : source === 'env'
        ? '環境変数 ADE_OPENCODE_API_KEY を使用中'
        : '未設定'

  return (
    <div className="ade-field">
      <label htmlFor={`${id}-key`}>API キー</label>
      <div style={{ display: 'flex', gap: 8 }}>
        <input
          id={`${id}-key`}
          className="ade-input"
          type="password"
          autoComplete="off"
          placeholder={source ? '新しいキーで置き換える' : 'OpenCode のコンソールで作ったキー'}
          value={key}
          onChange={(e) => setKey(e.target.value)}
        />
        <button
          className="ade-btn"
          type="button"
          disabled={!key.trim() || save.isPending}
          onClick={async () => {
            await save.mutateAsync(key)
            setKey('')
          }}
        >
          <KeyRound aria-hidden="true" />
          キーを保存
        </button>
        {source === 'saved' && (
          <button className="ade-btn ade-btn--quiet" type="button" onClick={() => save.mutate('')}>
            キーを消す
          </button>
        )}
      </div>
      <small>{status}。保存したキーは画面に表示しない。</small>
      {save.isError && <p className="error-text">保存できません: {save.error.message}</p>}
    </div>
  )
}

function TtsSection() {
  const status = useTtsStatus()
  const engine = status.data?.engine
  return (
    <section className="ade-panel" aria-label="音声合成">
      <header className="ade-panel__head">
        <h3>音声合成 (Irodori-TTS)</h3>
      </header>
      <div className="ade-panel__body">
        {engine?.state === 'ready' && <p style={{ margin: 0 }}>準備完了 · {engine.device}</p>}
        {engine?.state === 'loading' && <p style={{ margin: 0 }}>モデルを読み込み中</p>}
        {engine?.state === 'failed' && <p className="error-text">読み込みに失敗: {engine.error}</p>}
        {engine?.state === 'disabled' && <p style={{ margin: 0 }}>無効 (ADE_TTS=off で起動している)</p>}
        <small style={{ fontSize: 11, color: 'var(--ink-muted)' }}>
          使うモデルは起動時の環境変数 ADE_TTS_CHECKPOINT / ADE_TTS_CODEC で変える (Hugging Face のキャッシュから読む)。
        </small>
      </div>
    </section>
  )
}
