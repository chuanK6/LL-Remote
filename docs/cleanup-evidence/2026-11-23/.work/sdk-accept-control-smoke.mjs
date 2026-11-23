import { cp, mkdir, readFile, readdir, writeFile, stat } from 'node:fs/promises'
import { resolve, join, basename, sep } from 'node:path'
import { spawn, execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { randomUUID, createHash } from 'node:crypto'
import { createServer } from 'node:net'
import { chromium } from '@playwright/test'

const workspace = resolve(import.meta.dirname, '..')
const packageRoot = join(workspace, 'release', 'LLRemote-Portable-x64-0.1.6')
const userData = join(workspace, 'release', 'LLRemote-Portable-x64-0.1.5', 'Data')
const fixture = join(workspace, '.work', `原生接受控制-${randomUUID()}`)
const evidencePath = join(workspace, '.work', 'sdk-accept-control-evidence.json')
const run = promisify(execFile)
const wait = ms => new Promise(resolve => setTimeout(resolve, ms))
async function sourceDataDigest() {
  const digest = createHash('sha256')
  async function readTree(path) {
    const info = await stat(path)
    if (info.isDirectory()) {
      for (const entry of (await readdir(path)).sort()) await readTree(join(path, entry))
    } else { digest.update(path.slice(userData.length)); digest.update(await readFile(path)) }
  }
  for (const name of ['preferences.json', 'credentials', 'accounts', 'engines/Sunshine/config']) await readTree(join(userData, name))
  return digest.digest('hex')
}
const originalDigest = await sourceDataDigest()
const secretReference = createHash('sha256').update('LLRemote-v1:local:jiaoyue-login').digest('hex')
await stat(join(userData, 'credentials', `${secretReference}.bin`))
const { stdout: existing } = await run('powershell.exe', ['-NoProfile', '-Command', "@(Get-CimInstance Win32_Process -Filter \"Name='LLRemote.exe'\").Count"], { windowsHide: true })
if (Number(existing.trim()) !== 0) throw new Error('An existing desktop instance must remain untouched')
await mkdir(fixture, { recursive: true })
for (const entry of await readdir(packageRoot, { withFileTypes: true })) {
  if (entry.isFile()) await cp(join(packageRoot, entry.name), join(fixture, entry.name))
}
for (const name of ['engines', 'notices', 'component-tools']) await cp(join(packageRoot, name), join(fixture, name), { recursive: true })
const data = join(fixture, 'Data')
await mkdir(data, { recursive: true })
for (const name of ['credentials', 'accounts']) await cp(join(userData, name), join(data, name), { recursive: true })
await cp(join(userData, 'engines', 'Sunshine', 'config'), join(data, 'engines', 'Sunshine', 'config'), { recursive: true })
const preferences = JSON.parse(await readFile(join(userData, 'preferences.json'), 'utf8'))
Object.assign(preferences, { autostart: false, preventSleep: false, keepDisplayAwake: false, acceptControl: false, reuseExisting: false, showFloatBall: false, closeToTray: false })
await writeFile(join(data, 'preferences.json'), JSON.stringify(preferences, null, 2))
const configPath = join(data, 'engines', 'Sunshine', 'config', 'sunshine.conf')
let config = await readFile(configPath, 'utf8')
const localConfig = join(data, 'engines', 'Sunshine', 'config')
const fixedPaths = {
  credentials_file: join(localConfig, 'sunshine_state.json'),
  file_state: join(localConfig, 'sunshine_state.json'),
  file_apps: join(localConfig, 'apps.json'),
  cert: join(localConfig, 'credentials', 'cacert.pem'),
  pkey: join(localConfig, 'credentials', 'cakey.pem'),
  log_path: join(localConfig, 'sunshine.log'),
}
for (const [name, path] of Object.entries(fixedPaths)) {
  const line = `${name} = ${path.replaceAll('\\', '/')}`
  const pattern = new RegExp(`^\\s*${name}\\s*=.*$`, 'gm')
  config = pattern.test(config) ? config.replace(pattern, () => line) : `${config}\n${line}\n`
}
await writeFile(configPath, config)
const portServer = createServer()
await new Promise((resolve, reject) => { portServer.once('error', reject); portServer.listen(0, '127.0.0.1', resolve) })
const debugPort = portServer.address().port
await new Promise(resolve => portServer.close(resolve))
const evidence = { version: '0.1.6', startedUtc: new Date().toISOString(), fixture, sourceUserDataUnchanged: true, copiedEncryptedUserCredentials: true, originalUserDataLoadedByApplication: false, releaseBoot: false, systemWebView2: false, actualSdk: true, actualSunshine: true, authenticated: false, cycles: [], error: null, applicationExited: false, ownedProcessesExited: false }
await writeFile(evidencePath, JSON.stringify(evidence, null, 2))
const child = spawn(join(fixture, 'LLRemote.exe'), ['--background'], {
  cwd: fixture,
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}` },
  stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true,
})
let childError = null
child.on('error', error => { childError = error.code || 'SPAWN_FAILED' })
child.stderr.on('data', () => {})
let browser, page
try {
  const deadline = Date.now() + 40000
  while (Date.now() < deadline) {
    if (childError || child.exitCode !== null) throw { code: childError || 'DESKTOP_EXITED' }
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`, { timeout: 1000 }); break } catch {}
    await wait(200)
  }
  if (!browser) throw { code: 'WEBVIEW_UNAVAILABLE' }
  for (let i = 0; i < 50 && !page; i++) { page = browser.contexts()[0]?.pages()[0]; if (!page) await wait(100) }
  if (!page) throw { code: 'MAIN_PAGE_UNAVAILABLE' }
  await page.waitForFunction(() => !!window.__TAURI_INTERNALS__, null, { timeout: 15000 })
  let snapshot = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('bootstrap', {}))
  if (snapshot.version !== '0.1.6' || resolve(snapshot.dataDirectory).toLowerCase() !== data.toLowerCase()) throw { code: 'ISOLATION_FAILED' }
  evidence.releaseBoot = true
  const { stdout: browserProcess } = await run('powershell.exe', ['-NoProfile', '-Command', `[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe' AND ParentProcessId=${child.pid}" | Select-Object ExecutablePath | ConvertTo-Json -Compress`], { windowsHide: true })
  const processes = JSON.parse(browserProcess.trim() || 'null')
  const processList = Array.isArray(processes) ? processes : [processes]
  evidence.systemWebView2 = processList.some(p => p?.ExecutablePath?.toLowerCase().includes('\\microsoft\\edgewebview\\application\\'))
  if (!evidence.systemWebView2) throw { code: 'SYSTEM_WEBVIEW_NOT_USED' }
  const loginDeadline = Date.now() + 40000
  while (!snapshot.account.authenticated && Date.now() < loginDeadline) {
    await wait(500)
    snapshot = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('refresh_state', {}))
  }
  evidence.authenticated = snapshot.account.authenticated
  if (!snapshot.account.authenticated) throw { code: 'SAVED_LOGIN_NOT_RESTORED' }
  for (let cycle = 1; cycle <= 2; cycle++) {
    const started = Date.now()
    const enabled = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('set_accept_control', { enabled: true, password: '', webPassword: '' }))
    const row = { cycle, startMilliseconds: Date.now() - started, serverRunning: enabled.account.serverRunning, sunshineRunning: enabled.sunshine.running, sunshineReady: enabled.sunshine.ready, acceptControl: enabled.preferences.acceptControl, stopConfirmed: false }
    evidence.cycles.push(row)
    await writeFile(evidencePath, JSON.stringify(evidence, null, 2))
    if (!(row.serverRunning && row.sunshineRunning && row.sunshineReady && row.acceptControl)) throw { code: 'ACCEPT_CONTROL_NOT_READY' }
    const stopped = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('set_accept_control', { enabled: false, password: '', webPassword: '' }))
    row.stopConfirmed = !stopped.account.serverRunning && !stopped.sunshine.running && !stopped.preferences.acceptControl
    if (!row.stopConfirmed) throw { code: 'ACCEPT_CONTROL_STOP_NOT_CONFIRMED' }
    await writeFile(evidencePath, JSON.stringify(evidence, null, 2))
  }
} catch (error) {
  evidence.error = { code: error?.code || 'VALIDATION_FAILED', message: typeof error?.message === 'string' ? error.message.slice(0, 350) : null }
} finally {
  if (page) await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('quit_app', { stopAccepting: true })).catch(() => {})
  if (child.exitCode === null && child.signalCode === null && !childError) {
    await Promise.race([new Promise(resolve => child.once('exit', resolve)), wait(15000)])
  }
  evidence.applicationExited = childError !== null || child.exitCode !== null || child.signalCode !== null
  const fixturePrefix = (fixture + sep).replaceAll("'", "''")
  const command = `[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith('${fixturePrefix}', [System.StringComparison]::OrdinalIgnoreCase) } | Select-Object ProcessId,Name) | ConvertTo-Json -Compress`
  const { stdout: remaining } = await run('powershell.exe', ['-NoProfile', '-Command', command], { windowsHide: true })
  const owned = JSON.parse(remaining.trim() || 'null')
  evidence.ownedProcessesExited = !owned || (Array.isArray(owned) && owned.length === 0)
  if (!evidence.applicationExited || !evidence.ownedProcessesExited) evidence.error ||= { code: 'OWNED_PROCESS_CLEANUP_REQUIRED', message: null }
  evidence.finishedUtc = new Date().toISOString()
  evidence.sourceUserDataUnchanged = originalDigest === await sourceDataDigest()
  if (!evidence.sourceUserDataUnchanged) evidence.error ||= { code: 'SOURCE_DATA_CHANGED_DURING_VALIDATION', message: null }
  await writeFile(evidencePath, JSON.stringify(evidence, null, 2))
  await browser?.close().catch(() => {})
}
console.log(JSON.stringify({ ...evidence, fixture: basename(fixture) }, null, 2))
if (evidence.error) process.exitCode = 1
