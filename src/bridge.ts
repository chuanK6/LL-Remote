import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { AppError, AppSnapshot, ConnectRequest, DeviceEdit, Diagnostic, GeneralPreferences, IncomingControl, SessionPatch, SessionSnapshot, StreamPreferences, SunshineSettings } from './types'
import { previewSnapshot } from './preview'

export const native = isTauri()
const desktopRequired = { code: 'DESKTOP_REQUIRED', message: '此操作需要在 Windows 桌面版 LL远程中完成。', retryable: false, reauthenticate: false }
const browserSnapshot = previewSnapshot()
const call = <T>(command: string, args?: Record<string, unknown>): Promise<T> => native ? invoke<T>(command, args) : Promise.reject(desktopRequired)

export const bridge = {
  snapshot: (): Promise<AppSnapshot> => native ? call('bootstrap') : Promise.resolve(structuredClone(browserSnapshot)),
  refresh: (): Promise<AppSnapshot> => native ? call('refresh_state') : Promise.resolve(structuredClone(browserSnapshot)),
  login: (username: string, password: string, remember: boolean) => call<AppSnapshot>('login_jiaoyue', { username, password, remember }),
  authorize: (password: string) => call<AppSnapshot>('authorize_jiaoyue', { password }),
  logout: () => call<AppSnapshot>('logout_jiaoyue'),
  sunshineAccount: (username: string, password: string, confirmPassword: string, mode: string) => call<AppSnapshot>('setup_sunshine', { username, password, confirmPassword, mode }),
  sunshineControl: (action: string) => call<AppSnapshot>('control_sunshine', { action }),
  acceptControl: (enabled: boolean, password: string, webPassword = '') => call<AppSnapshot>('set_accept_control', { enabled, password, webPassword }),
  localServerPasswords: () => call<{ connectionPassword: string; webPassword: string }>('get_local_server_passwords'),
  sunshinePair: (pin: string, name: string, pairingId: string) => call<AppSnapshot>('confirm_sunshine_pin', { pin, name, pairingId }),
  sunshineSettings: (settings: SunshineSettings, restart: boolean) => call<AppSnapshot>('save_sunshine_settings', { settings, restart }),
  savePreferences: async (preferences: Partial<GeneralPreferences>): Promise<AppSnapshot> => {
    if (native) return call('save_preferences', { preferences })
    // Only nonsystem preferences are mutable in a labeled browser preview.
    Object.assign(browserSnapshot.preferences, preferences, { autostart: false, alwaysAcceptControl: false })
    return structuredClone(browserSnapshot)
  },
  editDevice: async (edit: DeviceEdit): Promise<AppSnapshot> => {
    if (native) return call('edit_device', { edit })
    const device = browserSnapshot.devices.find(d => d.id === edit.deviceId)
    if (device) { device.displayName = edit.displayName; device.favorite = edit.favorite; device.note = edit.note; if (edit.resetWallpaper) device.wallpaper = null }
    return structuredClone(browserSnapshot)
  },
  deleteDevice: async (deviceId: string): Promise<AppSnapshot> => {
    if (native) return call('delete_device', { deviceId })
    browserSnapshot.devices = browserSnapshot.devices.filter(device => device.id !== deviceId)
    return structuredClone(browserSnapshot)
  },
  saveStream: async (deviceId: string | null, preferences: StreamPreferences): Promise<AppSnapshot> => {
    if (native) return call('save_stream_preferences', { deviceId, preferences })
    const device = browserSnapshot.devices.find(candidate => candidate.id === deviceId)
    if (device) device.preferences = { ...preferences }
    else if (!deviceId) browserSnapshot.preferences.stream = { ...preferences }
    return structuredClone(browserSnapshot)
  },
  saveQuickApps: (deviceId: string, apps: string[]) => call<AppSnapshot>('save_quick_apps', { deviceId, apps }),
  associateHost: (deviceId: string, natId: string) => call<AppSnapshot>('associate_host', { deviceId, natId }),
  connect: (request: ConnectRequest) => call<SessionSnapshot>('connect_device', { request }),
  cancel: () => call<SessionSnapshot>('cancel_connection'),
  disconnect: (sessionId: string) => call<SessionSnapshot>('disconnect_session', { sessionId }),
  disconnectDevice: (deviceId: string) => call<AppSnapshot>('disconnect_device', { deviceId }),
  updateSession: (sessionId: string, patch: SessionPatch) => call<SessionSnapshot>('update_session', { sessionId, patch }),
  session: () => call<SessionSnapshot>('get_session'),
  showRemoteWindow: () => call<void>('show_remote_window'),
  incomingControl: () => call<IncomingControl>('get_incoming_control'),
  disconnectIncomingControl: () => call<void>('disconnect_incoming_control'),
  showSession: () => call<void>('show_session_controls'),
  diagnostics: (deviceId: string | null) => call<Diagnostic[]>('run_diagnostics', { deviceId }),
  refreshApps: (deviceId: string) => call<AppSnapshot>('refresh_device_apps', { deviceId }),
  openAdvanced: (app: string, deviceId: string | null = null) => call<string | null>('open_advanced', { app, deviceId }),
  openSignin: async () => { if (native) await call('open_signin'); else window.open('https://www.natpierce.cn/pc/sign/index.html', '_blank', 'noopener,noreferrer') },
  wallpaper: async (relative: string): Promise<string> => native ? convertFileSrc(await call<string>('wallpaper_path', { relative })) : relative,
  previewWallpaper: (path: string) => call<string>('preview_wallpaper', { path }),
  quit: (stopAccepting: boolean) => call<void>('quit_app', { stopAccepting }),
  listenSnapshot: (handler: (snapshot: AppSnapshot) => void): Promise<UnlistenFn> => native ? listen<AppSnapshot>('ll:snapshot', event => handler(event.payload)) : Promise.resolve(() => {}),
  listenSession: (handler: (session: SessionSnapshot) => void): Promise<UnlistenFn> => native ? listen<SessionSnapshot>('ll:session', event => handler(event.payload)) : Promise.resolve(() => {}),
  listenIncomingControl: (handler: (control: IncomingControl) => void): Promise<UnlistenFn> => listen<IncomingControl>('ll:incoming-control', event => handler(event.payload)),
  listenOperationError: (handler: (error: AppError) => void): Promise<UnlistenFn> => native ? listen<AppError>('ll:operation-error', event => handler(event.payload)) : Promise.resolve(() => {}),
}
