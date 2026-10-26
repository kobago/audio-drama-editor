import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { defineConfig, devices } from '@playwright/test'

// E2E は毎回空のデータディレクトリで、TTS を読み込まないサーバーに対して走らせる。
const dataDir = mkdtempSync(join(tmpdir(), 'ade-e2e-'))
const API_PORT = 3101
const WEB_PORT = 5181

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: `http://127.0.0.1:${WEB_PORT}`,
    trace: 'retain-on-failure',
    locale: 'ja-JP',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 1000 } } }],
  webServer: [
    {
      command: 'cargo run -q -p server --bin audio-drama-editor',
      cwd: '..',
      url: `http://127.0.0.1:${API_PORT}/api/health`,
      env: { ADE_TTS: 'off', ADE_DATA_DIR: dataDir, ADE_ADDR: `127.0.0.1:${API_PORT}` },
      timeout: 300_000,
      reuseExistingServer: false,
    },
    {
      command: `bunx vite --port ${WEB_PORT} --strictPort --host 127.0.0.1`,
      url: `http://127.0.0.1:${WEB_PORT}`,
      env: { ADE_API: `http://127.0.0.1:${API_PORT}` },
      reuseExistingServer: false,
    },
  ],
})
