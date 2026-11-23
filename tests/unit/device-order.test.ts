import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import App from '../../src/App.vue'
import { bridge } from '../../src/bridge'
import { previewSnapshot } from '../../src/preview'
import { useAppStore } from '../../src/stores/app'
import type { AppSnapshot } from '../../src/types'

vi.mock('../../src/bridge', () => ({
  native: false,
  bridge: {
    snapshot: vi.fn(), refresh: vi.fn(),
    listenSnapshot: vi.fn(), listenSession: vi.fn(async () => () => {}),
  },
}))

let wrapper: VueWrapper
let app: ReturnType<typeof useAppStore>
let pushSnapshot: (snapshot: AppSnapshot) => void
const originalClose = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'close')

function initialSnapshot() {
  const snapshot = previewSnapshot()
  for (const device of snapshot.devices) device.favorite = false
  return snapshot
}
function navigationNames() { return wrapper.findAll('.device-nav-name').map(device => device.text()) }

beforeAll(() => {
  Object.defineProperty(HTMLDialogElement.prototype, 'close', { configurable: true, value() {} })
})
afterAll(() => {
  if (originalClose) Object.defineProperty(HTMLDialogElement.prototype, 'close', originalClose)
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'close')
})
beforeEach(async () => {
  vi.resetAllMocks()
  vi.mocked(bridge.snapshot).mockResolvedValue(initialSnapshot())
  vi.mocked(bridge.listenSnapshot).mockImplementation(async handler => { pushSnapshot = handler; return () => {} })
  vi.mocked(bridge.listenSession).mockResolvedValue(() => {})
  const pinia = createPinia()
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/device/:id', name: 'device', component: { template: '<div />' } }],
  })
  await router.push('/device/preview-history')
  wrapper = mount(App, {
    global: { plugins: [pinia, router], stubs: { ConnectionBanner: true, SessionPanel: true } },
  })
  app = useAppStore(pinia)
  await flushPromises()
})
afterEach(() => wrapper.unmount())

describe('设备导航刷新保持位置', () => {
  it('重复倒序回包不移动设备，同时刷新在线状态并保留当前选择', async () => {
    app.selectedDeviceId = 'preview-history'
    const names = navigationNames()
    for (let refresh = 0; refresh < 2; refresh++) {
      const next = initialSnapshot()
      next.devices.reverse()
      next.account.updatedAt = `2026-11-22T13:00:0${refresh}Z`
      for (const device of next.devices) {
        device.status = device.id === 'preview-history' ? 'online' : 'offline'
        device.observedAt = next.account.updatedAt
        device.lastConnectedAt = next.account.updatedAt
      }
      vi.mocked(bridge.refresh).mockResolvedValueOnce(next)
      await app.refreshBackground()
      await flushPromises()

      expect(navigationNames()).toEqual(names)
      expect(wrapper.findAll('.device-button')[0].classes()).not.toContain('online')
      expect(wrapper.findAll('.device-button')[1].classes()).toContain('online')
      expect(app.selectedDeviceId).toBe('preview-history')
      expect(app.selectedDevice?.observedAt).toBe(next.account.updatedAt)
      expect(next.devices[0].id).toBe('preview-existing')
    }
  })

  it('推送的新设备追加到已有列表后，删除设备也不会重排剩余设备', async () => {
    const next = initialSnapshot()
    next.devices.reverse()
    next.devices.unshift({ ...next.devices[0], id: 'new-device', displayName: '新设备' })
    pushSnapshot(next)
    await flushPromises()
    expect(navigationNames()).toEqual(['大娃家', '朋友的电脑', 'DESKTOP-E4NTI0J', '新设备'])

    next.devices = next.devices.filter(device => device.id !== 'preview-history').reverse()
    pushSnapshot(next)
    await flushPromises()
    expect(navigationNames()).toEqual(['大娃家', 'DESKTOP-E4NTI0J', '新设备'])
    expect(app.snapshot.devices.some(device => device.id === 'preview-history')).toBe(false)
  })

  it('收藏操作仍把设备放到收藏组，后续回包保持组内顺序', async () => {
    const next = initialSnapshot()
    next.devices[2].favorite = true
    app.apply(next)
    await flushPromises()
    expect(navigationNames()).toEqual(['DESKTOP-E4NTI0J', '大娃家', '朋友的电脑'])

    next.devices.reverse()
    pushSnapshot(next)
    await flushPromises()
    expect(navigationNames()).toEqual(['DESKTOP-E4NTI0J', '大娃家', '朋友的电脑'])
  })

  it('切换账号重新采用该账号设备顺序，不沿用上个账号的位置', async () => {
    const next = initialSnapshot()
    next.account.username = '另一个账号'
    next.devices.reverse()
    pushSnapshot(next)
    await flushPromises()
    expect(navigationNames()).toEqual(['DESKTOP-E4NTI0J', '朋友的电脑', '大娃家'])
  })
})
