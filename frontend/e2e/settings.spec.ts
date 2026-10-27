import { expect, test } from '@playwright/test'

test('LLM の接続先を選び、API キーは表示しない', async ({ page }) => {
  await page.goto('/')
  await page.getByRole('link', { name: 'アプリ設定' }).click()
  await expect(page.getByRole('heading', { name: 'アプリ設定' })).toBeVisible()
  const llm = page.getByRole('region', { name: 'セリフを書く LLM' })

  // 接続先を選ぶと、その接続先の設定だけが出る
  await llm.getByRole('radio', { name: /llama-server/ }).check()
  await expect(llm.getByText('既定値')).toBeVisible()
  await llm.getByLabel('URL', { exact: true }).fill('http://192.168.1.20:8080/')
  await llm.getByRole('button', { name: 'URL を保存' }).click()
  await expect(llm.getByLabel('URL', { exact: true })).toHaveValue('http://192.168.1.20:8080')
  await expect(llm.getByText('この画面で保存した値')).toBeVisible()
  await expect(llm.getByLabel('API キー', { exact: true })).toHaveCount(0)

  // キーがないまま確認すると理由が出る
  await llm.getByRole('radio', { name: /OpenCode Go/ }).check()
  await expect(llm.getByText(/まだ保存されていません。/)).toBeVisible()
  await llm.getByRole('button', { name: '接続を確認' }).click()
  await expect(llm.getByText('接続できません', { exact: true })).toBeVisible()
  await expect(llm.getByRole('status')).toHaveText('API キーが保存されていません')

  // キーは保存しても画面に戻ってこない
  await llm.getByLabel('API キー', { exact: true }).fill('sk-test-not-a-real-key')
  await llm.getByRole('button', { name: 'キーを保存' }).click()
  await expect(llm.getByText(/保存済みです。/)).toBeVisible()
  await expect(llm.getByLabel('API キー', { exact: true })).toHaveValue('')
  await page.reload()
  await expect(page.getByText('sk-test-not-a-real-key')).toHaveCount(0)

  await llm.getByRole('button', { name: 'キーを消す' }).click()
  await expect(llm.getByText(/まだ保存されていません。/)).toBeVisible()
})

test('一覧を取れなくても、モデルの ID を入力して選べる', async ({ page }) => {
  await page.goto('/settings')
  const llm = page.getByRole('region', { name: 'セリフを書く LLM' })
  await llm.getByRole('radio', { name: /OpenCode Go/ }).check()

  await llm.getByRole('button', { name: 'モデル 未選択' }).click()
  await expect(llm.getByText(/一覧を取得できません: API キーが保存されていません。/)).toBeVisible()
  await llm.getByRole('combobox').fill('deepseek-v4.1-flash')
  await llm.getByRole('option', { name: 'deepseek-v4.1-flash を使う' }).click()
  await expect(llm.getByRole('button', { name: 'モデル deepseek-v4.1-flash' })).toBeVisible()

  await page.reload()
  await expect(llm.getByRole('button', { name: 'モデル deepseek-v4.1-flash' })).toBeVisible()
})

test('不正な URL は保存できず理由を表示する', async ({ page }) => {
  await page.goto('/settings')
  const llm = page.getByRole('region', { name: 'セリフを書く LLM' })
  await llm.getByRole('radio', { name: /llama-server/ }).check()
  await llm.getByLabel('URL', { exact: true }).fill('localhost:8080')
  await llm.getByRole('button', { name: 'URL を保存' }).click()
  await expect(page.getByText('保存できません: invalid URL: localhost:8080')).toBeVisible()
})
