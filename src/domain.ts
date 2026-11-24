import type { Account, AppError, Device, SessionPatch, SessionSnapshot, StreamPreferences } from './types'

export const statusLabel = (status: string): string => ({ online: '在线', offline: '离线', unknown: '状态未知', pending: '服务未就绪' }[status] || '状态未知')
export const sourceLabel = (source: string): string => ({ account: '同账号设备', code: '识别码历史', existing: '已有连接' }[source] || '已有连接')
export const stageLabel = (stage: string): string => ({ awaitingRoleSwitch: '\u7b49\u5f85\u786e\u8ba4\u81ea\u52a8\u5207\u6362\u89d2\u8272', idle: '尚未连接', preparingNetwork: '正在建立皎月连网络', checkingHost: '正在检查 Sunshine', checkingPairing: '正在检查配对', pairing: '等待首次配对确认', startingStream: '正在启动远程桌面', streaming: '远程桌面已启动', reconnecting: '正在恢复连接', disconnecting: '正在断开本次连接', disconnectFailed: '皎月连断开未确认', failed: '连接未完成', canceled: '已取消连接', disconnected: '本次连接已结束' }[stage] || '正在连接')
export const isConnecting = (stage: string): boolean => ['awaitingRoleSwitch', 'preparingNetwork', 'checkingHost', 'checkingPairing', 'pairing', 'startingStream', 'reconnecting'].includes(stage)
export function hasActiveDeviceSession(deviceId: string, session: SessionSnapshot): boolean {
  return session.deviceId === deviceId && (isConnecting(session.stage) || ['streaming', 'disconnecting', 'disconnectFailed'].includes(session.stage))
}
export function quotaLabel(account: Account): string {
  if (account.quotaState === 'expired') return '服务已到期'
  if (account.quotaMinutes === null) return '暂无法读取'
  if (account.quotaMinutes === 0) return '可用时长不足'
  return `${Math.floor(account.quotaMinutes / 60)} 小时 ${Math.floor(account.quotaMinutes % 60)} 分钟`
}
export function formatTime(value: string | null): string {
  if (!value) return '—'
  const date = new Date(value)
  if (!Number.isFinite(date.getTime())) return '—'
  return new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short', timeZone: 'Asia/Shanghai', hour12: false }).format(date)
}
export function deviceName(device: Device): string { return device.displayName || device.hostName || '未命名设备' }
export function retainDeviceOrder(previous: readonly Device[], incoming: readonly Device[]): Device[] {
  const remaining = new Map(incoming.map(device => [device.id, device]))
  const ordered: Device[] = []
  for (const { id } of previous) {
    const device = remaining.get(id)
    if (device) { ordered.push(device); remaining.delete(id) }
  }
  return [...ordered, ...remaining.values()]
}
export function normalizeError(error: unknown): AppError {
  if (error && typeof error === 'object' && 'code' in error && 'message' in error) return error as AppError
  // Do not display an exception string that might contain request parameters.
  return { code: 'SYSTEM_FAILED', message: '操作未完成，请在连接诊断中检查组件状态。', retryable: false, reauthenticate: false }
}
export function validateStream(value: StreamPreferences): string | null {
  if (!Number.isInteger(value.width) || !Number.isInteger(value.height) || value.width < 320 || value.width > 7680 || value.height < 200 || value.height > 4320 || value.width % 2 || value.height % 2) return '分辨率须为有效偶数，范围为 320–7680 × 200–4320。'
  if (!Number.isInteger(value.fps) || value.fps < 10 || value.fps > 360) return '帧率须为 10–360 的整数。'
  if (!Number.isInteger(value.bitrateKbps) || value.bitrateKbps < 1000 || value.bitrateKbps > 500000) return '码率须为 1–500 Mbps。'
  return null
}
export function patchStream(current: StreamPreferences, patch: SessionPatch): StreamPreferences {
  const next = { ...current }
  switch (patch.kind) {
    case 'resolution': next.width = patch.width; next.height = patch.height; break
    case 'bitrate': next.bitrateKbps = patch.bitrateKbps; break
    case 'fps': next.fps = patch.fps; break
    case 'mouse': next.mouseMode = patch.mode; break
    case 'window': next.windowMode = patch.mode; break
    case 'audio': next.volume = patch.volume; next.muted = patch.muted; next.audioDevice = patch.device; next.hostAudio = patch.hostAudio; break
  }
  return next
}
