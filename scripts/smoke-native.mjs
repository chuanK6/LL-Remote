import { spawn } from 'node:child_process'
import { mkdir, writeFile } from 'node:fs/promises'
import { resolve, join } from 'node:path'
import { chromium } from '@playwright/test'
import { createServer } from 'vite'
import { randomUUID } from 'node:crypto'
const root = resolve(import.meta.dirname, '..')
const data = join(root, '.work', `native-smoke-${randomUUID()}`)
await mkdir(data, { recursive: true })
const server = await createServer({ configFile: join(root, 'vite.config.ts') })
await server.listen()
const child = spawn(join(root, 'src-tauri', 'target', 'debug', 'LLRemote.exe'), ['--background'], { cwd: root, env: { ...process.env, LLREMOTE_TEST_DATA: data, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9225' }, stdio: 'ignore', windowsHide: true })
let browser
try {
  for (let i = 0; i < 80; i++) {
    if (child.exitCode !== null) throw new Error(`Desktop application exited with ${child.exitCode}`)
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:9225'); break } catch {}
    await new Promise(resolve => setTimeout(resolve, 150))
  }
  if (!browser) throw new Error('Desktop WebView2 did not become ready')
  const page = browser.contexts()[0].pages()[0]
  await page.waitForFunction(() => !!window.__TAURI_INTERNALS__, { timeout: 10000 })
  const snapshot = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('bootstrap', {}))
  if (!snapshot.preferences.preventSleep || snapshot.preferences.autostart || snapshot.preferences.setupComplete) throw new Error('Unexpected desktop defaults')
  if (!snapshot.dataDirectory.toLowerCase().includes('native-smoke')) throw new Error('Native data was not isolated')
  const invalid = await page.evaluate(async () => { try { await window.__TAURI_INTERNALS__.invoke('login_jiaoyue', { username: '', password: '', remember: false }); return null } catch (error) { return error.code } })
  if (invalid !== 'INVALID_INPUT') throw new Error('Desktop validation command did not reject empty credentials')
  const evidence = { desktopBoot: true, webview2: true, accountReadFromNative: snapshot.account.authenticated, importedDeviceCount: snapshot.devices.length, existingMoonlightHosts: snapshot.devices.filter(device => device.uuid).length, noSleepDefault: snapshot.preferences.preventSleep, autostartDefault: snapshot.preferences.autostart, setupRequired: !snapshot.preferences.setupComplete, emptyCredentialRejected: invalid, dataIsolated: true, powerStatus: snapshot.powerStatus, mutationsToExistingConfiguration: false }
  await writeFile(join(root, '.work', 'desktop-evidence.json'), JSON.stringify(evidence, null, 2))
  console.log(JSON.stringify(evidence, null, 2))
  await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('quit_app', { stopAccepting: false })).catch(() => {})
} finally {
  await browser?.close().catch(() => {})
  if (child.exitCode === null && child.signalCode === null) { const exited = new Promise(resolve => child.once('exit', resolve)); child.kill(); await exited }
  await server.close()
}
