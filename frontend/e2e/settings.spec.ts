import { expect, test } from '@playwright/test'

test('LLM の接続先を保存し、API キーは表示しない', async ({ page }) => {
  await page.goto('/')
  await page.getByRole('link', { name: 'アプリ設定' }).click()
  await expect(page.getByRole('heading', { name: 'アプリ設定' })).toBeVisible()

  const llama = page.getByRole('region', { name: 'llama-server' })
  await expect(llama.getByText('既定値')).toBeVisible()
  await llama.getByLabel('URL').fill('http://192.168.1.20:8080/')
  await page.getByRole('button', { name: '接続先を保存' }).click()
  await expect(llama.getByLabel('URL')).toHaveValue('http://192.168.1.20:8080')
  await expect(llama.getByText('この画面で保存')).toBeVisible()

  // キーがないまま確認すると理由が出る
  const opencode = page.getByRole('region', { name: 'OpenCode Go' })
  await expect(opencode.getByText(/未設定。/)).toBeVisible()
  await opencode.getByRole('button', { name: '接続を確認' }).click()
  await expect(opencode.getByText('接続できない')).toBeVisible()
  await expect(opencode.getByRole('status')).toHaveText('API キーが未設定')

  // キーは保存しても画面に戻ってこない
  await opencode.getByLabel('API キー').fill('sk-test-not-a-real-key')
  await opencode.getByRole('button', { name: 'キーを保存' }).click()
  await expect(opencode.getByText(/保存済み。/)).toBeVisible()
  await expect(opencode.getByLabel('API キー')).toHaveValue('')
  await page.reload()
  await expect(page.getByText('sk-test-not-a-real-key')).toHaveCount(0)

  await opencode.getByRole('button', { name: 'キーを消す' }).click()
  await expect(opencode.getByText(/未設定。/)).toBeVisible()
})

test('不正な URL は保存できず理由を表示する', async ({ page }) => {
  await page.goto('/settings')
  const llama = page.getByRole('region', { name: 'llama-server' })
  await llama.getByLabel('URL').fill('localhost:8080')
  await page.getByRole('button', { name: '接続先を保存' }).click()
  await expect(page.getByText('保存できません: invalid URL: localhost:8080')).toBeVisible()
})
