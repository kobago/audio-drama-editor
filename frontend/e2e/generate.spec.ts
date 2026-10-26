import { expect, test } from '@playwright/test'

test('続きを書く: LLM の設定がなければ理由を出し、台本は変わらない', async ({ page }) => {
  await page.goto('/')
  await page.getByRole('button', { name: '新しい作品' }).click()
  await page.getByLabel('タイトル').fill('生成のテスト')
  await page.getByRole('button', { name: '作成' }).click()
  await expect(page).toHaveURL(/\/projects\/\d+$/)
  const projectId = page.url().match(/projects\/(\d+)/)![1]

  await page.goto(`/projects/${projectId}/cast`)
  await page.getByLabel('キャラクター名').fill('アリス')
  await page.getByRole('button', { name: '追加' }).click()
  await expect(page.getByRole('heading', { level: 1, name: 'アリス' })).toBeVisible()

  await page.goto(`/projects/${projectId}`)
  await page.getByRole('button', { name: 'シーンを追加' }).click()
  await page.getByLabel('シーン名').fill('屋上')
  await page.getByRole('button', { name: '追加', exact: true }).click()
  await page.getByLabel('追加する行の話者').selectOption({ label: 'アリス' })
  await page.getByLabel('追加するセリフ').fill('遅いよ、もう。')
  await page.getByLabel('追加するセリフ').press('Enter')
  await expect(page.getByRole('listitem')).toHaveCount(1)

  await page.getByRole('button', { name: '続きを書く' }).click()
  const panel = page.getByRole('form', { name: '続きを書く' })
  await expect(panel.getByText('「アリス「遅いよ、もう。」」の後ろに入れる。')).toBeVisible()
  await panel.getByLabel('行数').selectOption('2')
  await panel.getByRole('button', { name: '書く', exact: true }).click()
  await expect(panel.getByRole('alert')).toContainText('API キー')
  await expect(page.getByRole('listitem')).toHaveCount(1)

  // インスペクタ: 意図を書ける。書き直しも設定がなければ理由を出す
  await page.getByText('遅いよ、もう。').first().click()
  await page.getByLabel('意図').fill('遅れてきたことを軽く責める')
  await page.getByRole('button', { name: '書き直す' }).click()
  await expect(page.getByText(/API キー/).first()).toBeVisible()
  await expect(page.getByLabel('セリフ', { exact: true })).toHaveValue('遅いよ、もう。')
})
