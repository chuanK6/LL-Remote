import { spawn } from 'node:child_process'
import { mkdirSync } from 'node:fs'
import { resolve, join } from 'node:path'
import { toolchainEnv } from './toolchain.mjs'
const root = resolve(import.meta.dirname, '..')
const build = join(root, '.work', 'engine-host-build')
mkdirSync(build, { recursive: true })
const compiler = spawn('cl.exe', ['/nologo', '/O2', '/W4', '/EHsc', '/MT', '/std:c++17', join(root, 'component-tools', 'EngineHost.cpp'), `/Fe${join(root, 'LLRemote.EngineHost.exe')}`, '/link', '/MACHINE:X86'], { cwd: build, env: toolchainEnv('x86'), stdio: 'inherit', windowsHide: true })
compiler.on('error', error => { console.error(error.message); process.exitCode = 1 })
compiler.on('exit', code => { process.exitCode = code ?? 1 })
