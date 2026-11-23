// Real packaged Tauri/WebView2 + an isolated bundled Sunshine + a local native
// protocol fixture. --existing-sunshine uses an installed service read-only.
import { spawn, execFile } from 'node:child_process'
import { cp, mkdir, readFile, writeFile, readdir, symlink } from 'node:fs/promises'
import { resolve, join } from 'node:path'
import { createServer } from 'node:http'
import https from 'node:https'
import { createServer as createTcpServer } from 'node:net'
import { createSocket } from 'node:dgram'
import { randomUUID, createHash } from 'node:crypto'
import { promisify } from 'node:util'
import { WebSocketServer } from 'ws'
import { chromium, expect } from '@playwright/test'

const workspace = resolve(import.meta.dirname, '..')
if (!process.argv[2]) throw new Error('Usage: node scripts/smoke-onboarding.mjs <portable directory>')
const source = resolve(process.argv[2])
const existingSunshine = process.argv.includes('--existing-sunshine')
async function sunshineWebPort() {
  for (let base = 20000; base < 62000; base += 37) {
    const tcp = [], udp = []
    let available = false
    try {
      for (const offset of [-5, 0, 1, 21]) {
        const socket = createTcpServer()
        await new Promise((resolve, reject) => { socket.once('error', reject); socket.listen({ port: base + offset, host: '0.0.0.0', exclusive: true }, resolve) })
        tcp.push(socket)
      }
      for (const offset of [9, 10, 11, 13]) {
        const socket = createSocket('udp4')
        udp.push(socket)
        await new Promise((resolve, reject) => { socket.once('error', reject); socket.bind({ port: base + offset, address: '0.0.0.0', exclusive: true }, resolve) })
      }
      available = true
    } catch {} finally {
      await Promise.allSettled(tcp.map(socket => new Promise(resolve => socket.close(resolve))))
      for (const socket of udp) { try { socket.close() } catch {} }
    }
    if (available) return base + 1
  }
  throw new Error('No isolated Sunshine port group is available')
}
const root = join(workspace, '.work', `首用 通道验证-${randomUUID()}`)
const execFileAsync = promisify(execFile)
await mkdir(join(root, 'engines'), { recursive: true })
for (const entry of await readdir(source, { withFileTypes: true })) {
  if (entry.isFile()) await cp(join(source, entry.name), join(root, entry.name))
}
// Isolate only the test copy's single-instance identifier. The distributed
// executable is unchanged and the user's running main window stays active.
const fixtureExecutable = join(root, 'LLRemote.exe')
const executableBytes = await readFile(fixtureExecutable)
const identifier = Buffer.from('cn.llremote.desktop')
const isolatedIdentifier = Buffer.from(`cn.llremote.t${randomUUID().replaceAll('-', '').slice(0, 6)}`)
let identifiersReplaced = 0, offset = 0
while ((offset = executableBytes.indexOf(identifier, offset)) !== -1) {
  isolatedIdentifier.copy(executableBytes, offset)
  offset += identifier.length
  identifiersReplaced++
}
if (identifiersReplaced === 0) throw new Error('Cannot isolate the test copy single-instance namespace')
await writeFile(fixtureExecutable, executableBytes)
for (const name of ['notices', 'component-tools']) await symlink(join(source, name), join(root, name), 'junction')
for (const name of ['Sunshine', 'Jiaoyuelian']) await cp(join(source, 'engines', name), join(root, 'engines', name), { recursive: true })
// This native Moonlight copy uses its own portable settings, even if the PC has
// existing registry hosts. The test never writes the user's native settings.
await cp(join(source, 'engines', 'Moonlight'), join(root, 'engines', 'Moonlight'), { recursive: true })
await writeFile(join(root, 'engines', 'Moonlight', 'portable.dat'), '')
await mkdir(join(root, 'Data'), { recursive: true })
if (!existingSunshine) {
  await writeFile(join(root, 'Data', 'preferences.json'), JSON.stringify({ reuseExisting: false }))
  const sunshinePort = await sunshineWebPort()
  const configRoot = join(root, 'Data', 'engines', 'Sunshine', 'config')
  await mkdir(join(configRoot, 'credentials'), { recursive: true })
  const path = name => join(configRoot, name).replaceAll('\\', '/')
  await writeFile(path('sunshine.conf'), `locale = zh\nport = ${sunshinePort - 1}\ncredentials_file = ${path('sunshine_state.json')}\nfile_state = ${path('sunshine_state.json')}\nfile_apps = ${path('apps.json')}\ncert = ${path('credentials/cacert.pem')}\npkey = ${path('credentials/cakey.pem')}\nlog_path = ${path('sunshine.log')}\n`)
  await writeFile(path('apps.json'), JSON.stringify({ env: {}, apps: [{ name: 'Desktop' }] }))
}

const separator = '<$!$>'
const script = '// isolated native protocol fixture\n'
let authenticated = false
let localAuthorized = false
let vpn = false, serverRunning = false, serverPassword = '', webPassword = ''
let loginRequests = 0
let acceptedLogin = false
const fixturePassword = `Fixture-${randomUUID()}`
const frame = () => authenticated
  ? (serverRunning ? ['2','fixture-account','0','12345678',`VPN|10.8.0.1|all|all|all|${webPassword}`,'','',serverPassword,'3','10.8.0.1','Fixture local','0',''] : ['1','fixture-account','0','12345678',vpn ? `VPN||all|all|all|${webPassword}` : '',serverPassword,'3','','Fixture local','0','']).join(separator)
  : ['0', '', '', '', '', ''].join(separator)
const server = createServer((request, response) => {
  response.setHeader('Content-Type', 'text/javascript')
  response.end(request.url === '/web/script.js' ? script : '')
})
const sockets = new WebSocketServer({ server, path: '/ws' })
sockets.on('connection', socket => {
  socket.send(localAuthorized ? frame() : 'authorize')
  socket.on('message', data => {
    const fields = data.toString().split(separator)
    if (fields[0] === 'authorize') {
      if (fields[1] !== 'fixture-local-password') { socket.send('authorize'); return }
      localAuthorized = true
      socket.send(frame())
    } else if (fields[0] === 'login') {
      if (!localAuthorized) { socket.send('authorize'); return }
      loginRequests++
      if (fields[1] !== 'fixture-account' || fields[2] !== fixturePassword) { socket.send(['info', '账号或密码错误'].join(separator)); return }
      authenticated = true
      acceptedLogin = true
      for (const client of sockets.clients) client.send(frame())
    } else if (fields[0] === 'pclist') {
      socket.send(['pclist', 'fixture-peer', 'Fixture PC', '', '', '', '', '', '', ''].join(separator))
    } else if (fields[0] === 'VPN') {
      vpn = fields[1] === '1'; socket.send(frame())
    } else if (fields[0] === 'startServer') {
      if (!vpn || !fields[1] || !fields[3] || fields[3].length < 6 || fields[3].length > 20) { socket.send(['info','组网页面访问密码错误'].join(separator)); return }
      serverPassword = fields[1]; webPassword = fields[3]; serverRunning = true; socket.send(frame())
    } else if (fields[0] === 'stopServer') {
      serverRunning = false; socket.send(frame())
    }
  })
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const manifest = JSON.parse(await readFile(join(source, 'engines', 'manifest.json'), 'utf8'))
manifest.jiaoyuelian.port = server.address().port
manifest.jiaoyuelian.scriptSha256 = createHash('sha256').update(script).digest('hex')
await writeFile(join(root, 'engines', 'manifest.json'), JSON.stringify(manifest, null, 2))
const portServer = createServer()
await new Promise(resolve => portServer.listen(0, '127.0.0.1', resolve))
const port = portServer.address().port
await new Promise(resolve => portServer.close(resolve))
const child = spawn(join(root, 'LLRemote.exe'), ['--background'], {
  cwd: root, stdio: 'ignore', windowsHide: true,
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
})
let browser, invoke
try {
  const deadline = Date.now() + 45000
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Packaged LLRemote exited: ${child.exitCode}`)
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break } catch {}
    await new Promise(resolve => setTimeout(resolve, 150))
  }
  if (!browser) throw new Error('Packaged WebView2 unavailable')
  let page
  const pageDeadline = Date.now() + 15000
  while (Date.now() < pageDeadline) {
    page = browser.contexts()[0]?.pages()[0]
    if (page) break
    await new Promise(resolve => setTimeout(resolve, 100))
  }
  if (!page) throw new Error('The packaged WebView2 did not create the main page')
  await page.waitForSelector('.shell .brand', { state: 'attached', timeout: 15000 })
  invoke = async (command, args = {}) => {
    const result = await page.evaluate(async ({ command, args }) => {
      try { return { value: await window.__TAURI_INTERNALS__.invoke(command, args) } }
      catch (error) { return { error: { code: error.code, message: error.message } } }
    }, { command, args })
    if (result.error) throw new Error(`${command}: ${result.error.code}: ${result.error.message}`)
    return result.value
  }
  if (!existingSunshine) {
    await expect(page.getByRole('heading', { name: '首次使用 · 设置 Sunshine 账号' })).toBeVisible()
    const started = await invoke('control_sunshine', { action: 'start' })
    const password = `Fixture-${randomUUID()}`
    const account = await new Promise((resolve, reject) => {
      const body = JSON.stringify({ currentUsername: '', currentPassword: '', newUsername: 'sunshine-fixture', newPassword: password, confirmNewPassword: password })
      const request = https.request(`https://127.0.0.1:${started.sunshine.port}/api/password`, { method: 'POST', rejectUnauthorized: false, headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) } }, response => {
        let text = ''
        response.on('data', chunk => { text += chunk })
        response.on('end', () => { try { resolve(JSON.parse(text)) } catch { reject(new Error('Fixture account response was invalid')) } })
      })
      request.on('error', reject)
      request.end(body)
    })
    if (account.status !== true) throw new Error('The isolated browser-style Sunshine account was not created')
  }
  const first = await invoke('bootstrap')
  if (!first.portable || first.sunshine.external !== existingSunshine || !first.sunshine.ready || !first.sunshine.accountInitialized || !first.preferences.setupComplete || first.sunshine.credentialValid) throw new Error('The native Sunshine account was not synchronized without importing its password')
  if (!existingSunshine) await page.getByRole('button', { name: '继续登录皎月连', exact: true }).click()
  await expect(page.getByRole('heading', { name: '登录皎月连', exact: true })).toBeVisible()
  await expect(page.getByText('第一步已完成，已同步 Sunshine 的实际账号状态。', { exact: true })).toBeVisible()
  await page.getByRole('button', { name: 'Moonlight', exact: true }).click()
  await expect(page.getByRole('heading', { name: 'Moonlight · 常用设置' })).toBeVisible()
  await expect(page.locator('.topbar').getByRole('button', { name: '完成首次连接', exact: true })).toBeVisible()
  await expect(page.getByRole('button', { name: '取消向导', exact: true })).toHaveCount(0)
  await page.locator('.topbar').getByRole('button', { name: '完成首次连接', exact: true }).click()
  await expect(page.getByRole('heading', { name: '登录皎月连', exact: true })).toBeVisible()
  await page.getByLabel('皎月连账号', { exact: true }).fill('fixture-account')
  await page.getByLabel('密码', { exact: true }).fill('incorrect-fixture-password')
  await page.getByLabel('在本机记住登录').uncheck()
  await page.getByRole('button', { name: '登录并继续', exact: true }).click()
  await expect(page.getByLabel('本地管理访问密码', { exact: true })).toBeVisible()
  if (loginRequests !== 0) throw new Error('The background authorization challenge did not block credentials until local verification')
  await page.getByLabel('本地管理访问密码', { exact: true }).fill('incorrect-local-fixture-password')
  await page.getByRole('button', { name: '验证访问密码', exact: true }).click()
  await expect(page.getByRole('alert')).toContainText('访问密码验证失败')
  await page.getByLabel('本地管理访问密码', { exact: true }).fill('fixture-local-password')
  await page.getByRole('button', { name: '验证访问密码', exact: true }).click()
  await expect(page.getByLabel('本地管理访问密码', { exact: true })).toHaveCount(0)
  await page.getByRole('button', { name: '登录并继续', exact: true }).click()
  await expect(page.getByRole('alert')).toContainText('账号或密码不正确')
  if (loginRequests !== 1) throw new Error('Login was blocked before reaching the native account channel')
  await page.getByLabel('密码', { exact: true }).fill(fixturePassword)
  await page.getByRole('button', { name: '登录并继续', exact: true }).click()
  await expect(page.getByRole('heading', { name: '全部设备', exact: true })).toBeVisible()
  await expect(page.getByRole('cell', { name: /Fixture PC/ })).toBeVisible()
  await expect(page.locator('.topbar').getByRole('button', { name: '完成首次连接', exact: true })).toBeVisible()
  const loggedIn = await invoke('bootstrap')
  if (!acceptedLogin || !loggedIn.account.authenticated || !loggedIn.preferences.onboardingComplete) throw new Error('Completed onboarding was not shared with the native login state')
  const persisted = JSON.parse(await readFile(join(root, 'Data', 'preferences.json'), 'utf8'))
  if (!persisted.onboardingComplete) throw new Error('Onboarding completion was not persisted')
  if (persisted.firstConnectionComplete) throw new Error('Account login must not finish the first connection')
  const accepting = await invoke('set_accept_control', { enabled: true, password: 'fixture-control-password', webPassword: 'web123' })
  if (!accepting.account.serverRunning || !accepting.preferences.acceptControl) throw new Error('The local accepting-control workflow did not start')
  await expect(page.locator('.topbar').getByRole('button', { name: '完成首次连接', exact: true })).toBeVisible()
  await invoke('set_accept_control', { enabled: false, password: '', webPassword: '' })

  const device = loggedIn.devices.find(device => device.natId === 'fixture-peer')
  const settings = { ...device.preferences, width: 2560, height: 1440, fps: 120, bitrateKbps: 42000, codec: 'av1', windowMode: 'borderless' }
  await invoke('save_stream_preferences', { deviceId: device.id, preferences: settings })
  const ini = join(root, ...(existingSunshine ? ['engines', 'Moonlight'] : ['Data', 'engines', 'Moonlight']), 'Moonlight Game Streaming Project', 'Moonlight.ini')
  const saved = await readFile(ini, 'utf8')
  if (!saved.includes('videocfg=4') || !saved.includes('fps=120') || !saved.includes('bitrate=42000')) throw new Error('LL settings did not reach the native Moonlight configuration')
  await page.getByRole('button', { name: /Fixture PC/, exact: true }).click()
  await page.getByRole('button', { name: 'Moonlight', exact: true }).click()
  await expect(page.getByLabel('帧率', { exact: true })).toHaveValue('120')
  await writeFile(ini, saved.replace('fps=120', 'fps=90').replace('bitrate=42000', 'bitrate=37000'))
  await expect(page.getByLabel('帧率', { exact: true })).toHaveValue('90', { timeout: 12000 })
  await expect(page.getByLabel('码率', { exact: true })).toHaveValue('37')
  // An unsaved LL draft must survive native status polling.
  await page.getByLabel('帧率', { exact: true }).fill('100')
  await invoke('refresh_state')
  await expect(page.getByLabel('帧率', { exact: true })).toHaveValue('100')

  await page.getByRole('button', { name: 'fixture-account', exact: false }).click()
  authenticated = false
  for (const client of sockets.clients) client.send(frame())
  await expect(page.getByText('未登录', { exact: true })).toBeVisible({ timeout: 12000 })
  authenticated = true
  for (const client of sockets.clients) client.send(frame())
  await expect(page.getByText('已登录', { exact: true })).toBeVisible({ timeout: 12000 })
  const evidence = { version: first.version, packagedTauriBoot: true, systemWebView2: browser.version(), existingLocalSystemSunshineReadOnly: existingSunshine, isolatedBundledSunshine: !existingSunshine, sunshineAccountSynchronizedWithoutPassword: true, firstStepNoLongerBlocksNativeLogin: true, localAuthorizationVisibleAndRecoverableAfterBackgroundProbe: true, nativePasswordErrorsVisible: true, nativeLoginAndDeviceList: true, guideCollapsedAndRestoredFromTitlebar: true, firstConnectionEntryRemainsAfterLogin: true, localAcceptControlWithSeparateWebPassword: true, accountStepsPersisted: true, moonlightSettingsBothDirections: true, unsavedDraftPreservedDuringPolling: true, nativeAccountChangesReflectedWithoutNavigation: true, nativeAccountFixture: true, realRemoteAccountLogin: false, streamingTested: false }
  await writeFile(join(workspace, '.work', 'onboarding-0.1.3.json'), JSON.stringify(evidence, null, 2) + '\n')
  await writeFile(join(workspace, '.work', 'onboarding-0.1.3-path.txt'), root)
  console.log(JSON.stringify(evidence, null, 2))
  await invoke('quit_app', { stopAccepting: false }).catch(() => {})
} finally {
  if (child.exitCode === null) await invoke?.('quit_app', { stopAccepting: false }).catch(() => {})
  await browser?.close().catch(() => {})
  if (child.exitCode === null && child.signalCode === null) {
    const exited = new Promise(resolve => child.once('exit', resolve))
    child.kill()
    await exited
  }
  for (const socket of sockets.clients) socket.terminate()
  await new Promise(resolve => sockets.close(resolve))
  await new Promise(resolve => server.close(resolve))
}
