import { expect, test } from '@playwright/test'

test('キャラクターの記憶を書き、セリフの記憶パネルから探す', async ({ page }) => {
  await page.goto('/')
  await page.getByRole('button', { name: '新しい作品' }).click()
  await page.getByLabel('タイトル').fill('記憶のテスト')
  await page.getByRole('button', { name: '作成' }).click()
  await expect(page).toHaveURL(/\/projects\/\d+$/)
  const projectId = page.url().match(/projects\/(\d+)/)![1]

  await page.goto(`/projects/${projectId}/cast`)
  await page.getByLabel('キャラクター名').fill('アリス')
  await page.getByRole('button', { name: '追加' }).click()
  await page.getByRole('tab', { name: '記憶' }).click()

  // 追加
  const add = page.getByRole('region', { name: '記憶を追加' })
  await add.getByLabel('種類').selectOption('recollection')
  await add.getByLabel('いつ').fill('小学4年の夏')
  await add.getByLabel('関わる人物').fill('ボブ、母')
  await add.getByLabel('内容').fill('夏祭りで、ボブと花火を見る約束をした。')
  await add.getByRole('button', { name: '追加' }).click()
  const list = page.getByLabel('記憶の一覧')
  await expect(list.getByText('小学4年の夏・ボブ、母')).toBeVisible()
  await expect(add.getByLabel('内容')).toHaveValue('')
  await add.getByLabel('種類').selectOption('habit')
  await add.getByLabel('内容').fill('ボブは待ち合わせにいつも遅れてくる。')
  await add.getByRole('button', { name: '追加' }).click()

  await expect(list.getByText('ボブは待ち合わせに')).toBeVisible()
  await expect(page.getByRole('tab', { name: /記憶/ }).getByLabel('2 件')).toBeVisible()

  // 種類で絞り込む
  await page.getByRole('group', { name: '種類で絞り込む' }).getByRole('button', { name: /好み・習慣/ }).click()
  await expect(list.getByText('夏祭りで')).toBeHidden()
  await page.getByRole('group', { name: '種類で絞り込む' }).getByRole('button', { name: /すべて/ }).click()

  // 編集
  await list.getByRole('button', { name: /「夏祭りで、.*」を編集/ }).click()
  await list.getByLabel('内容').fill('夏祭りで、ボブと「来年も花火を見よう」と約束した。')
  await list.getByRole('button', { name: '保存' }).click()
  await expect(list.getByText('「来年も花火を見よう」')).toBeVisible()

  // 台本のセリフから記憶を探す (テストではモデルを読み込まないので、理由が出る)
  await page.goto(`/projects/${projectId}`)
  await page.getByRole('button', { name: 'シーンを追加' }).click()
  await page.getByLabel('シーン名').fill('屋上')
  await page.getByRole('button', { name: '追加', exact: true }).click()
  await page.getByLabel('追加する行の話者').selectOption({ label: 'アリス' })
  await page.getByLabel('追加するセリフ').fill('遅いよ、もう。')
  await page.getByLabel('追加するセリフ').press('Enter')
  await page.getByText('遅いよ、もう。').first().click()

  const panel = page.getByRole('region', { name: 'このセリフで使う記憶' })
  await expect(panel).toBeVisible()
  await panel.getByLabel('これから話すこと').fill('遅れてきたことを軽く責める')
  await panel.getByRole('button', { name: '記憶を探す' }).click()
  await expect(panel.getByText(/記憶の検索ができません/)).toBeVisible()
})
