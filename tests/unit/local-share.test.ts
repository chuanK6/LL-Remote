import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import { bridge } from '../../src/bridge'
import { previewSnapshot } from '../../src/preview'
import { formatLocalShare } from '../../src/local-share'
import { useAppStore } from '../../src/stores/app'
import type { AppSnapshot } from '../../src/types'
import AssistPage from '../../src/pages/AssistPage.vue'
import AppsPage from '../../src/pages/AppsPage.vue'

vi.mock('../../src/bridge', () => ({
  native: true,
  bridge: {
    localServerPasswords: vi.fn(), acceptControl: vi.fn(), connect: vi.fn(),
    openAdvanced: vi.fn(), savePreferences: vi.fn(), saveStream: vi.fn(),
    sunshineSettings: vi.fn(), sunshineControl: vi.fn(), sunshinePair: vi.fn(),
  },
}))

const writeText = vi.fn()
const originalClipboard = Object.getOwnPropertyDescriptor(navigator, 'clipboard')
let wrapper: VueWrapper | undefined

beforeEach(() => {
  vi.resetAllMocks()
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } })
  writeText.mockResolvedValue(undefined)
  vi.mocked(bridge.localServerPasswords).mockResolvedValue({ connectionPassword: 'fixture-connection', webPassword: 'fixture-web-only' })
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  if (originalClipboard) Object.defineProperty(navigator, 'clipboard', originalClipboard)
  else Reflect.deleteProperty(navigator, 'clipboard')
})

function sharingSnapshot() {
  const snapshot = previewSnapshot()
  Object.assign(snapshot.account, { localCode: '12345678', localVpnAddress: '10.9.0.1', serverRunning: true })
  snapshot.preferences.acceptControl = true
  snapshot.sunshine.running = true
  snapshot.sunshine.ready = true
  return snapshot
}

function passwordInputSnapshot(username: string) {
  const snapshot = sharingSnapshot()
  snapshot.account.username = username
  snapshot.account.serverRunning = false
  snapshot.preferences.acceptControl = false
  return snapshot
}

async function render(path: string, snapshot: AppSnapshot) {
  const pinia = createPinia()
  setActivePinia(pinia)
  const app = useAppStore()
  app.apply(snapshot)
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/assist', component: AssistPage }, { path: '/app/:app', component: AppsPage }],
  })
  await router.push(path)
  await router.isReady()
  wrapper = mount(path.startsWith('/app/') ? AppsPage : AssistPage, { global: { plugins: [pinia, router] } })
  await flushPromises()
  return { app, page: wrapper, pinia, router }
}

describe('本机协助信息分享', () => {
  it('首次登录和同账号短暂认证变化保留未提交密码', async () => {
    const snapshot = passwordInputSnapshot('account-a')
    snapshot.account.authenticated = false
    snapshot.account.username = null
    const { app, page } = await render('/assist', snapshot)
    await page.get('#localAssistPassword').setValue('account-a-first-draft')
    await page.get('#assistWebPassword').setValue('account-a-first-web')
    app.apply(passwordInputSnapshot('account-a'))
    await flushPromises()
    for (const username of ['account-a', null]) {
      const transient = passwordInputSnapshot('account-a')
      transient.account.authenticated = false
      transient.account.username = username
      app.apply(transient)
      await flushPromises()
      app.apply(passwordInputSnapshot('account-a'))
      await flushPromises()
      expect(page.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('account-a-first-draft')
      expect(page.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('account-a-first-web')
    }
  })

  it('真实切换账号清除旧草稿并读取新账号保存的密码', async () => {
    const { app, page } = await render('/assist', passwordInputSnapshot('account-a'))
    await page.get('#localAssistPassword').setValue('account-a-unsubmitted')
    await page.get('#assistWebPassword').setValue('account-a-unsubmitted-web')
    vi.mocked(bridge.localServerPasswords).mockResolvedValue({ connectionPassword: 'account-b-saved', webPassword: 'account-b-web' })
    app.apply(passwordInputSnapshot('account-b'))
    await flushPromises()
    expect(page.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('account-b-saved')
    expect(page.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('account-b-web')
  })

  it('离开协助页期间切换账号不会恢复上一账号草稿', async () => {
    const { app, page, pinia, router } = await render('/assist', passwordInputSnapshot('account-a'))
    await page.get('#localAssistPassword').setValue('account-a-away-draft')
    await page.get('#assistWebPassword').setValue('account-a-away-web')
    page.unmount()
    await router.push('/app/jiaoyue')
    app.apply(passwordInputSnapshot('account-b'))
    vi.mocked(bridge.localServerPasswords).mockResolvedValue({ connectionPassword: 'account-b-saved', webPassword: 'account-b-web' })
    await router.push('/assist')
    wrapper = mount(AssistPage, { global: { plugins: [pinia, router] } })
    await flushPromises()
    expect(wrapper.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('account-b-saved')
    expect(wrapper.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('account-b-web')
  })

  it('旧账号迟到密码不会恢复旧草稿或覆盖新账号编辑', async () => {
    let finishA!: (value: { connectionPassword: string; webPassword: string }) => void
    let finishB!: (value: { connectionPassword: string; webPassword: string }) => void
    vi.mocked(bridge.localServerPasswords).mockImplementationOnce(() => new Promise(resolve => { finishA = resolve }))
    const { app, page } = await render('/assist', passwordInputSnapshot('account-a'))
    await page.get('#localAssistPassword').setValue('account-a-pending-draft')
    await page.get('#assistWebPassword').setValue('account-a-pending-web')
    vi.mocked(bridge.localServerPasswords).mockImplementationOnce(() => new Promise(resolve => { finishB = resolve }))
    app.apply(passwordInputSnapshot('account-b'))
    await flushPromises()
    expect(page.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('')
    expect(page.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('')
    await page.get('#localAssistPassword').setValue('account-b-new-draft')
    await page.get('#assistWebPassword').setValue('')
    finishA({ connectionPassword: 'account-a-old-saved', webPassword: 'account-a-old-web' })
    await flushPromises()
    expect(bridge.localServerPasswords).toHaveBeenCalledTimes(2)
    expect(page.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('account-b-new-draft')
    expect(page.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('')
    finishB({ connectionPassword: 'account-b-saved', webPassword: 'account-b-web' })
    await flushPromises()
    expect(page.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('account-b-new-draft')
    expect(page.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('')
  })

  it('旧账号读取失败后继续处理新账号排队的密码读取', async () => {
    let failA!: (error: unknown) => void
    vi.mocked(bridge.localServerPasswords).mockImplementationOnce(() => new Promise((_, reject) => { failA = reject }))
    const snapshot = sharingSnapshot()
    snapshot.account.username = 'account-a'
    const { app, page } = await render('/assist', snapshot)
    vi.mocked(bridge.localServerPasswords).mockResolvedValue({ connectionPassword: 'account-b-saved', webPassword: 'account-b-web' })
    const next = sharingSnapshot()
    next.account.username = 'account-b'
    app.apply(next)
    await flushPromises()
    failA({ code: 'HOST_TIMEOUT' })
    await flushPromises()
    expect(bridge.localServerPasswords).toHaveBeenCalledTimes(2)
    expect(page.get<HTMLInputElement>('[aria-label="当前本机连接密码"]').element.value).toBe('account-b-saved')
  })

  it('旧账号迟到的开启结果不会恢复旧账号或解除新草稿保护', async () => {
    const { app, page } = await render('/assist', passwordInputSnapshot('account-a'))
    await page.get('#localAssistPassword').setValue('account-a-submitted')
    let finish!: (value: AppSnapshot) => void
    vi.mocked(bridge.acceptControl).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    await page.get('[aria-label="启用本机接受控制"]').trigger('click')
    vi.mocked(bridge.localServerPasswords).mockResolvedValue({ connectionPassword: 'account-b-saved', webPassword: 'account-b-web' })
    app.apply(passwordInputSnapshot('account-b'))
    await flushPromises()
    await page.get('#localAssistPassword').setValue('account-b-pending-draft')
    await page.get('#assistWebPassword').setValue('account-b-pending-web')
    const accepted = sharingSnapshot()
    accepted.account.username = 'account-a'
    finish(accepted)
    await flushPromises()
    expect(app.snapshot.account.username).toBe('account-b')
    expect(app.snapshot.preferences.acceptControl).toBe(false)
    app.snapshot.account.serverRunning = true
    await flushPromises()
    expect(page.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('account-b-pending-draft')
    expect(page.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('account-b-pending-web')
  })

  it('后台连续替换快照时保留首次编辑的连接密码和页面密码', async () => {
    const snapshot = sharingSnapshot()
    snapshot.preferences.acceptControl = false
    snapshot.account.serverRunning = false
    const { app, page } = await render('/assist', snapshot)
    await page.get('#localAssistPassword').setValue('first-draft-connection')
    await page.get('#assistWebPassword').setValue('first-draft-web')
    for (let tick = 1; tick <= 3; tick++) {
      const update = structuredClone(snapshot)
      update.account.updatedAt = `2026-11-22T12:00:0${tick}Z`
      app.apply(update)
      await flushPromises()
      expect(page.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('first-draft-connection')
      expect(page.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('first-draft-web')
    }
  })

  it('迟到的已保存密码不会覆盖正在编辑或有意清空的输入', async () => {
    const snapshot = sharingSnapshot()
    snapshot.preferences.acceptControl = false
    snapshot.account.serverRunning = false
    let finish!: (value: { connectionPassword: string; webPassword: string }) => void
    vi.mocked(bridge.localServerPasswords).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    const { page } = await render('/assist', snapshot)
    await page.get('#localAssistPassword').setValue('new-draft')
    await page.get('#assistWebPassword').setValue('temporary')
    await page.get('#assistWebPassword').setValue('')
    finish({ connectionPassword: 'old-saved', webPassword: 'old-web-saved' })
    await flushPromises()
    expect(page.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('new-draft')
    expect(page.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('')
  })

  it('切到其他页面再回来保留尚未提交的本机密码草稿', async () => {
    const snapshot = sharingSnapshot()
    snapshot.preferences.acceptControl = false
    snapshot.account.serverRunning = false
    const { page, pinia, router } = await render('/assist', snapshot)
    await page.get('#localAssistPassword').setValue('return-draft-connection')
    await page.get('#assistWebPassword').setValue('return-draft-web')
    page.unmount()
    await router.push('/app/jiaoyue')
    await router.push('/assist')
    wrapper = mount(AssistPage, { global: { plugins: [pinia, router] } })
    await flushPromises()
    expect(wrapper.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('return-draft-connection')
    expect(wrapper.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('return-draft-web')
  })

  it('开启操作返回时保留用户在请求期间新改的密码草稿', async () => {
    const snapshot = sharingSnapshot()
    snapshot.preferences.acceptControl = false
    snapshot.account.serverRunning = false
    const { app, page } = await render('/assist', snapshot)
    await page.get('#localAssistPassword').setValue('submitted-connection')
    await page.get('#assistWebPassword').setValue('submitted-web')
    let finish!: (value: AppSnapshot) => void
    vi.mocked(bridge.acceptControl).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    await page.get('[aria-label="启用本机接受控制"]').trigger('click')
    await page.get('#localAssistPassword').setValue('later-connection')
    await page.get('#assistWebPassword').setValue('later-web')
    vi.mocked(bridge.localServerPasswords).mockResolvedValue({ connectionPassword: 'submitted-connection', webPassword: 'submitted-web' })
    finish(sharingSnapshot())
    await flushPromises()
    app.apply(snapshot)
    await flushPromises()
    expect(page.get<HTMLInputElement>('#localAssistPassword').element.value).toBe('later-connection')
    expect(page.get<HTMLInputElement>('#assistWebPassword').element.value).toBe('later-web')
  })

  it('用户点击后一次复制访问码、当前连接密码和真实组网地址，排除管理页面密码', async () => {
    const snapshot = sharingSnapshot()
    const { page } = await render('/assist', snapshot)
    const field = page.get<HTMLInputElement>('[aria-label="当前本机连接密码"]')
    expect(field.attributes('type')).toBe('password')
    expect(page.get('[aria-label="本机访问码"]').text()).toBe('12345678')
    expect(page.get('[aria-label="本机组网地址"]').text()).toBe('10.9.0.1')
    expect(writeText).not.toHaveBeenCalled()

    await page.get('[aria-label="显示当前连接密码"]').trigger('click')
    expect(field.attributes('type')).toBe('text')
    expect(field.element.value).toBe('fixture-connection')
    await page.findAll('button').find(b=>b.text()==='全部复制')!.trigger('click')
    await flushPromises()
    expect(writeText).toHaveBeenCalledWith('访问码：12345678\n连接密码：fixture-connection\n组网地址：10.9.0.1')
    expect(writeText.mock.calls[0][0]).not.toContain('fixture-web-only')
    expect(JSON.stringify(snapshot)).not.toContain('fixture-connection')
  })

  it('地址尚未取得时显示等待状态并阻止复制不完整的连接信息', async () => {
    const snapshot = sharingSnapshot()
    Object.assign(snapshot.account, { localVpnAddress: null })
    const { page } = await render('/assist', snapshot)
    expect(page.get('[aria-label="本机组网地址"]').text()).toBe('正在读取组网地址')
    const copy = page.findAll('button').find(b=>b.text()==='全部复制')!
    expect(copy.attributes('disabled')).toBeDefined()
    await copy.trigger('click')
    expect(writeText).not.toHaveBeenCalled()
  })

  it('开启接受控制后重新读取生效密码，不复制开启前的旧值', async () => {
    const snapshot = sharingSnapshot()
    snapshot.preferences.acceptControl = false
    snapshot.account.serverRunning = false
    const { page } = await render('/assist', snapshot)
    await page.get('#localAssistPassword').setValue('new-connection')
    const enabled = sharingSnapshot()
    vi.mocked(bridge.acceptControl).mockResolvedValueOnce(enabled)
    vi.mocked(bridge.localServerPasswords).mockResolvedValue({ connectionPassword: 'new-connection', webPassword: 'fixture-web-only' })
    await page.get('[aria-label="启用本机接受控制"]').trigger('click')
    await flushPromises()
    expect(bridge.acceptControl).toHaveBeenCalledWith(true, 'new-connection', 'fixture-web-only')
    expect(page.find('#localAssistPassword').exists()).toBe(false)
    await page.findAll('button').find(b=>b.text()==='全部复制')!.trigger('click')
    expect(writeText).toHaveBeenCalledWith('访问码：12345678\n连接密码：new-connection\n组网地址：10.9.0.1')
  })

  it('已退出账号时立即移除上一账号的可复制信息', async () => {
    const { app, page } = await render('/assist', sharingSnapshot())
    app.snapshot.account.authenticated = false
    await flushPromises()
    expect(page.get('[aria-label="本机访问码"]').text()).toBe('登录后读取访问码')
    expect(page.get('[aria-label="本机组网地址"]').text()).toBe('登录后读取组网地址')
    expect(page.find('[aria-label="当前本机连接密码"]').exists()).toBe(false)
    expect(page.findAll('button').find(b=>b.text()==='全部复制')!.attributes('disabled')).toBeDefined()
  })

  it('皎月连设置展示本机地址，并由用户显式点击复制', async () => {
    const { page } = await render('/app/jiaoyue', sharingSnapshot())
    expect(page.get('[aria-label="本机组网地址"]').text()).toBe('10.9.0.1')
    expect(writeText).not.toHaveBeenCalled()
    await page.findAll('button').find(b=>b.text()==='复制地址')!.trigger('click')
    expect(writeText).toHaveBeenCalledWith('10.9.0.1')
  })

  it('密码中的空格和符号原样保留，缺少任一分享字段时不生成完整分享文本', () => {
    expect(formatLocalShare({ accessCode: '12345678', connectionPassword: '  exact $()" password  ', vpnAddress: '10.9.0.1' })).toContain('连接密码：  exact $()" password  \n')
    expect(formatLocalShare({ accessCode: '12345678', connectionPassword: '', vpnAddress: '10.9.0.1' })).toBeNull()
    expect(formatLocalShare({ accessCode: null, connectionPassword: 'fixture', vpnAddress: '10.9.0.1' })).toBeNull()
  })
})
