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
const executableBytes = await readFile(process.env.LLREMOTE_SMOKE_EXE || fixtureExecutable)
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
const cached = process.env.LLREMOTE_SMOKE_FIXTURE
if (cached) {
  await cp(join(cached, 'Data'), join(root, 'Data'), { recursive: true, filter: source => !/[\\/]WebView2(?:[\\/]|$)/.test(source) })
}
if (!existingSunshine) {
  await writeFile(join(root, 'Data', 'preferences.json'), JSON.stringify({ reuseExisting: false }))
  const sunshinePort = await sunshineWebPort()
  const configRoot = join(root, 'Data', 'engines', 'Sunshine', 'config')
  await mkdir(join(configRoot, 'credentials'), { recursive: true })
  const path = name => join(configRoot, name).replaceAll('\\', '/')
  await writeFile(path('sunshine.conf'), `locale = zh\nport = ${sunshinePort - 1}\ncredentials_file = ${path('sunshine_state.json')}\nfile_state = ${path('sunshine_state.json')}\nfile_apps = ${path('apps.json')}\ncert = ${path('credentials/cacert.pem')}\npkey = ${path('credentials/cakey.pem')}\nlog_path = ${path('sunshine.log')}\n`)
  await writeFile(path('apps.json'), JSON.stringify({ env: {}, apps: [{ name: 'Desktop' }] }))
}

const streamAddress = '127.0.0.1:' + (await readFile(join(root, 'Data/engines/Sunshine/config/sunshine.conf'), 'utf8')).match(/port = (\d+)/)[1]
let connected = false, disconnects = 0
const separator = '<$!$>'
const script = '// isolated native protocol fixture\n'
let authenticated = true
let localAuthorized = true
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
      socket.send(['pclist', 'fixture-peer', 'Fixture PC', '', '', connected ? '10.8.0.2' : '', '', '', connected ? '1' : '', connected ? 'fixture-peer' : ''].join(separator))
    } else if (fields[0] === 'conpc') {
      connected = true; socket.send(['conpc', 'yes', 'fixture-peer', '10.8.0.2'].join(separator))
    } else if (fields[0] === 'pcinfo') {
      socket.send(['pcinfo', 'fixture-peer', '', 'Fixture PC', 'VPN|' + streamAddress].join(separator))
    } else if (fields[0] === 'stopcon') {
      connected = false; disconnects++; socket.send('discon')
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
const child = spawn(join(root, 'LLRemote.exe'), [], {
  cwd: root, stdio: 'ignore', windowsHide: true,
  env: { ...process.env, __COMPAT_LAYER: 'RunAsInvoker', WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
})
let browser, invoke
let startupError
child.on('error', error => { startupError = error })
try {
  const deadline = Date.now() + 45000
  while (Date.now() < deadline) {
    if (startupError) throw startupError
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
  const username = 'll-stream-smoke'
  const password = 'Fixture-' + randomUUID()
  if (!cached) await invoke('setup_sunshine', { username, password, confirmPassword: password, mode: 'create' })
  const host = process.env.LLREMOTE_GEOMETRY_ONLY ? await invoke('control_sunshine', { action: 'start' }) : await invoke('set_accept_control', { enabled: true, password: 'fixture-control-password', webPassword: 'web123' })
  if (!host.sunshine.ready || (!serverRunning && !process.env.LLREMOTE_GEOMETRY_ONLY)) throw new Error('Sunshine was not started for a native session')
  const device = host.devices.find(value => value.natId === 'fixture-peer')
  if (!device) throw new Error('Missing fixture device')
  const preferences = { ...device.preferences, width: 1280, height: 720, fps: 30, bitrateKbps: 5000, codec: 'h264', windowMode: 'windowed', mouseMode: 'office' }
  await invoke('save_stream_preferences', { deviceId: device.id, preferences })
  const openStream = async () => {
    const session = await invoke('connect_device', { request: { deviceId: device.id, address: streamAddress, app: 'Desktop' } })
    console.log(JSON.stringify({ phase: 'connecting', id: session.id }))
    let confirmed = false
    await expect.poll(async () => {
      const current = await invoke('get_session')
      if (current.error) throw new Error(JSON.stringify(current.error))
      if (current.pin && !confirmed) {
        const snapshot = await invoke('bootstrap')
        if (snapshot.sunshine.pendingPairings.length) {
          await invoke('confirm_sunshine_pin', { pin: current.pin, name: '', pairingId: snapshot.sunshine.pendingPairings[0].id })
          confirmed = true
        }
      }
      return current.stage
    }, { timeout: 45000 }).toBe('streaming')
    const context = browser.contexts()[0]
    let panel
    await expect.poll(() => {
      panel = context.pages().find(candidate => candidate !== page)
      return !!panel
    }).toBe(true)
    await panel.locator('.session-ball').waitFor({ timeout: 10000 })
    return panel
  }
  let panel = await openStream()
  const ball = panel.locator('.session-ball')
  const card = panel.locator('.session-settings')
  console.log(JSON.stringify({ phase: 'streaming', panelSize: await panel.evaluate(() => [innerWidth, innerHeight]) }))
  const bounds = async (moveBy = 0) => JSON.parse((await execFileAsync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', join(workspace, '.work', 'session-window-bounds.ps1'), '-AppId', String(child.pid), '-MoveBy', String(moveBy)])).stdout)
  await invoke('show_session_controls')
  await expect.poll(async () => {
    const state = await bounds()
    return state.ball[0] >= state.stream[0] && state.ball[2] === state.stream[2]
  }, { timeout: 8000 }).toBe(true)
  const ballBefore = await bounds()
  const ballAfter = await bounds(-44)
  for (const { stream, ball } of [ballBefore, ballAfter]) {
    if (ball[0] < stream[0] || ball[1] < stream[1] || ball[2] !== stream[2] || ball[3] > stream[3] || ball[2] - ball[0] !== ball[3] - ball[1]) throw new Error('Ball did not fit at the client edge: ' + JSON.stringify({ stream, ball }))
  }
  if (ballAfter.ball[0] - ballBefore.ball[0] !== -44) throw new Error('Ball did not follow the stream')
  console.log(JSON.stringify({ phase: 'ballGeometry', ballBefore, ballAfter }))
  await ball.click()
  await expect(card).toBeVisible()
  await panel.locator('.session-surface').click({ position: { x: 1, y: 1 } })
  await expect(card).toHaveCount(0)
  await ball.click()
  const buttons = panel.locator('.segments').nth(1).locator('button')
  for (const [index, mode] of [[2, 'borderless'], [1, 'windowed'], [0, 'fullscreen'], [1, 'windowed']]) {
    await buttons.nth(index).click()
    await expect.poll(async () => (await invoke('get_session')).preferences.windowMode).toBe(mode)
    if (!(await card.isVisible())) await ball.click()
  }
  await panel.locator('.segments').first().locator('button').first().click()
  await expect.poll(async () => (await invoke('get_session')).preferences.mouseMode).toBe('game')
  await panel.locator('.plain-icon').click()
  await ball.click()
  await panel.locator('.disconnect-button').click()
  await expect.poll(async () => (await invoke('get_session')).stage).toBe('disconnected')
  if (disconnects !== 1) throw new Error('The disconnect button did not stop the fixture network connection')
  panel = await openStream()
  const ended = new Promise(resolve => child.once('exit', resolve))
  await invoke('quit_app', { stopAccepting: false }).catch(() => {})
  const exit = await Promise.race([ended, new Promise(resolve => setTimeout(() => resolve('timeout'), 20000))])
  const { stdout: survivors } = await execFileAsync('powershell.exe', ['-NoProfile', '-Command', "Get-CimInstance Win32_Process -Filter \"Name='Moonlight.exe' OR Name='sunshine.exe'\" | Select-Object -ExpandProperty ExecutablePath"])
  if (exit !== 0 || survivors.toLowerCase().includes(root.toLowerCase()) || serverRunning || disconnects !== 2) throw new Error(JSON.stringify({ exit, serverRunning, disconnects }))
  console.log(JSON.stringify({ nativeSession: true, ballDockedAndFollowsWindow: true, displayModes: true, mouseSwitch: true, panelBlankClick: true, disconnectButton: true, exitWithActiveStream: true, sunshineExited: true, networkStopConfirmed: true }))
} finally {
  if (child.exitCode === null && child.signalCode === null && child.pid) {
    await invoke?.('quit_app', { stopAccepting: false }).catch(() => {})
    child.kill()
  }
  await browser?.close().catch(() => {})
  for (const socket of sockets.clients) socket.terminate()
  server.closeAllConnections()
  server.close()
}
