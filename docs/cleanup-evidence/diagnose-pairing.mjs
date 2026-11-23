import { spawn, execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { readFile, writeFile } from 'node:fs/promises'
import https from 'node:https'
import { resolve, join } from 'node:path'
const workspace = resolve(import.meta.dirname, '..')
const fixture = join(workspace, '.work', 'pairing-fixture-dGJDsG')
const config = join(fixture, 'engines', 'Sunshine', 'config', 'sunshine.conf')
const base = Number(/^port\s*=\s*(\d+)/m.exec(await readFile(config, 'utf8'))[1])
const child = spawn(join(workspace, 'engines', 'Sunshine', 'sunshine.exe'), [config], { cwd: join(workspace, 'engines', 'Sunshine'), stdio: 'ignore', windowsHide: true })
try {
  for (let i = 0; i < 80; i++) {
    if (child.exitCode !== null) throw new Error('Fixture Sunshine exited')
    const ready = await new Promise(resolve => https.get(`https://127.0.0.1:${base + 1}/api/configLocale`, { rejectUnauthorized: false }, response => { response.resume(); resolve(response.statusCode === 200) }).on('error', () => resolve(false)))
    if (ready) break
    await new Promise(resolve => setTimeout(resolve, 150))
  }
  const serverInfo = await fetch(`http://127.0.0.1:${base}/serverinfo`).then(response => response.text())
  console.log(JSON.stringify({base, httpsPort:Number(/<HttpsPort>(\d+)/.exec(serverInfo)?.[1]), httpAvailable:serverInfo.includes('<root'), pairState: /<PairStatus>(\d+)/.exec(serverInfo)?.[1]}))
  const native = await promisify(execFile)(join(workspace, 'engines', 'Moonlight', 'Moonlight.exe'), ['list', `127.0.0.1:${base}`], { cwd: join(workspace, 'engines', 'Moonlight'), env: { ...process.env, LLREMOTE_MOONLIGHT_DATA_DIR: join(fixture, 'Moonlight') }, windowsHide: true, timeout: 35000 }).catch(error => error)
  const stderr = (native.stderr || '').replace(/\?[^\s]+/g, '?<redacted>')
  await writeFile(join(workspace, '.work', 'pairing-native-error.txt'), stderr)
  console.log(JSON.stringify({code:native.code ?? 0, stderr, stdout:native.stdout || ''}))
} finally {
  if (child.exitCode === null && child.signalCode === null) { const exited = new Promise(resolve => child.once('exit', resolve)); child.kill(); await exited }
}
