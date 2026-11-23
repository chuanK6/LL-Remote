import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import { bridge } from '../../src/bridge'
import { previewSnapshot } from '../../src/preview'
import { useAppStore } from '../../src/stores/app'
import { emptySession, type AppSnapshot, type SessionSnapshot } from '../../src/types'
import DevicePage from '../../src/pages/DevicePage.vue'
import DevicesPage from '../../src/pages/DevicesPage.vue'
import ConnectionBanner from '../../src/components/ConnectionBanner.vue'

vi.mock('../../src/bridge', () => ({
  bridge: {
    connect: vi.fn(),
    disconnectDevice: vi.fn(),
    associateHost: vi.fn(),
  },
}))

let wrapper: VueWrapper | undefined

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

function connectedSnapshot() {
  const snapshot = previewSnapshot()
  snapshot.devices[0].networkConnected = true
  snapshot.session = {
    ...emptySession(),
    id: 'session-home',
    deviceId: snapshot.devices[0].id,
    deviceName: '大娃家',
    stage: 'pairing',
    pin: '3141',
  }
  return snapshot
}

function disconnectedSnapshot(snapshot: AppSnapshot) {
  const next = structuredClone(snapshot)
  next.devices[0].networkConnected = false
  next.session = { ...emptySession(), stage: 'disconnected' }
  return next
}

async function renderPage(path: string, snapshot: AppSnapshot) {
  const pinia = createPinia()
  setActivePinia(pinia)
  const app = useAppStore()
  app.apply(snapshot)
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/device/:id', component: DevicePage },
      { path: '/devices', name: 'devices', component: DevicesPage },
      { path: '/pairing', component: { template: '<div>配对</div>' } },
    ],
  })
  await router.push(path)
  await router.isReady()
  wrapper = mount(path === '/devices' ? DevicesPage : DevicePage, {
    global: { plugins: [pinia, router], stubs: { DeviceWallpaper: true, DeviceEditor: true } },
  })
  return { app, page: wrapper }
}

describe('设备网络连接状态与断开操作', () => {
  it('等待首次配对时显示网络已连接，并等待后端断开结果后更新页面', async () => {
    const snapshot = connectedSnapshot()
    const { app, page } = await renderPage('/device/preview-home', snapshot)
    let finish!: (value: AppSnapshot) => void
    vi.mocked(bridge.disconnectDevice).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))

    expect(page.get('[aria-label="设备连接状态"]').text()).toContain('皎月连已连接')
    expect(page.get('[aria-label="设备连接状态"]').text()).toContain('等待首次配对确认')
    const button = page.findAll('button').find(candidate => candidate.text() === '断开连接')!
    await button.trigger('click')

    expect(bridge.disconnectDevice).toHaveBeenCalledWith('preview-home')
    expect(button.attributes('disabled')).toBeDefined()
    expect(app.snapshot.devices[0].networkConnected).toBe(true)
    finish(disconnectedSnapshot(snapshot))
    await flushPromises()

    expect(page.find('[aria-label="设备连接状态"]').exists()).toBe(false)
    expect(app.busy).toBe(false)
    expect(app.disconnectingDeviceId).toBeNull()
  })

  it('列表只给实际连接的设备显示断开入口', async () => {
    const snapshot = connectedSnapshot()
    const { page } = await renderPage('/devices', snapshot)
    const rows = page.findAll('tbody tr')

    expect(rows[0].text()).toContain('皎月连已连接')
    expect(rows[0].find('[aria-label="断开 大娃家 的连接"]').exists()).toBe(true)
    expect(rows[1].text()).toContain('状态未知')
    expect(rows[1].text()).not.toContain('皎月连已连接')
    expect(rows[1].text()).not.toContain('断开连接')
  })

  it('断开失败保留真实连接状态和可重试入口', async () => {
    const snapshot = connectedSnapshot()
    const { app, page } = await renderPage('/device/preview-home', snapshot)
    vi.mocked(bridge.disconnectDevice).mockRejectedValueOnce({
      code: 'NETWORK_FAILED', message: '断开未完成，请重试。', retryable: true, reauthenticate: false,
    })
    const button = page.findAll('button').find(candidate => candidate.text() === '断开连接')!
    await button.trigger('click')
    await flushPromises()

    expect(app.error?.code).toBe('NETWORK_FAILED')
    expect(app.snapshot.devices[0].networkConnected).toBe(true)
    expect(button.attributes('disabled')).toBeUndefined()
    expect(app.disconnectingDeviceId).toBeNull()
  })

  it('切到其他设备时不会把当前会话状态显示为该设备的连接', async () => {
    const { page } = await renderPage('/device/preview-history', connectedSnapshot())

    expect(page.find('[aria-label="设备连接状态"]').exists()).toBe(false)
    expect(page.get('.enter').text()).toBe('进入桌面')
    expect(page.get('.cover').attributes('disabled')).toBeDefined()
  })

  it('切换页面时连接横幅持续显示本次 PIN，配对结束后隐藏', async () => {
    const pinia = createPinia()
    setActivePinia(pinia)
    const app = useAppStore()
    app.apply(connectedSnapshot())
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/devices', component: { template: '<div />' } },
        { path: '/device/:id', component: { template: '<div />' } },
      ],
    })
    await router.push('/devices')
    wrapper = mount(ConnectionBanner, { global: { plugins: [pinia, router] } })
    expect(wrapper.get('.banner-pin').text()).toBe('配对 PIN 3141')

    await router.push('/device/preview-history')
    expect(wrapper.get('.banner-pin').text()).toBe('配对 PIN 3141')
    app.snapshot.session.stage = 'checkingPairing'
    await flushPromises()
    expect(wrapper.find('.banner-pin').exists()).toBe(false)
  })

  it('较晚返回的首次响应保留同一会话已经收到的配对状态', async () => {
    setActivePinia(createPinia())
    const app = useAppStore()
    const snapshot = connectedSnapshot()
    snapshot.session = emptySession()
    app.apply(snapshot)
    let finish!: (value: SessionSnapshot) => void
    vi.mocked(bridge.connect).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))

    const connecting = app.connect({ deviceId: 'preview-home' })
    app.snapshot.session = { ...emptySession(), id: 'progress-session', deviceId: 'preview-home', stage: 'pairing', pin: '3141' }
    finish({ ...emptySession(), id: 'progress-session', deviceId: 'preview-home', stage: 'preparingNetwork' })
    await connecting

    expect(app.snapshot.session.stage).toBe('pairing')
    expect(app.snapshot.session.pin).toBe('3141')
  })

  it('首次响应前的重复点击只建立一次连接，迟到响应不会覆盖断开状态', async () => {
    const pinia = createPinia()
    setActivePinia(pinia)
    const app = useAppStore()
    const snapshot = connectedSnapshot()
    snapshot.session = emptySession()
    app.apply(snapshot)
    let finish!: (value: SessionSnapshot) => void
    vi.mocked(bridge.connect).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    vi.mocked(bridge.disconnectDevice).mockResolvedValueOnce(disconnectedSnapshot(snapshot))

    const first = app.connect({ deviceId: 'preview-home' })
    await app.connect({ deviceId: 'preview-home' })
    expect(bridge.connect).toHaveBeenCalledTimes(1)
    await app.disconnectDevice('preview-home')
    finish({ ...emptySession(), id: 'late-session', deviceId: 'preview-home', stage: 'checkingPairing' })
    await first

    expect(app.snapshot.session.stage).toBe('disconnected')
    expect(app.snapshot.devices[0].networkConnected).toBe(false)
    expect(app.busy).toBe(false)
  })
})
