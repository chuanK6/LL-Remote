import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { useAppStore } from '../../src/stores/app'
import { emptySnapshot, type SigninStatus } from '../../src/types'

const { getStatus, openSignin } = vi.hoisted(() => ({ getStatus: vi.fn(), openSignin: vi.fn() }))
vi.mock('../../src/bridge', () => ({ bridge: { signinStatus: getStatus, openSignin } }))

const websiteStatus: SigninStatus = {
  username: 'fixture',
  expiresAt: '2026-11-27T13:00:00+08:00',
  availableAt: '2026-11-27T11:00:00+08:00'
}

function accountSnapshot(username: string) {
  const snapshot = emptySnapshot()
  snapshot.account = { ...snapshot.account, username, authenticated: true }
  return snapshot
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.useFakeTimers()
  vi.setSystemTime(new Date('2026-11-27T10:59:59+08:00'))
  getStatus.mockReset()
  openSignin.mockReset()
})
afterEach(() => vi.useRealTimers())

it('shows a notice before the signup window and opens official sign-in at the threshold', async () => {
  const app = useAppStore()
  getStatus.mockResolvedValue(websiteStatus)
  app.apply(accountSnapshot('fixture'))
  await app.refreshSigninStatus()

  expect(app.signinAvailability).toBe('active')
  await app.openSignin()
  expect(app.notice).toBe('服务未到期，暂时无法签到。')
  expect(openSignin).not.toHaveBeenCalled()

  vi.setSystemTime(new Date('2026-11-27T11:00:00+08:00'))
  app.updateSigninClock()
  expect(app.signinAvailability).toBe('available')
  expect(app.signinReminder).toContain('13:00')
  expect(getStatus).toHaveBeenCalledTimes(1)
  await app.openSignin()
  expect(openSignin).toHaveBeenCalledTimes(1)
})

it('clears the old account date and ignores its pending response after switching accounts', async () => {
  const app = useAppStore()
  let finishFirst!: (status: SigninStatus) => void
  getStatus.mockReturnValueOnce(new Promise<SigninStatus>(resolve => { finishFirst = resolve }))
  getStatus.mockResolvedValueOnce({ ...websiteStatus, username: 'other', expiresAt: null })
  app.apply(accountSnapshot('fixture'))
  const first = app.refreshSigninStatus()

  app.apply(accountSnapshot('other'))
  const second = app.refreshSigninStatus()
  expect(app.signinStatus).toBeNull()
  finishFirst(websiteStatus)
  await Promise.all([first, second])

  expect(app.signinStatus?.username).toBe('other')
  expect(app.signinStatus?.expiresAt).toBeNull()
})

it('opens official sign-in when an older install has no stored cutoff', async () => {
  const app = useAppStore()
  getStatus.mockResolvedValue(null)
  app.apply(accountSnapshot('fixture'))
  await app.refreshSigninStatus()
  expect(app.signinAvailability).toBe('available')
  expect(app.serviceMinutes).toBe(0)
  await app.openSignin()
  expect(openSignin).toHaveBeenCalledTimes(1)
})
