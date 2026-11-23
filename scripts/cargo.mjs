import { spawn } from 'node:child_process'
import { toolchainEnv } from './toolchain.mjs'
const child = spawn('cargo.exe', process.argv.slice(2), { env: toolchainEnv(), stdio: 'inherit', windowsHide: true })
child.on('error', error => { console.error(error.message); process.exitCode = 1 })
child.on('exit', code => { process.exitCode = code ?? 1 })
