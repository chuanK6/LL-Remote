import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'

export function toolchainEnv(arch = 'x64') {
  const env = { ...process.env }
  env.CARGO_HTTP_MULTIPLEXING = 'false'
  const cargoBin = join(env.USERPROFILE || '', '.cargo', 'bin')
  if (existsSync(cargoBin)) env.PATH = `${cargoBin};${env.PATH || ''}`
  const vswhere = join(env['ProgramFiles(x86)'] || 'C:\\Program Files (x86)', 'Microsoft Visual Studio', 'Installer', 'vswhere.exe')
  if (existsSync(vswhere)) {
    const installation = execFileSync(vswhere, ['-latest', '-products', '*', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath'], { encoding: 'utf8', windowsHide: true }).trim()
    const vcvars = join(installation, 'VC', 'Auxiliary', 'Build', arch === 'x86' ? 'vcvars32.bat' : 'vcvars64.bat')
    if (installation && existsSync(vcvars)) {
      const output = execFileSync('cmd.exe', ['/d', '/s', '/c', `""${vcvars}" >nul && set"`], { encoding: 'utf8', windowsHide: true, windowsVerbatimArguments: true })
      for (const line of output.split(/\r?\n/)) {
        const separator = line.indexOf('=')
        if (separator > 0) env[line.slice(0, separator)] = line.slice(separator + 1)
      }
      const pathKey = Object.keys(env).find(key => key.toLowerCase() === 'path')
      const discoveredPath = pathKey ? env[pathKey] : env.PATH
      if (pathKey && pathKey !== 'PATH') delete env[pathKey]
      env.PATH = `${cargoBin};${discoveredPath || ''}`
    }
  }
  return env
}
