import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { bridge } from '../bridge'
import { isConnecting, normalizeError, retainDeviceOrder } from '../domain'
import { emptySnapshot, type AppError, type AppSnapshot, type ConnectRequest } from '../types'

export const useAppStore = defineStore('app', () => {
  const snapshot = ref(emptySnapshot())
  const initialized = ref(false)
  const loading = ref(false)
  const refreshing = ref(false)
  const error = ref<AppError | null>(null)
  const notice = ref('')
  const connectionRequestPending = ref(false)
  const disconnectingDeviceId = ref<string | null>(null)
  const selectedDeviceId = ref<string | null>(null)
  const firstConnectionObserved = ref(false)
  const selectedDevice = computed(() => snapshot.value.devices.find(d => d.id === selectedDeviceId.value) || null)
  const onlineAccountDevices = computed(() => snapshot.value.account.authenticated
    ? snapshot.value.devices.filter(device => device.accountOnline === true)
    : [])
  const busy = computed(() => connectionRequestPending.value || isConnecting(snapshot.value.session.stage) || snapshot.value.session.stage === 'streaming' || snapshot.value.session.localAcceptancePaused)
  const needsOnboarding = computed(() => !firstConnectionObserved.value && !snapshot.value.preferences.firstConnectionComplete)
  const onboardingPath = computed(() => snapshot.value.preferences.setupComplete ? snapshot.value.account.authenticated ? '/devices' : '/login' : '/setup')
  let initialization: Promise<void> | undefined
  let noticeTimer: ReturnType<typeof setTimeout> | undefined
  let operationGeneration = 0
  let activeOperation: Promise<void> | undefined
  let settingsQueue = Promise.resolve()

  function apply(value: AppSnapshot) {
    if (value.session.stage === 'streaming' || value.preferences.firstConnectionComplete) firstConnectionObserved.value = true
    const previous = value.account.username === snapshot.value.account.username ? snapshot.value.devices : []
    const devices = retainDeviceOrder(previous, value.devices)
    snapshot.value = { ...value, devices }
    if (!selectedDeviceId.value || !devices.some(d => d.id === selectedDeviceId.value)) selectedDeviceId.value = devices[0]?.id || null
  }
  function tell(message: string) { notice.value = message; clearTimeout(noticeTimer); noticeTimer = setTimeout(() => { notice.value = '' }, 4500) }
  async function run<T>(operation: () => Promise<T>): Promise<T | undefined> {
    if (loading.value) return
    operationGeneration++
    loading.value = true; error.value = null
    const completion = Promise.resolve().then(operation)
    activeOperation = completion.then(() => {}, () => {})
    try { return await completion } catch (caught) { error.value = normalizeError(caught); return undefined } finally {
      activeOperation = undefined
      loading.value = false
    }
  }
  function saveSetting(operation: () => Promise<AppSnapshot>) {
    settingsQueue = settingsQueue.then(async () => {
      if (activeOperation) await activeOperation
      await run(async () => { apply(await operation()) })
    })
    return settingsQueue
  }
  async function init() {
    if (initialization) return initialization
    initialization = (async () => {
      await bridge.listenSnapshot(value => { if (!loading.value) apply(value) })
      await bridge.listenSession(session => {
        if (session.stage === 'streaming') firstConnectionObserved.value = true
        snapshot.value.session = session
      })
      await bridge.listenOperationError(value => { error.value = value })
      await run(async () => apply(await bridge.snapshot()))
      initialized.value = true
    })()
    return initialization
  }
  async function refresh() { await run(async () => apply(await bridge.refresh())) }
  async function refreshBackground() {
    if (loading.value || refreshing.value) return
    refreshing.value = true
    const generation = operationGeneration
    try {
      const value = await bridge.refresh()
      if (!loading.value && generation === operationGeneration) apply(value)
    } catch { /* A background refresh must preserve foreground validation feedback. */ }
    finally { refreshing.value = false }
  }
  async function connect(request: ConnectRequest) {
    if (busy.value || loading.value) { tell('已有操作正在处理，请稍候或断开当前连接。'); return }
    const generation = ++operationGeneration
    connectionRequestPending.value = true
    error.value = null
    try {
      const session = await bridge.connect(request)
      if (generation === operationGeneration && snapshot.value.session.id !== session.id) {
        if (session.stage === 'streaming') firstConnectionObserved.value = true
        snapshot.value.session = session
      }
    } catch (caught) {
      if (generation === operationGeneration) error.value = normalizeError(caught)
    } finally { connectionRequestPending.value = false }
  }
  async function disconnectDevice(deviceId: string) {
    await run(async () => {
      disconnectingDeviceId.value = deviceId
      try {
        apply(await bridge.disconnectDevice(deviceId))
        tell('已断开连接。')
      } finally { disconnectingDeviceId.value = null }
    })
  }
  async function deleteSavedDevice(deviceId: string) {
    const value = await run(() => bridge.deleteDevice(deviceId))
    if (value) { apply(value); tell('已删除设备及连接记录。') }
    else await refreshBackground()
  }
  return { snapshot, initialized, loading, refreshing, error, notice, selectedDeviceId, selectedDevice, onlineAccountDevices, busy, disconnectingDeviceId, firstConnectionObserved, needsOnboarding, onboardingPath, init, apply, tell, run, saveSetting, refresh, refreshBackground, connect, disconnectDevice, deleteSavedDevice }
})
