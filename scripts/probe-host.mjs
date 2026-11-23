import { spawn } from 'node:child_process'
import { mkdir, writeFile, readdir, copyFile } from 'node:fs/promises'
import { resolve, join } from 'node:path'
import WebSocket from 'ws'
const root = resolve(import.meta.dirname, '..')
const data = join(root, '.work', 'nat-host-probe')
await mkdir(data, { recursive: true })
await copyFile(join(root, 'engines', 'Jiaoyuelian', 'network.dll'), join(data, 'network.dll'))
const child = spawn(join(root, 'LLRemote.EngineHost.exe'), ['-p', '33373'], { cwd: data, env: { ...process.env, LLREMOTE_ENGINE_DIRECTORY: join(root, 'engines', 'Jiaoyuelian'), LLREMOTE_DATA_DIRECTORY: data }, stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true })
let loadError = ''
let nativeDiagnostic = { panic: false, missingNetworkDependency: false }
child.stderr.on('data', data => { const value = data.toString(); const safe = value.split(/\r?\n/).filter(line => line.startsWith('HOST_')); if (safe.length) loadError += safe.join(' '); nativeDiagnostic.panic ||= value.includes('panic:'); nativeDiagnostic.missingNetworkDependency ||= value.includes('network.dll') })
const evidence = { x86Host: false, separatePort: 33373, profileIsolated: true, accountMutation: false, networkConnection: false, originalExeLaunched: false }
try {
  for (let i = 0; i < 50; i++) {
    if (child.exitCode !== null) throw new Error(`Host exited with ${child.exitCode} ${loadError} ${JSON.stringify(nativeDiagnostic)}`)
    try { const response = await fetch('http://127.0.0.1:33373'); if (response.ok) { evidence.x86Host = true; break } } catch {}
    await new Promise(resolve => setTimeout(resolve, 150))
  }
  if (!evidence.x86Host) throw new Error('Isolated native endpoint did not start')
  await writeFile(join(root, '.work', 'nat-interface-script.js'), await fetch('http://127.0.0.1:33373/web/script.js').then(response => response.text()))
  const types = await new Promise((resolve, reject) => {
    const ws = new WebSocket('ws://127.0.0.1:33373/ws', { origin: 'http://127.0.0.1:33373', handshakeTimeout: 5000 })
    const values = []
    const timer = setTimeout(() => { ws.close(); resolve(values) }, 1500)
    ws.on('message', bytes => { const type = bytes.toString().split('<$!$>')[0]; if (!values.includes(type)) values.push(type) })
    ws.on('error', error => { clearTimeout(timer); reject(error) })
  })
  evidence.frameTypes = types
  evidence.dataFiles = await readdir(data)
  await writeFile(join(root, '.work', 'host-evidence.json'), JSON.stringify(evidence, null, 2))
  console.log(JSON.stringify(evidence, null, 2))
} finally { if (child.exitCode === null && child.signalCode === null) { const exited = new Promise(resolve => child.once('exit', resolve)); child.kill(); await exited } }
