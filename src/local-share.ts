export interface LocalShareInfo {
  accessCode: string | null | undefined
  connectionPassword: string
  vpnAddress: string | null | undefined
}

export function formatLocalShare(info: LocalShareInfo): string | null {
  const code = info.accessCode?.trim()
  const address = info.vpnAddress?.trim()
  if (!code || !info.connectionPassword || !address) return null
  return `访问码：${code}\n连接密码：${info.connectionPassword}\n组网地址：${address}`
}

export function localVpnAddressState(authenticated: boolean, networkActive: boolean): string {
  if (!authenticated) return '登录后读取组网地址'
  return networkActive ? '正在读取组网地址' : '组网建立后自动读取'
}
