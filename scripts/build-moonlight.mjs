import { spawn } from 'node:child_process'
import { existsSync, mkdirSync, copyFileSync, readdirSync, readFileSync, writeFileSync, createWriteStream } from 'node:fs'
import { resolve, join } from 'node:path'
import { createHash } from 'node:crypto'
import { toolchainEnv } from './toolchain.mjs'

const root = resolve(import.meta.dirname, '..')
const source = join(root, 'third-party', 'sources', 'moonlight')
const build = join(root, '.work', 'moonlight-build')
const qtBin = join(root, '.tools', 'Qt', '6.12.0', 'msvc2022_64', 'bin')
const env = toolchainEnv()
env.PATH = `${qtBin};${env.PATH}`
const log = createWriteStream(join(root, '.work', 'moonlight-build.log'))
function run(command, args, cwd = root) { return new Promise((resolve, reject) => {
  const child = spawn(command, args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true })
  for (const stream of [child.stdout, child.stderr]) stream.on('data', chunk => { log.write(chunk); for (const line of chunk.toString().split(/\r?\n/)) if (/fatal error|error C\d+|error LNK\d+|Project ERROR|Applied LLRemote|finished/i.test(line)) console.log(line.slice(0, 500)) })
  child.on('error', reject); child.on('exit', code => code === 0 ? resolve() : reject(new Error(`${command} exited with ${code}; see .work/moonlight-build.log`)))
}) }
await run('python.exe', ['third-party/moonlight-extension/apply.py'])
mkdirSync(build, { recursive: true })
await run(join(qtBin, 'qmake.exe'), ['-r', join(source, 'moonlight-qt.pro'), 'CONFIG+=release', 'CONFIG-=debug', 'CONFIG+=force_debug_info', 'CONFIG+=no_batch'], build)
const jom = join(root, '.tools', 'jom', 'jom.exe')
await run(existsSync(jom) ? jom : 'nmake.exe', existsSync(jom) ? ['/J4', 'release'] : ['/NOLOGO', 'release'], build)
const target = join(root, 'engines', 'Moonlight')
const executable = join(build, 'app', 'release', 'Moonlight.exe')
if (!existsSync(executable)) throw new Error('Moonlight executable missing after build')
copyFileSync(executable, join(target, 'Moonlight.exe'))
const hook = join(build, 'AntiHooking', 'release', 'AntiHooking.dll')
if (existsSync(hook)) copyFileSync(hook, join(target, 'AntiHooking.dll'))
await run(join(qtBin, 'windeployqt.exe'), ['--release', '--force', '--dir', target, '--qmldir', join(source, 'app', 'gui'), '--no-opengl-sw', '--no-compiler-runtime', '--no-sql', executable], build)
// The GameStream pairing certificate must survive a native process restart.
// Include the OpenSSL TLS backend explicitly: default Windows deployment can
// omit it even though the matching OpenSSL libraries are bundled below.
const tls = join(target, 'tls')
mkdirSync(tls, { recursive: true })
copyFileSync(join(qtBin, '..', 'plugins', 'tls', 'qopensslbackend.dll'), join(tls, 'qopensslbackend.dll'))
// The pinned dependency archive contains the runtime DLLs alongside its import libs.
const libDir = join(source, 'libs', 'windows', 'lib', 'x64')
for (const file of readdirSync(libDir)) if (file.toLowerCase().endsWith('.dll')) copyFileSync(join(libDir, file), join(target, file))
const manifestPath = join(root, 'engines', 'manifest.json')
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
manifest.moonlight.extensionProtocol = 1
manifest.moonlight.sha256 = createHash('sha256').update(readFileSync(join(target, 'Moonlight.exe'))).digest('hex')
writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n')
console.log('Moonlight management extension built and staged. Native verification remains required.')
log.end()
