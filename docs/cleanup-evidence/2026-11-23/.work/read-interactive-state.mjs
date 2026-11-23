import { readFile, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { chromium } from '@playwright/test'

const workspace = resolve(import.meta.dirname, '..')
const metadata = JSON.parse(await readFile(join(workspace, '.work', 'interactive-desktop.json'), 'utf8'))
let browser
for (let attempt = 0; attempt < 35; attempt++) {
  try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${metadata.DebugPort}`, { timeout: 1000 }); break } catch {}
  await new Promise(resolve => setTimeout(resolve, 400))
}
if (!browser) throw new Error('The desktop debug endpoint did not become available')
try {
  const page = browser.contexts()[0]?.pages()[0]
  if (!page) throw new Error('The main desktop page has not loaded')
  await page.waitForFunction(() => !!window.__TAURI_INTERNALS__, null, { timeout: 8000 })
  const snapshot = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('bootstrap', {}))
  const result = { timestamp: new Date().toISOString(), version: snapshot.version, elevated: metadata.Elevated, portable: snapshot.portable, dataDirectory: snapshot.dataDirectory, componentCount: snapshot.components.length, allComponentsAvailable: snapshot.components.every(c=>c.available), sunshineAccountInitialized: snapshot.sunshine.accountInitialized, setupComplete: snapshot.preferences.setupComplete, authenticated: snapshot.account.authenticated, serverRunning: snapshot.account.serverRunning, sunshineRunning: snapshot.sunshine.running, sunshineReady: snapshot.sunshine.ready, acceptControl: snapshot.preferences.acceptControl, hasStorageWarning: !!snapshot.storageWarning }
  await writeFile(join(workspace, '.work', 'interactive-state.json'), JSON.stringify(result, null, 2))
  console.log(JSON.stringify(result, null, 2))
} finally {
  await browser.close().catch(() => {})
}
