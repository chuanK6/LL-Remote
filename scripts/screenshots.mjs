import { chromium } from '@playwright/test'
import { createServer } from 'vite'
import { mkdir } from 'node:fs/promises'
const server = await createServer()
await server.listen()
const browser = await chromium.launch({ headless: true })
try {
  const page = await browser.newPage({ viewport: { width: 1140, height: 844 }, deviceScaleFactor: 1 })
  await mkdir('docs/screenshots', { recursive: true })
  for (const [name, path] of [['device', '/#/device/preview-home'], ['moonlight', '/#/app/moonlight'], ['setup', '/?view=setup']]) {
    await page.goto('http://127.0.0.1:1420' + path)
    await page.waitForSelector('.workspace h1')
    await page.screenshot({ path: `docs/screenshots/${name}.png` })
  }
} finally { await browser.close(); await server.close() }
