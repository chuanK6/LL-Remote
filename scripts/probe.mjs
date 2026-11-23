// Read-only M0 probe. Never print a raw native frame: it can contain passwords.
import WebSocket from 'ws'
import { mkdir, writeFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'

const evidence = { recordedAt: new Date().toISOString(), nat: { reachable: false, frameTypes: [], hostCount: 0, authenticated: false, quotaReadable: false }, sunshine: {}, mutations: false }
const html = await fetch('http://127.0.0.1:33272').then(r => r.text()).catch(() => '')
const script = await fetch('http://127.0.0.1:33272/web/script.js').then(r => r.text()).catch(() => '')
evidence.nat.scriptSha256 = createHash('sha256').update(script).digest('hex')
evidence.nat.protocol = script.includes('<$!$>') && script.includes('concode') ? 'v1.06-delimited' : 'unsupported'
evidence.nat.titleVerified = html.includes('皎月连')
await new Promise(resolve => {
  const ws = new WebSocket('ws://127.0.0.1:33272/ws', { origin: 'http://127.0.0.1:33272', handshakeTimeout: 5000, maxPayload: 1024 * 1024 })
  const timer = setTimeout(() => { ws.close(); resolve() }, 6500)
  let asked = false
  ws.on('open', () => { evidence.nat.reachable = true })
  ws.on('message', data => {
    const fields = data.toString().split('<$!$>')
    const type = fields[0]
    if (!evidence.nat.frameTypes.includes(type)) evidence.nat.frameTypes.push(type)
    if (type === '1' || type === '2') {
      evidence.nat.authenticated = true
      evidence.nat.serverRunning = type === '2'
      if (!asked) { asked = true; ws.send('pclist<$!$>') }
    }
    if (type === 'pclist') evidence.nat.hostCount = Math.max(0, Math.floor((fields.length - 7) / 4))
    // No username, code, address, native log, password or PIN is exported.
  })
  ws.on('error', () => { clearTimeout(timer); resolve() })
  ws.on('close', () => { clearTimeout(timer); resolve() })
})
await mkdir('.work', { recursive: true })
await writeFile('.work/m0-evidence.json', JSON.stringify(evidence, null, 2))
console.log(JSON.stringify(evidence, null, 2))
