import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, test, vi } from 'vitest'
import { createPinia } from 'pinia'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import App from '../../src/App.vue'
import { bridge } from '../../src/bridge'
import { useAppStore } from '../../src/stores/app'
import { emptySnapshot, type AppSnapshot } from '../../src/types'

vi.mock('../../src/bridge', () => ({
  native: false,
  bridge: {
    snapshot: vi.fn(),
    listenSnapshot: vi.fn(async () => () => {}),
    listenSession: vi.fn(async () => () => {}),
    sunshinePair: vi.fn(),
  },
}))

const firstId = 'a'.repeat(32)
const secondId = 'b'.repeat(32)
const thirdId = 'c'.repeat(32)
let wrapper: VueWrapper
let router: Router
let app: ReturnType<typeof useAppStore>
const originalShowModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'showModal')
const originalClose = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'close')
const originalScrollTo = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollTo')

function snapshotWithPairings(...pairings: { id: string; name: string }[]): AppSnapshot {
  const snapshot = emptySnapshot()
  snapshot.sunshine = {
    ...snapshot.sunshine,
    running: true,
    ready: true,
    accountInitialized: true,
    credentialValid: true,
    pendingPairings: pairings,
  }
  return snapshot
}

beforeAll(() => {
  Object.defineProperty(Element.prototype, 'scrollTo', { configurable: true, value() {} })
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true,
    value(this: HTMLDialogElement) { this.setAttribute('open', '') },
  })
  Object.defineProperty(HTMLDialogElement.prototype, 'close', {
    configurable: true,
    value(this: HTMLDialogElement) {
      this.removeAttribute('open')
      this.dispatchEvent(new Event('close'))
    },
  })
})

afterAll(() => {
  if (originalShowModal) Object.defineProperty(HTMLDialogElement.prototype, 'showModal', originalShowModal)
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'showModal')
  if (originalClose) Object.defineProperty(HTMLDialogElement.prototype, 'close', originalClose)
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'close')
  if (originalScrollTo) Object.defineProperty(Element.prototype, 'scrollTo', originalScrollTo)
  else Reflect.deleteProperty(Element.prototype, 'scrollTo')
})

beforeEach(async () => {
  vi.clearAllMocks()
  vi.mocked(bridge.snapshot).mockResolvedValue(emptySnapshot())
  const pinia = createPinia()
  router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/app/moonlight', component: { template: '<h2>Moonlight</h2>' } },
      { path: '/setup', component: { template: '<h2>设置 Sunshine 账号</h2>' } },
    ],
  })
  await router.push('/app/moonlight')
  await router.isReady()
  wrapper = mount(App, { attachTo: document.body, global: { plugins: [pinia, router] } })
  app = useAppStore(pinia)
  await flushPromises()
})

afterEach(() => wrapper.unmount())

describe('本机 PIN 确认弹窗', () => {
  test('在其他页面接收请求，轮询保留输入，确认时使用当前请求并关闭弹窗', async () => {
    app.apply(snapshotWithPairings({ id: firstId, name: 'Living room PC' }))
    await flushPromises()
    expect((wrapper.get('dialog').element as HTMLDialogElement).open).toBe(true)
    await wrapper.get('#incomingPairingPin').setValue('4827')
    app.apply(snapshotWithPairings({ id: firstId, name: 'Living room PC' }))
    await flushPromises()
    expect((wrapper.get('#incomingPairingPin').element as HTMLInputElement).value).toBe('4827')
    vi.mocked(bridge.sunshinePair).mockResolvedValue(snapshotWithPairings())
    await wrapper.get('dialog form').trigger('submit')
    await flushPromises()
    expect(bridge.sunshinePair).toHaveBeenCalledWith('4827', '', firstId)
    expect((wrapper.get('dialog').element as HTMLDialogElement).open).toBe(false)
    expect((wrapper.get('#incomingPairingPin').element as HTMLInputElement).value).toBe('')
  })

  test('切换设备清理 PIN，稍后处理后等待新请求才再次自动弹出', async () => {
    const pairings = [{ id: firstId, name: 'PC' }, { id: secondId, name: 'PC' }]
    app.apply(snapshotWithPairings(...pairings))
    await flushPromises()
    await wrapper.get('#incomingPairingPin').setValue('1234')
    await wrapper.get('#incomingPairingDevice').setValue(secondId)
    expect((wrapper.get('#incomingPairingPin').element as HTMLInputElement).value).toBe('')
    await wrapper.findAll('dialog button').find(button => button.text() === '稍后处理')!.trigger('click')
    app.apply(snapshotWithPairings(...pairings))
    await flushPromises()
    expect((wrapper.get('dialog').element as HTMLDialogElement).open).toBe(false)
    app.apply(snapshotWithPairings(...pairings, { id: thirdId, name: 'New PC' }))
    await flushPromises()
    expect((wrapper.get('dialog').element as HTMLDialogElement).open).toBe(true)
    expect((wrapper.get('#incomingPairingDevice').element as HTMLSelectElement).value).toBe(thirdId)
  })

  test('凭据失效时保留待处理请求并提供账号验证入口', async () => {
    app.apply(snapshotWithPairings({ id: firstId, name: 'PC' }))
    await flushPromises()
    await wrapper.get('#incomingPairingPin').setValue('4827')
    vi.mocked(bridge.sunshinePair).mockRejectedValue({ code: 'SUNSHINE_ACCOUNT_REQUIRED', message: '请验证 Sunshine 管理账号。' })
    await wrapper.get('dialog form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('dialog').text()).toContain('请验证 Sunshine 管理账号。')
    const invalidCredential = snapshotWithPairings({ id: firstId, name: 'PC' })
    invalidCredential.sunshine.credentialValid = false
    app.apply(invalidCredential)
    await flushPromises()
    expect((wrapper.get('dialog').element as HTMLDialogElement).open).toBe(true)
    await wrapper.findAll('dialog button').find(button => button.text() === '验证管理账号')!.trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/setup')
    expect((wrapper.get('#incomingPairingPin').element as HTMLInputElement).value).toBe('')
  })
})
