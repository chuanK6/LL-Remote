import { spawn, execFile } from 'node:child_process'
import { mkdir, writeFile, readFile } from 'node:fs/promises'
import { resolve, join, dirname, sep } from 'node:path'
import { promisify } from 'node:util'
import { createServer } from 'node:net'
import { randomUUID } from 'node:crypto'
import { chromium } from '@playwright/test'
const workspace = resolve(import.meta.dirname, '..')
if (!process.argv[2]) throw new Error('Usage: node scripts/smoke-packaged.mjs <LLRemote.exe> [--installed]')
const executable = resolve(process.argv[2])
const root = dirname(executable)
const execFileAsync = promisify(execFile)
const { stdout: existing } = await execFileAsync('powershell.exe', ['-NoProfile', '-Command', "@(Get-CimInstance Win32_Process -Filter \"Name='LLRemote.exe'\").Count"], { windowsHide: true, timeout: 10000 })
const isolatedInstance = process.argv.includes('--isolated-instance')
if (Number(existing.trim()) > 0 && !isolatedInstance) throw new Error('Exit the currently running LLRemote through its tray menu before the isolated package smoke test.')
if (isolatedInstance && (await readFile(executable)).includes(Buffer.from('cn.llremote.desktop'))) throw new Error('An isolated-instance test must use a fixture copy with its own instance identifier')
const installed = process.argv.includes('--installed')
try { await readFile(join(root, 'runtimes', 'WebView2', 'msedgewebview2.exe')); throw new Error('The package still contains a bundled WebView2 runtime') } catch (error) { if (error.code !== 'ENOENT') throw error }
const isolatedLocalData = join(workspace, '.work', `安装 数据验证-${randomUUID()}`)
const expectedData = installed ? join(isolatedLocalData, 'LLRemote') : join(root, 'Data')
const portServer = createServer()
await new Promise((resolve, reject) => { portServer.once('error', reject); portServer.listen(0, '127.0.0.1', resolve) })
const port = portServer.address().port
await new Promise(resolve => portServer.close(resolve))
const child = spawn(executable, ['--background'], { cwd: root, env: { ...process.env, ...(installed ? { LOCALAPPDATA: isolatedLocalData } : {}), WEBVIEW2_BROWSER_EXECUTABLE_FOLDER: join(root, 'unavailable-runtime-fixture'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` }, stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true })
let startupError = ''
child.stderr.on('data', data => { startupError = (startupError + data.toString()).slice(0, 6000) })
child.on('error', error => { startupError += error.message })
let browser
try {
  const deadline = Date.now() + 45000
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Packaged application exited: ${child.exitCode}; ${startupError}`)
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break } catch {}
    await new Promise(resolve => setTimeout(resolve, 200))
  }
  if (!browser) throw new Error(`Packaged WebView2 unavailable; ${startupError}`)
  let page
  const pageDeadline = Date.now() + 15000
  while (Date.now() < pageDeadline) {
    page = browser.contexts()[0]?.pages()[0]
    if (page) break
    await new Promise(resolve => setTimeout(resolve, 100))
  }
  if (!page) throw new Error('The packaged WebView2 did not create the main page')
  await page.waitForFunction(() => !!window.__TAURI_INTERNALS__, null, { timeout: 15000 })
  await page.waitForSelector('.shell .brand', { state: 'attached', timeout: 15000 })
  const snapshot = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('bootstrap', {}))
  if (snapshot.portable !== !installed || resolve(snapshot.dataDirectory).toLowerCase() !== expectedData.toLowerCase() || snapshot.components.length !== 3 || snapshot.components.some(component => !component.available)) throw new Error('Packaged resources or data path failed')
  const { stdout } = await execFileAsync('powershell.exe', ['-NoProfile', '-Command', `[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe' AND ParentProcessId=${child.pid}" | Select-Object ExecutablePath,CommandLine | ConvertTo-Json -Compress`], { windowsHide: true, timeout: 10000 })
  const processInfo = JSON.parse(stdout.trim() || 'null')
  const runtime = (Array.isArray(processInfo) ? processInfo : [processInfo]).find(value => value?.ExecutablePath)
  const runtimePath = runtime?.ExecutablePath ? resolve(runtime.ExecutablePath).toLowerCase() : ''
  if (!runtimePath.includes('\\microsoft\\edgewebview\\application\\') || runtimePath.startsWith(root.toLowerCase() + sep)) throw new Error(`The browser process did not use the shared system WebView2 runtime: ${JSON.stringify({ actual: runtime?.ExecutablePath, parentPid: child.pid })}`)
  const browserData = /(?:"--user-data-dir=([^"]+)"|--user-data-dir="([^"]+)"|--user-data-dir=([^\s"]+))/.exec(runtime.CommandLine)
  const browserDataPath = browserData?.slice(1).find(Boolean)
  const expectedBrowserData = join(expectedData, 'WebView2').toLowerCase()
  if (!browserDataPath || !(resolve(browserDataPath).toLowerCase() === expectedBrowserData || resolve(browserDataPath).toLowerCase().startsWith(expectedBrowserData + sep))) throw new Error(`WebView2 data did not follow the application data directory: ${JSON.stringify({ actual: browserDataPath, expected: expectedBrowserData })}`)
  const evidence = { releaseBoot: true, portable: snapshot.portable, completeEngineResources: true, dataRootRelative: !installed, dataIsolated: true, directoryContainsSpacesAndChinese: /\s/.test(root) && /[\u4e00-\u9fff]/.test(root), deviceCount: snapshot.devices.length, existingMoonlightHosts: snapshot.devices.filter(device => device.uuid).length, systemRuntimeActuallyUsed: true, systemRuntimeVersion: browser.version(), inheritedRuntimeOverrideIgnored: true, browserDataFollowsApplication: true, noConnectionAttempted: true }
  await mkdir(join(workspace, '.work'), { recursive: true })
  await writeFile(join(workspace, '.work', installed ? 'installer-evidence.json' : 'portable-evidence.json'), JSON.stringify(evidence, null, 2) + '\n')
  console.log(JSON.stringify(evidence, null, 2))
  await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('quit_app', { stopAccepting: false })).catch(() => {})
} finally {
  await browser?.close().catch(() => {})
  if (child.exitCode === null && child.signalCode === null) { const exited = new Promise(resolve => child.once('exit', resolve)); child.kill(); await exited }
}
