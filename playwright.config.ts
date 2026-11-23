import { defineConfig, devices } from '@playwright/test'
export default defineConfig({
  testDir: './tests/e2e', fullyParallel: false, workers: 1,
  timeout: 30000,
  use: { baseURL: 'http://127.0.0.1:1420', viewport: { width: 1140, height: 844 }, headless: true, trace: 'retain-on-failure' },
  projects: [{ name: 'desktop', use: { ...devices['Desktop Chrome'], viewport: { width: 1140, height: 844 } } }, { name: 'scaled', use: { ...devices['Desktop Chrome'], viewport: { width: 960, height: 700 }, deviceScaleFactor: 1.5 } }],
  webServer: { command: 'npm run dev', url: 'http://127.0.0.1:1420', reuseExistingServer: true, timeout: 30000 },
})
