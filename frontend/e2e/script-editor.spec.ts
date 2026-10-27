import { expect, type Page, test } from '@playwright/test'

/** 作品を作って台本画面を開き、作品 ID を返す。 */
async function createProject(page: Page, title: string): Promise<string> {
  await page.goto('/')
  await page.getByRole('button', { name: '新しい作品' }).click()
  await page.getByLabel('タイトル').fill(title)
  await page.getByLabel('あらすじ').fill('転校が決まった幼なじみと、最後の一か月。')
  await page.getByRole('button', { name: '作成' }).click()
  await expect(page).toHaveURL(/\/projects\/\d+$/)
  return page.url().match(/projects\/(\d+)/)![1]
}

async function addCharacter(page: Page, projectId: string, name: string) {
  await page.goto(`/projects/${projectId}/cast`)
  await page.getByLabel('キャラクター名').fill(name)
  await page.getByRole('button', { name: '追加' }).click()
  await expect(page.getByRole('heading', { level: 1, name })).toBeVisible()
}

test('作品・キャスト・シーンを作って台本を書く', async ({ page }) => {
  const projectId = await createProject(page, '放課後の約束')
  await expect(page.getByText('まだシーンがありません')).toBeVisible()

  await addCharacter(page, projectId, 'アリス')
  await page.getByRole('tab', { name: '話し方' }).click()
  await page.getByLabel('話し方 (システムプロンプトに入る)').fill('一人称は「私」。幼なじみにはため口。')
  await page.getByLabel('追加するサンプルセリフ').fill('ふーん。')
  await page.getByLabel('追加するサンプルの長さ').selectOption('S')
  await page.getByRole('button', { name: '追加' }).click()
  await page.getByRole('button', { name: '保存' }).click()
  await expect(page.getByText('未保存の変更あり')).toBeHidden()

  await addCharacter(page, projectId, 'ボブ')
  await page.getByRole('tab', { name: '声' }).click()
  await page.getByLabel(/声の基本/).fill('低く落ち着いた青年の声')
  await page.getByRole('button', { name: '保存' }).click()
  await expect(page.getByText('未保存の変更あり')).toBeHidden()

  // キャスト一覧に設定状況が出る
  await page.getByRole('link', { name: 'キャスト一覧' }).click()
  await expect(page.getByText('話し方 サンプル 1 本')).toBeVisible()
  await expect(page.getByText('声 説明のみ')).toBeVisible()

  // シーンを追加して台本を書く
  await page.getByRole('link', { name: '台本' }).click()
  await page.getByRole('button', { name: 'シーンを追加' }).click()
  await page.getByLabel('シーン名').fill('放課後の屋上')
  await page.getByRole('button', { name: '追加', exact: true }).click()
  await expect(page.getByRole('heading', { level: 1, name: '放課後の屋上' })).toBeVisible()

  const composer = page.getByLabel('追加するセリフ')
  await page.getByLabel('追加する行の話者').selectOption({ label: 'アリス' })
  await composer.fill('遅いよ、もう。ずっと待ってたんだから。')
  await composer.press('Enter')
  // 2 人の掛け合いでは話者が自動で入れ替わる
  await expect(page.getByLabel('追加する行の話者')).toHaveValue(/\d+/)
  await composer.fill('ごめん、待たせた。')
  await composer.press('Enter')
  await page.getByLabel('追加する行の話者').selectOption({ label: 'ト書き' })
  await composer.fill('（風が吹き抜ける）')
  await composer.press('Enter')

  const lines = page.getByRole('list', { name: '台本' }).getByRole('listitem')
  await expect(lines).toHaveCount(3)
  await expect(lines.nth(0)).toContainText('アリス')
  await expect(lines.nth(1)).toContainText('ボブ')
  await expect(lines.nth(2)).toContainText('風が吹き抜ける')

  // 再読み込みしても残っている
  await page.reload()
  await expect(lines).toHaveCount(3)
})

test('インスペクタで演技を付けると caption とタグに反映される', async ({ page }) => {
  const projectId = await createProject(page, '演技のテスト')
  await addCharacter(page, projectId, 'アリス')
  await page.getByRole('tab', { name: '声' }).click()
  await page.getByLabel(/声の基本/).fill('若い女性の声')
  await page.getByRole('button', { name: '保存' }).click()
  await expect(page.getByText('未保存の変更あり')).toBeHidden()

  await page.goto(`/projects/${projectId}`)
  await page.getByRole('button', { name: 'シーンを追加' }).click()
  await page.getByLabel('シーン名').fill('屋上')
  await page.getByRole('button', { name: '追加', exact: true }).click()
  await page.getByLabel('追加するセリフ').fill('遅いよ、もう。')
  await page.getByLabel('追加するセリフ').press('Enter')

  // 追加した行が編集対象になり、インスペクタが開く
  const line = page.getByRole('listitem').first()
  await expect(line).toHaveAttribute('data-focused', 'true')
  const performance = page.getByRole('region', { name: '演技' })
  await performance.getByLabel('感情').fill('不満')
  await performance.getByLabel('距離').selectOption('近い')

  await expect(performance.getByText('若い女性の声。不満を込めて、近い距離感で話す。')).toBeVisible()
  await expect(line.getByText('不満')).toBeVisible()
  await expect(line.getByText('距離 近い')).toBeVisible()

  // 手動で上書きすると行に「音声指示 手動」と出る
  await performance.getByRole('button', { name: '手動で上書き' }).click()
  await performance.getByLabel('音声生成に渡す指示 (手動)').fill('囁くように。')
  await expect(line.getByText('音声指示 手動')).toBeVisible()
})

test('キーボードで行を選び、TTS が使えないときは理由を表示する', async ({ page }) => {
  const projectId = await createProject(page, '操作のテスト')
  await addCharacter(page, projectId, 'アリス')
  await page.goto(`/projects/${projectId}`)
  await page.getByRole('button', { name: 'シーンを追加' }).click()
  await page.getByLabel('シーン名').fill('屋上')
  await page.getByRole('button', { name: '追加', exact: true }).click()
  for (const text of ['一行目', '二行目', '三行目']) {
    await page.getByLabel('追加するセリフ').fill(text)
    await page.getByLabel('追加するセリフ').press('Enter')
  }

  const lines = page.getByRole('list', { name: '台本' }).getByRole('listitem')
  await lines.nth(0).click()
  await page.keyboard.press('Space')
  await page.keyboard.press('ArrowDown')
  await page.keyboard.press('Space')
  await expect(lines.nth(1)).toHaveAttribute('data-focused', 'true')
  await expect(lines.nth(0)).toHaveAttribute('aria-selected', 'true')
  await expect(lines.nth(1)).toHaveAttribute('aria-selected', 'true')
  await expect(lines.nth(2)).toHaveAttribute('aria-selected', 'false')

  const generate = page.getByRole('button', { name: /音声生成/ }).first()
  await expect(generate.getByLabel('2 行')).toBeVisible()
  await expect(page.getByText('音声合成 オフ')).toBeVisible()
  await generate.click()
  await expect(page.getByRole('alert')).toContainText('TTS engine is not available')

  // 編集中の行を下へ移動する
  await page.getByRole('button', { name: '編集中の行を下へ' }).click()
  await expect(lines.nth(2)).toContainText('二行目')
})
