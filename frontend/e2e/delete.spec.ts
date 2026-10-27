import { expect, test } from '@playwright/test'

test('シーン・キャラクター・作品を、確認してから削除する', async ({ page, request }) => {
  const project = await (await request.post('/api/projects', { data: { title: '消す作品' } })).json()
  const alice = await (
    await request.post(`/api/projects/${project.id}/characters`, { data: { name: 'アリス' } })
  ).json()
  await request.post(`/api/projects/${project.id}/scenes`, { data: { title: '屋上', sort_order: 0 } })
  const night = await (
    await request.post(`/api/projects/${project.id}/scenes`, { data: { title: '夜の電話', sort_order: 1 } })
  ).json()
  await request.post(`/api/scenes/${night.id}/lines`, { data: { text: 'もしもし', character_id: alice.id } })

  // シーン: シーン設定の中で消す。消したら残ったシーンを開く
  await page.goto(`/projects/${project.id}?scene=${night.id}`)
  await page.getByRole('button', { name: 'シーン設定' }).click()
  await page.getByRole('button', { name: 'シーンを削除' }).click()
  await expect(page.getByRole('alert')).toContainText('「夜の電話」のセリフ・テイク')
  await page.getByRole('button', { name: 'やめる' }).click()
  await page.getByRole('button', { name: 'シーンを削除' }).click()
  await page.getByRole('button', { name: '削除する' }).click()
  await expect(page.getByRole('heading', { name: '屋上' })).toBeVisible()
  await expect(page.getByRole('navigation').getByText('夜の電話')).toHaveCount(0)

  // キャラクター: 基本タブの末尾で消す。消したらキャスト一覧に戻る
  await page.goto(`/projects/${project.id}/cast/${alice.id}`)
  await page.getByRole('button', { name: 'キャラクターを削除' }).click()
  await expect(page.getByRole('alert')).toContainText('セリフは話者なしで台本に残ります')
  await page.getByRole('button', { name: '削除する' }).click()
  await expect(page).toHaveURL(new RegExp(`/projects/${project.id}/cast$`))
  await expect(page.getByText('アリス')).toHaveCount(0)

  // 作品: 作品の設定で消す。消したら作品一覧に戻る
  await page.getByRole('link', { name: '設定', exact: true }).click()
  await page.getByRole('button', { name: '作品を削除' }).click()
  await expect(page.getByRole('alert')).toContainText('「消す作品」のキャスト・シーン')
  await page.getByRole('button', { name: '削除する' }).click()
  await expect(page).toHaveURL(/\/$/)
  await expect(page.getByText('消す作品')).toHaveCount(0)
  expect((await request.get(`/api/projects/${project.id}`)).status()).toBe(404)
})
