import { spawn } from 'node:child_process'
import { createConnection } from 'node:net'
import { mkdir, writeFile } from 'node:fs/promises'
import { resolve, join } from 'node:path'
import { randomBytes, randomUUID } from 'node:crypto'
const root = resolve(import.meta.dirname, '..')
const data = join(root, '.work', 'moonlight-probe')
await mkdir(data, { recursive: true })
const pipe = `LLRemote-${randomUUID()}`
const token = randomBytes(32).toString('hex')
const env = { ...process.env, QT_QPA_PLATFORM: 'offscreen', QT_QUICK_BACKEND: 'software', QT_QPA_PLATFORM_PLUGIN_PATH: join(root, '.tools', 'Qt', '6.12.0', 'msvc2022_64', 'plugins', 'platforms'), LLREMOTE_MOONLIGHT_DATA_DIR: data, LLREMOTE_PIPE_NAME: pipe, LLREMOTE_SESSION_TOKEN: token }
const child = spawn(join(root, 'engines', 'Moonlight', 'Moonlight.exe'), ['--ll-settings'], { cwd: join(root, 'engines', 'Moonlight'), env, stdio: 'ignore', windowsHide: true })
const request = suppliedToken => new Promise((resolve, reject) => {
  const socket = createConnection(`\\\\.\\pipe\\${pipe}`)
  const timer = setTimeout(() => { socket.destroy(); reject(new Error('Native pipe timeout')) }, 3000)
  let response = ''
  socket.on('connect', () => socket.write(JSON.stringify({ version: 1, token: suppliedToken, action: 'status', payload: {} }) + '\n'))
  socket.on('data', bytes => { response += bytes.toString(); if (response.includes('\n')) { clearTimeout(timer); socket.destroy(); resolve(JSON.parse(response.split('\n')[0])) } })
  socket.on('error', error => { clearTimeout(timer); reject(error) })
})
try {
  let status
  for (let i = 0; i < 40; i++) {
    if (child.exitCode !== null) throw new Error(`Moonlight exited with ${child.exitCode}`)
    try { status = await request(token); if (status.settingsVisible) break } catch {}
    await new Promise(resolve => setTimeout(resolve, 150))
  }
  if (!status?.ok || !status.settingsVisible) throw new Error('Native settings view or pipe unavailable')
  const denied = await request('incorrect-token')
  if (denied.ok) throw new Error('Native pipe accepted an invalid session token')
  const evidence = { settingsDirect: status.settingsVisible, protocol: status.version, tokenRejected: !denied.ok, realSession: status.connected, metrics: status.metrics, isolatedNativeConfiguration: true }
  await writeFile(join(root, '.work', 'moonlight-evidence.json'), JSON.stringify(evidence, null, 2))
  console.log(JSON.stringify(evidence, null, 2))
} finally { if (child.exitCode === null && child.signalCode === null) { const exited = new Promise(resolve => child.once('exit', resolve)); child.kill(); await exited } }
