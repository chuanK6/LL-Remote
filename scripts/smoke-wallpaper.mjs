// Real WebView2/native wallpaper commands in a fresh copy; no user Data loaded.
import { spawn } from 'node:child_process'
import { cp, mkdir, readdir, readFile, writeFile, symlink } from 'node:fs/promises'
import { resolve, join } from 'node:path'
import { createServer } from 'node:net'
import { randomUUID } from 'node:crypto'
import { chromium, expect } from '@playwright/test'

if (!process.argv[2]) throw new Error('Usage: node scripts/smoke-wallpaper.mjs <portable directory> [--baseline]')
const baseline = process.argv.includes('--baseline')
const workspace = resolve(import.meta.dirname, '..')
const source = resolve(process.argv[2])
const root = join(workspace, '.work', `壁纸 测试-${randomUUID()}`)
await mkdir(join(root, 'Data', 'wallpapers'), { recursive: true })
for (const entry of await readdir(source, { withFileTypes: true })) {
  if (entry.isFile()) await cp(join(source, entry.name), join(root, entry.name))
}
for (const name of ['engines', 'notices', 'component-tools']) await symlink(join(source, name), join(root, name), 'junction')
const executable = join(root, 'LLRemote.exe')
const exe = await readFile(executable)
const original = Buffer.from('cn.llremote.desktop')
const isolated = Buffer.from(`cn.llremote.t${randomUUID().replaceAll('-', '').slice(0, 6)}`)
let offset = 0, count = 0
while ((offset = exe.indexOf(original, offset)) !== -1) { isolated.copy(exe, offset); offset += original.length; count++ }
if (!count) throw new Error('Cannot isolate the test instance')
await writeFile(executable, exe)
// The harness shell is not elevated. Only the test copy requests asInvoker;
// the distributed EXE keeps its administrator manifest.
const admin = Buffer.from('level="requireAdministrator"')
const invoker = Buffer.from('level="asInvoker"'.padEnd(admin.length, ' '))
const manifestOffset = exe.indexOf(admin)
if (manifestOffset < 0) throw new Error('Cannot locate the test execution manifest')
invoker.copy(exe, manifestOffset)
await writeFile(executable, exe)
await writeFile(join(root, 'Data', 'preferences.json'), JSON.stringify({ rememberLogin: false, reuseExisting: false, preventSleep: false, alwaysAcceptControl: false }))
const relative = 'wallpapers/测试 图片.png'
const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aD1sAAAAASUVORK5CYII=', 'base64')
await writeFile(join(root, 'Data', relative), png)
const oversized = join(root, '超限.png')
await writeFile(oversized, Buffer.concat([png, Buffer.alloc(5 * 1024 * 1024 + 1 - png.length)]))
const tcp = createServer()
await new Promise(resolve => tcp.listen(0, '127.0.0.1', resolve))
const port = tcp.address().port
await new Promise(resolve => tcp.close(resolve))
const child = spawn(executable, [], { cwd: root, env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` }, windowsHide: true, stdio: 'ignore' })
let startupError
child.on('error', error => { startupError = error })
let browser
try {
  const deadline = Date.now() + 45000
  while (Date.now() < deadline) {
    if (startupError) throw startupError
    if (child.exitCode !== null) throw new Error(`App exited: ${child.exitCode}`)
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break } catch {}
    await new Promise(resolve => setTimeout(resolve, 200))
  }
  if (!browser) throw new Error('Native WebView2 unavailable')
  const page = browser.contexts()[0].pages()[0]
  const errors = []
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()) })
  await page.waitForSelector('.shell', { state: 'attached' })
  const result = await page.evaluate(async ({ relative, oversized }) => {
    const ipc = window.__TAURI_INTERNALS__
    const snapshot = await ipc.invoke('bootstrap', {})
    const path = await ipc.invoke('wallpaper_path', { relative })
    const url = ipc.convertFileSrc(path, 'asset')
    const image = new Image()
    const loaded = await new Promise(resolve => { image.onload = () => resolve(image.naturalWidth > 0); image.onerror = () => resolve(false); image.src = url })
    const preview = await ipc.invoke('preview_wallpaper', { path })
    let sizeError = null
    try { await ipc.invoke('preview_wallpaper', { path: oversized }) } catch (error) { sizeError = error }
    let traversalBlocked = false
    try { await ipc.invoke('wallpaper_path', { relative: 'wallpapers/../preferences.json' }) } catch { traversalBlocked = true }
    return { version: snapshot.version, trayDefault: snapshot.preferences.keepRunningInTrayOnClose, loaded, path, url, previewIsPng: preview.startsWith('data:image/png;base64,'), sizeError, traversalBlocked }
  }, { relative, oversized })
  console.log(JSON.stringify({ ...result, errors }, null, 2))
  if (!baseline) {
    expect(result.loaded).toBe(true)
    expect(result.previewIsPng).toBe(true)
    expect(result.trayDefault).toBe(true)
    expect(result.sizeError?.message).toContain('5 MB')
    expect(result.traversalBlocked).toBe(true)
    await page.goto(page.url().split('#')[0] + '#/app/moonlight')
    const bitrate = page.getByRole('spinbutton', { name: '码率数值（Mbps）' })
    await expect(bitrate).toHaveValue('15')
    await bitrate.press('ArrowUp')
    await expect(bitrate).toHaveValue('16')
    await bitrate.press('ArrowDown')
    await expect(bitrate).toHaveValue('15')
    await bitrate.blur()
    const data = { version: result.version, nativeWallpaperLoaded: true, pngPreview: true, oversizedRejected: true, pathTraversalBlocked: true, defaultCloseToTray: true, bitrateArrowStepMbps: 1, isolatedData: true, chineseAndSpacePath: true, testCopyPatches: ['single-instance identifier', 'asInvoker manifest'], distributedExeUnmodified: true, realKernelsStarted: false }
    await writeFile(join(workspace, '.work', 'wallpaper-evidence.json'), JSON.stringify(data, null, 2) + '\n')
  }
  await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('quit_app', { stopAccepting: false })).catch(() => {})
  await new Promise(resolve => { if (child.exitCode !== null) resolve(); else { child.once('exit', resolve); setTimeout(resolve, 5000) } })
} finally {
  await browser?.close().catch(() => {})
  if (child.exitCode === null && child.signalCode === null) child.kill()
}
