<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { UserRound, ChevronDown, ArrowUpRight } from 'lucide-vue-next'
import { bridge, native } from './bridge'
import { normalizeError } from './domain'
import { useAppStore } from './stores/app'
import ConnectionBanner from './components/ConnectionBanner.vue'
import FeedbackMessage from './components/FeedbackMessage.vue'
import SessionPanel from './pages/SessionPanel.vue'
import SessionPerformance from './pages/SessionPerformance.vue'
import MainSidebar from './components/MainSidebar.vue'
import IncomingControlPanel from './components/IncomingControlPanel.vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type { IncomingControl } from './types'
const app = useAppStore()
const route = useRoute()
const router = useRouter()
const windowLabel = native ? getCurrentWindow().label : ''
const performanceWindow = windowLabel === 'session-performance'
const overlay = performanceWindow || windowLabel === 'session-controls' || new URLSearchParams(location.search).get('window') === 'session'
const guiding = computed(() => route.name === 'setup' || route.name === 'login')
const pairingDialog = ref<HTMLDialogElement>()
const pairingInput = ref<HTMLInputElement>()
const incomingPairingId = ref('')
const incomingPin = ref('')
const incomingName = ref('roth')
const pairingError = ref('')
const pendingPairings = computed(() => app.snapshot.sunshine.pendingPairings)
const incomingPairing = computed(() => pendingPairings.value.find(pairing => pairing.id === incomingPairingId.value))
let seenPairings = new Set<string>()
let refreshTimer: ReturnType<typeof setInterval> | undefined
let stopFocusListener: UnlistenFn | undefined
let stopIncomingListener: UnlistenFn | undefined
const incomingControl = ref<IncomingControl | null>(null)
const incomingExpanded = ref(false)
const deviceHasStream = computed(() => route.name === 'device' && route.params.id === app.snapshot.session.deviceId && app.snapshot.session.stage === 'streaming')

function updateIncomingControl(control: IncomingControl) {
  if (control.count === 0) {
    incomingControl.value = null
    incomingExpanded.value = false
    return
  }
  if (control.startedAt !== incomingControl.value?.startedAt || control.count > (incomingControl.value?.count ?? 0)) {
    incomingExpanded.value = true
  }
  incomingControl.value = control
  app.firstConnectionObserved = true
}

async function showPairingDialog() {
  await nextTick()
  if (overlay || !incomingPairing.value || !pairingDialog.value || pairingDialog.value.open) return
  pairingDialog.value.showModal()
  pairingInput.value?.focus()
}
function closePairingDialog() {
  if (app.loading) return
  pairingDialog.value?.close()
}
function clearPairingInput() {
  incomingPin.value = ''
  incomingName.value = 'roth'
  pairingError.value = ''
}
async function confirmIncomingPin() {
  if (!/^\d{4}$/.test(incomingPin.value)) {
    pairingError.value = '请输入控制端显示的四位数字 PIN。'
    return
  }
  if (!incomingName.value.trim()) {
    pairingError.value = '请输入配对设备名称。'
    return
  }
  const selectedId = incomingPairingId.value
  if (!selectedId || !incomingPairing.value) return
  pairingError.value = ''
  const result = await app.run(() => bridge.sunshinePair(incomingPin.value, incomingName.value.trim(), selectedId))
  if (!result) {
    pairingError.value = app.error?.message || ''
    if (app.error?.code === 'PAIRING_FAILED') incomingPin.value = ''
    return
  }
  pairingDialog.value?.close()
  clearPairingInput()
  app.apply(result)
  app.tell('配对已确认，控制端将自动连接桌面。')
}
async function verifySunshine() {
  closePairingDialog()
  await router.push('/setup')
}
watch(incomingPairingId, clearPairingInput)
watch(pendingPairings, pairings => {
  if (overlay) return
  const newPairing = pairings.find(pairing => !seenPairings.has(pairing.id))
  seenPairings = new Set(pairings.map(pairing => pairing.id))
  if (!pairings.length) {
    pairingDialog.value?.close()
    incomingPairingId.value = ''
    clearPairingInput()
    return
  }
  if (!pairings.some(pairing => pairing.id === incomingPairingId.value)) incomingPairingId.value = pairings[0].id
  if (newPairing && !pairingDialog.value?.open) {
    incomingPairingId.value = newPairing.id
    void showPairingDialog()
  }
}, { deep: true, immediate: true, flush: 'post' })
async function signIn() { if (guiding.value) await router.push('/account'); try { await bridge.openSignin() } catch (caught) { app.error = normalizeError(caught) } }
onMounted(async () => {
  if (overlay) return
  await app.init()
  if (!native && route.name === 'devices' && app.snapshot.devices[0]) await router.replace(`/device/${app.snapshot.devices[0].id}`)
  if (native) {
    const window = getCurrentWindow()
    let receivedUpdate = false
    stopIncomingListener = await bridge.listenIncomingControl(control => {
      receivedUpdate = true
      updateIncomingControl(control)
    })
    void bridge.incomingControl().then(control => {
      if (!receivedUpdate) updateIncomingControl(control)
    }).catch(caught => { app.error = normalizeError(caught) })
    stopFocusListener = await window.onFocusChanged(event => { if (event.payload && app.initialized) app.refreshBackground() })
    refreshTimer = setInterval(() => { if (app.initialized) app.refreshBackground() }, 4000)
  }
})
onBeforeUnmount(() => { clearInterval(refreshTimer); stopFocusListener?.(); stopIncomingListener?.(); clearPairingInput() })
watch(() => app.snapshot.session.stage, stage => {
  if (overlay) return
  if (stage === 'pairing') router.push('/pairing')
  else if (stage === 'streaming' && route.name === 'pairing' && app.snapshot.session.deviceId) router.replace(`/device/${app.snapshot.session.deviceId}`)
})
watch(() => route.fullPath, () => { app.error = null; document.querySelector('.workspace')?.scrollTo({ top: 0 }); if (route.params.id) app.selectedDeviceId = String(route.params.id) })
</script>
<template>
  <SessionPerformance v-if="performanceWindow" />
  <SessionPanel v-else-if="overlay" />
  <div v-else class="shell" :class="{ 'incoming-expanded': incomingExpanded }">
    <header class="topbar">
      <div class="top-actions">
        <button v-if="app.initialized && app.needsOnboarding" class="onboarding-button" @click="router.push(app.onboardingPath)">完成首次连接</button>
        <button class="account-button" @click="router.push('/account')">
          <UserRound class="icon" />
          <span>{{ app.snapshot.account.username || '皎月连账号' }}</span>
          <ChevronDown class="chevron-icon" />
        </button>
        <button class="signin-button" @click="signIn">签到 <ArrowUpRight :size="14" /></button>
      </div>
    </header>
    <MainSidebar :hidden="incomingExpanded">
      <IncomingControlPanel v-if="incomingControl && !incomingExpanded" :control="incomingControl" compact @toggle="incomingExpanded = true" />
    </MainSidebar>
    <IncomingControlPanel v-if="incomingControl && incomingExpanded" :control="incomingControl" @toggle="incomingExpanded = false" />
    <main v-show="!incomingExpanded" class="workspace" id="pageContent">
      <ConnectionBanner v-if="!deviceHasStream" />
      <FeedbackMessage v-if="app.error" :message="app.error.message" kind="error" />
      <FeedbackMessage v-if="app.snapshot.storageWarning" :message="app.snapshot.storageWarning" kind="warning" />
      <div v-if="pendingPairings.length" class="connection-banner" role="status"><span>有设备等待本机确认配对 PIN。</span><button class="small-button" @click="showPairingDialog">确认配对 PIN</button></div>
      <div v-if="app.snapshot.sunshine.running && app.snapshot.sunshine.accountInitialized && !app.snapshot.sunshine.credentialValid" class="connection-banner" role="status"><span>验证 Sunshine 管理账号后，可在本机接收并确认配对 PIN。</span><button class="small-button" @click="verifySunshine">验证 Sunshine 管理账号</button></div>
      <RouterView v-if="app.initialized" v-slot="{ Component }">
        <div :key="route.fullPath" class="page-content">
          <component :is="Component" />
        </div>
      </RouterView>
      <div v-else class="loading-page"><span class="spinner"/>正在读取本机配置…</div>
    </main>
  </div>
  <dialog v-if="!overlay" ref="pairingDialog" class="pairing-dialog" aria-labelledby="incomingPairingTitle" @close="clearPairingInput" @cancel="app.loading && $event.preventDefault()">
    <h2 id="incomingPairingTitle">确认远程连接 PIN</h2>
    <p>有设备请求控制本机。请输入对方 LL远程显示的四位 PIN，确认后对方会自动进入桌面。</p>
    <form @submit.prevent="confirmIncomingPin">
      <label class="form-label" for="incomingPairingDevice">请求连接的设备</label>
      <select id="incomingPairingDevice" v-model="incomingPairingId" :disabled="app.loading"><option v-for="pairing in pendingPairings" :key="pairing.id" :value="pairing.id">{{ pairing.name || '新设备' }} · {{ pairing.id.slice(0, 8) }}</option></select>
      <label class="form-label" for="incomingPairingPin">控制端显示的四位 PIN</label>
      <input id="incomingPairingPin" ref="pairingInput" v-model="incomingPin" class="text-input" inputmode="numeric" pattern="[0-9]{4}" maxlength="4" required autocomplete="off" :disabled="app.loading" placeholder="四位数字 PIN" />
      <label class="form-label" for="incomingPairingName">配对设备名称</label>
      <input id="incomingPairingName" v-model="incomingName" class="text-input" maxlength="128" autocomplete="off" required :disabled="app.loading" />
      <FeedbackMessage v-if="!app.snapshot.sunshine.credentialValid" message="请先验证本机 Sunshine 管理账号，再确认此次配对。" kind="warning" />
      <FeedbackMessage :message="pairingError" kind="error" />
      <div class="page-actions"><button v-if="!app.snapshot.sunshine.credentialValid" type="button" class="secondary" :disabled="app.loading" @click="verifySunshine">验证管理账号</button><button type="button" class="secondary" :disabled="app.loading" @click="closePairingDialog">稍后处理</button><button type="submit" class="primary" :disabled="app.loading || !app.snapshot.sunshine.credentialValid">{{ app.loading ? '正在确认…' : '确认并允许连接' }}</button></div>
    </form>
  </dialog>
  <div v-if="!native && !overlay" class="preview-note">界面预览 · 未连接 Windows 内核</div>
  <div v-if="app.notice && !overlay" class="toast visible" role="status">{{ app.notice }}</div>
</template>

<style scoped>
.shell { min-width: 0; min-height: 0; }
.topbar { grid-column: 2; grid-row: 1; }
.workspace { grid-column: 2; grid-row: 2; }
.incoming-expanded .topbar { grid-column: 1 / -1; }
.top-actions { min-width: 0; margin-left: auto; }
.account-button { min-width: 0; max-width: 220px; }
.account-button .icon { flex-shrink: 0; }
.account-button span { overflow: hidden; text-overflow: ellipsis; }
.signin-button { flex-shrink: 0; }
@media (prefers-reduced-motion: no-preference) {
  .page-content { animation: page-enter 460ms cubic-bezier(.2, .75, .3, 1) both; }
}
@keyframes page-enter {
  from { opacity: 0; transform: translateY(30px); }
  to { opacity: 1; transform: none; }
}
@media (max-width: 900px) {
  .shell { grid-template-columns: 210px minmax(0, 1fr); }
  .workspace { padding-inline: 20px; }
}
@media (max-width: 700px) {
  .shell { grid-template-columns: clamp(138px, 29vw, 190px) minmax(0, 1fr); }
  .workspace { padding-inline: 14px; }
}
.pairing-dialog {
  width: min(440px, calc(100vw - 64px));
  border: 1px solid #d7e4ef;
  border-radius: 12px;
  padding: 26px;
  background: #fff;
  color: #314b61;
  box-shadow: 0 12px 48px #17334d30;
}
.pairing-dialog::backdrop {
  background: #152c4566;
}
.pairing-dialog h2 {
  font-size: 20px;
  font-weight: 500;
  margin: 0 0 14px;
}
.pairing-dialog p {
  font-size: 13px;
  line-height: 1.8;
  color: #637c90;
}
.pairing-dialog select {
  width: 100%;
  height: 38px;
}
.pairing-dialog .page-actions {
  flex-wrap: wrap;
}
</style>
