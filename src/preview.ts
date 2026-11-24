// Browser-only layout preview. It never logs in, stores a password or starts a session.
import { defaultStream, emptySnapshot, type AppSnapshot, type Device } from './types'

export function previewSnapshot(): AppSnapshot {
  const snapshot = emptySnapshot()
  if (new URLSearchParams(location.search).get('view') === 'setup') return snapshot
  snapshot.preferences.setupComplete = true
  snapshot.preferences.onboardingComplete = true
  snapshot.preferences.sunshineUsername = 'sunshine'
  snapshot.account = { ...snapshot.account, username: '皎月连账号', authenticated: true, localName: '本机', localCode: null }
  const device = (id: string, name: string, source: string, status: string): Device => ({ id, displayName: name, hostName: name, source, natId: source === 'account' ? id : null, uuid: id, address: null, peerCode: source === 'code' ? '45881236' : null, status, networkConnected: false, serviceReady: status === 'online' ? true : null, observedAt: null, lastConnectedAt: null, favorite: false, note: '', wallpaper: null, credentialRef: null, credentialState: 'missing', preferences: defaultStream(), apps: [{ id: '1', name: 'Desktop' }, { id: '2', name: 'Steam Big Picture' }], quickApps: ['Desktop', 'Steam Big Picture'] })
  snapshot.devices = [device('preview-home', '大娃家', 'account', 'online'), device('preview-history', '朋友的电脑', 'code', 'unknown'), device('preview-existing', 'DESKTOP-E4NTI0J', 'existing', 'unknown')]
  snapshot.devices[0].favorite = true
  snapshot.devices[0].accountOnline = true
  snapshot.devices[1].lastConnectedAt = '2026-11-24T04:20:00Z'
  snapshot.devices[2].lastConnectedAt = '2026-11-23T08:10:00Z'
  snapshot.history = [
    { id: 'preview-record-1', deviceId: 'preview-history', peerCode: '45881236', connectedAt: '2026-11-24T04:20:00Z', method: '识别码', result: 'success', errorCode: null },
    { id: 'preview-record-2', deviceId: 'preview-existing', peerCode: null, connectedAt: '2026-11-23T08:10:00Z', method: '已有连接', result: 'success', errorCode: null },
  ]
  snapshot.powerStatus = '网页预览'
  return snapshot
}
