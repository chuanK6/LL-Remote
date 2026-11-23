// Isolated startup probe for Unicode paths; no account or connection is created.
import { spawn } from 'node:child_process'
import { mkdir, writeFile, cp } from 'node:fs/promises'
import { resolve, join } from 'node:path'
import { createServer } from 'node:net'
import { createSocket } from 'node:dgram'
import https from 'node:https'
import { randomUUID } from 'node:crypto'

const root = resolve(import.meta.dirname, '..')
const data = join(root, '.work', `Sunshine 中文路径-${randomUUID()}`)
await mkdir(join(data, 'credentials'), { recursive: true })
let base
for (let candidate = 42000; candidate < 60000; candidate += 37) {
  const tcp = [], udp = []
  let ready = false
  try {
    for (const offset of [-5, 0, 1, 21]) {
      const server = createServer()
      await new Promise((resolve, reject) => { server.once('error', reject); server.listen(candidate + offset, '0.0.0.0', resolve) })
      tcp.push(server)
    }
    for (const offset of [9, 10, 11, 13]) {
      const socket = createSocket('udp4'); udp.push(socket)
      await new Promise((resolve, reject) => { socket.once('error', reject); socket.bind(candidate + offset, '0.0.0.0', resolve) })
    }
    ready = true
  } catch {} finally {
    await Promise.allSettled(tcp.map(server => new Promise(resolve => server.close(resolve))))
    for (const socket of udp) { try { socket.close() } catch {} }
  }
  if (ready) { base = candidate; break }
}
if (!base) throw new Error('No isolated port group available')
const path = name => join(data, name).replaceAll('\\', '/')
const relative = process.argv.includes('--relative')
const nativePath = name => relative ? name : path(name)
const config = `locale = zh\nport = ${base}\ncredentials_file = ${nativePath('state.json')}\nfile_state = ${nativePath('state.json')}\nfile_apps = ${nativePath('apps.json')}\ncert = ${nativePath('credentials/cacert.pem')}\npkey = ${nativePath('credentials/cakey.pem')}\nlog_path = ${nativePath('sunshine.log')}\n`;
await writeFile(path('sunshine.conf'), config)
await writeFile(path('apps.json'), JSON.stringify({env:{},apps:[{name:'Desktop'}]}))
const directory = join(root, 'engines', 'Sunshine')
if (relative) await cp(join(directory, 'assets'), join(data, 'assets'), { recursive: true })
const child = spawn(join(directory, 'sunshine.exe'), [relative ? 'sunshine.conf' : path('sunshine.conf')], { cwd: relative ? data : directory, windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] })
let diagnostic = ''
child.stderr.on('data', data => { diagnostic = (diagnostic + data.toString()).slice(-16000) })
let available = false
try {
  const deadline = Date.now() + 45000
  while (Date.now() < deadline && child.exitCode === null) {
    available = await new Promise(resolve => {
      const request = https.get(`https://127.0.0.1:${base + 1}/api/configLocale`, { rejectUnauthorized: false, timeout: 500 }, response => { response.resume(); resolve(response.statusCode === 200) })
      request.on('timeout', () => { request.destroy(); resolve(false) })
      request.on('error', () => resolve(false))
    })
    if (available) break
    await new Promise(resolve => setTimeout(resolve, 150))
  }
  const evidence = { unicodeConfigPath: true, relativeNativePaths: relative, isolatedPorts: true, managementApiAvailable: available, exitCode: child.exitCode, accountCreated: false, remoteConnection: false }
  await writeFile(join(root, '.work', 'sunshine-unicode-probe.json'), JSON.stringify(evidence, null, 2))
  console.log(JSON.stringify(evidence, null, 2))
  if (process.argv.includes('--expect-error') ? available : !available) throw new Error('Unexpected Unicode startup result')
} finally {
  if (child.exitCode === null && child.signalCode === null) { const exited = new Promise(resolve => child.once('exit', resolve)); child.kill(); await exited }
}
