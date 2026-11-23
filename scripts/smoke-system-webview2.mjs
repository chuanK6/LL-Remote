// Launch a copy of the package with fresh Data; never load the user's login data.
import { cp, mkdir, readdir, readFile, writeFile } from 'node:fs/promises'
import { resolve, join } from 'node:path'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { randomUUID } from 'node:crypto'

const workspace = resolve(import.meta.dirname, '..')
if (!process.argv[2]) throw new Error('Usage: node scripts/smoke-system-webview2.mjs <package directory>')
const source = resolve(process.argv[2])
const fixture = join(workspace, '.work', `system WebView2 smoke-${randomUUID()}`)
await mkdir(fixture, { recursive: true })
for (const entry of await readdir(source, { withFileTypes: true })) {
  if (entry.isFile()) await cp(join(source, entry.name), join(fixture, entry.name))
}
for (const directory of ['engines', 'notices', 'component-tools']) await cp(join(source, directory), join(fixture, directory), { recursive: true })
await mkdir(join(fixture, 'Data'), { recursive: true })
await writeFile(join(fixture, 'Data', 'preferences.json'), JSON.stringify({ rememberLogin: false, reuseExisting: false, preventSleep: false, acceptControl: false }))
const run = promisify(execFile)
const { stdout: existing } = await run('powershell.exe', ['-NoProfile', '-Command', "@(Get-CimInstance Win32_Process -Filter \"Name='LLRemote.exe'\").Count"], { windowsHide: true })
const isolated = Number(existing.trim()) > 0
if (isolated) {
  const executable = join(fixture, 'LLRemote.exe')
  const data = await readFile(executable)
  const original = Buffer.from('cn.llremote.desktop')
  const identifier = Buffer.from(`cn.llremote.t${randomUUID().replaceAll('-', '').slice(0, 6)}`)
  let offset = 0, count = 0
  while ((offset = data.indexOf(original, offset)) !== -1) { identifier.copy(data, offset); offset += original.length; count++ }
  if (count === 0) throw new Error('Unable to isolate the test copy from the running application')
  await writeFile(executable, data)
}
await writeFile(join(workspace, '.work', 'system-webview2-fixture-path.txt'), fixture)
const { stdout, stderr } = await run(process.execPath, [join(workspace, 'scripts', 'smoke-packaged.mjs'), join(fixture, 'LLRemote.exe'), ...(isolated ? ['--isolated-instance'] : [])], { cwd: workspace, windowsHide: true, timeout: 90000, maxBuffer: 1024 * 1024 })
const evidence = JSON.parse(await readFile(join(workspace, '.work', 'portable-evidence.json'), 'utf8'))
evidence.testCopyIsolated = true
evidence.onlyInstanceIdentifierPatched = isolated
evidence.actualUserDataLoaded = false
await writeFile(join(workspace, '.work', 'system-webview2-startup.json'), JSON.stringify(evidence, null, 2) + '\n')
console.log(stdout.trim())
if (stderr.trim()) console.error(stderr.trim())
