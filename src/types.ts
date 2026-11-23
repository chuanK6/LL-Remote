import { version } from '../package.json'

export interface AppError { code: string; message: string; retryable: boolean; reauthenticate: boolean }
export type MouseMode = 'game' | 'office'
export type WindowMode = 'fullscreen' | 'windowed' | 'borderless'
export interface StreamPreferences {
  width: number; height: number; fps: number; bitrateKbps: number
  codec: 'auto' | 'h264' | 'hevc' | 'av1'; windowMode: WindowMode; mouseMode: MouseMode
  audioConfig: 'stereo' | '5.1' | '7.1'; hostAudio: boolean; volume: number; muted: boolean
  audioDevice: string; captureKeys: 'fullscreen' | 'always' | 'never'; gamepad: boolean
}
export interface GeneralPreferences {
  preventSleep: boolean; keepDisplayAwake: boolean; autostart: boolean; keepRunningInTrayOnClose: boolean
  rememberLogin: boolean; autoRetry: boolean; alwaysAcceptControl: boolean; showFloatBall: boolean; reuseExisting: boolean
}
export interface Preferences extends GeneralPreferences {
  acceptControl: boolean
  setupComplete: boolean; onboardingComplete: boolean; firstConnectionComplete: boolean; sunshineUsername: string | null; moonlightImported: boolean; stream: StreamPreferences
}
export interface Account {
  username: string | null; authenticated: boolean; quotaMinutes: number | null
  quotaState: string; updatedAt: string | null; localCode: string | null; localName: string | null
  localVpnAddress: string | null
  serverRunning: boolean; protocolSupported: boolean
}
export interface PublishedApp { id: string; name: string }
export interface Device {
  id: string; hostName: string; displayName: string; source: string; natId: string | null
  uuid: string | null; address: string | null; peerCode: string | null
  status: string; networkConnected: boolean; serviceReady: boolean | null; observedAt: string | null; lastConnectedAt: string | null
  favorite: boolean; note: string; wallpaper: string | null; credentialRef: string | null
  credentialState: string; preferences: StreamPreferences; apps: PublishedApp[]; quickApps: string[]
}
export interface HistoryEntry {
  id: string; deviceId: string | null; peerCode: string | null; connectedAt: string
  method: string; result: string; errorCode: string | null
}
export interface ComponentStatus { name: string; available: boolean; running: boolean; ready: boolean; owned: boolean; version: string; detail: string }
export interface SunshineSettings { outputName: string; audioSink: string; adapterName: string }
export interface SunshineStatus {
  running: boolean; ready: boolean; accountInitialized: boolean; accountUsername: string | null; credentialValid: boolean
  external: boolean; port: number; locale: string; apps: PublishedApp[]
  pendingPairings: { id: string; name: string }[]; config: SunshineSettings
}
export interface Metrics {
  fps: number | null; bitrateMbps: number | null; networkMs: number | null
  decodeMs: number | null; renderMs: number | null; droppedPercent: number | null; updatedAt: string | null
}
export interface SessionSnapshot {
  id: string | null; deviceId: string | null; deviceName: string | null; stage: string; attempt: number
  app: string | null; pin: string | null; error: AppError | null; preferences: StreamPreferences | null
  startedAt: string | null; metrics: Metrics; extensionAvailable: boolean
}
export interface IncomingControl {
  count: number
  startedAt: string | null
  deviceName: string | null
}
export interface AppSnapshot {
  version: string; portable: boolean; dataDirectory: string; preferences: Preferences; account: Account
  devices: Device[]; history: HistoryEntry[]; sunshine: SunshineStatus; session: SessionSnapshot
  components: ComponentStatus[]; powerStatus: string; storageWarning: string | null
}
export interface DeviceEdit { deviceId: string; displayName: string; favorite: boolean; note: string; wallpaperPath: string | null; resetWallpaper: boolean }
export interface ConnectRequest { deviceId?: string; peerCode?: string; password?: string; app?: string; address?: string }
export type SessionPatch =
  | { kind: 'resolution'; width: number; height: number }
  | { kind: 'bitrate'; bitrateKbps: number }
  | { kind: 'fps'; fps: number }
  | { kind: 'mouse'; mode: MouseMode }
  | { kind: 'window'; mode: WindowMode }
  | { kind: 'audio'; volume: number; muted: boolean; device: string; hostAudio: boolean }
export interface Diagnostic { name: string; status: string; detail: string }

export const defaultStream = (): StreamPreferences => ({ width: 1920, height: 1080, fps: 60, bitrateKbps: 15000, codec: 'auto', windowMode: 'windowed', mouseMode: 'office', audioConfig: 'stereo', hostAudio: false, volume: 75, muted: false, audioDevice: '', captureKeys: 'fullscreen', gamepad: true })
export const defaultPreferences = (): Preferences => ({ preventSleep: true, keepDisplayAwake: false, autostart: false, keepRunningInTrayOnClose: false, rememberLogin: true, autoRetry: true, acceptControl: false, alwaysAcceptControl: false, showFloatBall: true, setupComplete: false, onboardingComplete: false, firstConnectionComplete: false, sunshineUsername: null, reuseExisting: true, moonlightImported: false, stream: defaultStream() })
export const emptyMetrics = (): Metrics => ({ fps: null, bitrateMbps: null, networkMs: null, decodeMs: null, renderMs: null, droppedPercent: null, updatedAt: null })
export const emptySession = (): SessionSnapshot => ({ id: null, deviceId: null, deviceName: null, stage: 'idle', attempt: 0, app: null, pin: null, error: null, preferences: null, startedAt: null, metrics: emptyMetrics(), extensionAvailable: false })
export const emptySnapshot = (): AppSnapshot => ({ version, portable: false, dataDirectory: '', preferences: defaultPreferences(), account: { username: null, authenticated: false, quotaMinutes: null, quotaState: 'unknown', updatedAt: null, localCode: null, localName: null, localVpnAddress: null, serverRunning: false, protocolSupported: false }, devices: [], history: [], sunshine: { running: false, ready: false, accountInitialized: false, accountUsername: null, credentialValid: false, external: false, port: 47990, locale: 'zh', apps: [], pendingPairings: [], config: { outputName: '', audioSink: '', adapterName: '' } }, session: emptySession(), components: [], powerStatus: '正在检查', storageWarning: null })
