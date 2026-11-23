<script lang="ts">
type AssistPasswordDraft = {
  owner: string | null; ownerRevision: number
  connection: string; web: string; connectionEdited: boolean; webEdited: boolean
  connectionRevision: number; webRevision: number
}
// Unsaved form input survives route changes in this window, without disk storage.
const assistPasswordDrafts = new WeakMap<object, AssistPasswordDraft>()
</script>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Copy, ArrowRight, Eye, EyeOff } from 'lucide-vue-next'
import { bridge, native } from '../bridge'
import { formatLocalShare, localVpnAddressState } from '../local-share'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
import FeedbackMessage from '../components/FeedbackMessage.vue'
import ToggleSwitch from '../components/ToggleSwitch.vue'

const app = useAppStore()
const route = useRoute()
const router = useRouter()
const device = computed(() => app.snapshot.devices.find(d => d.id === route.query.device))
const mode = ref(route.query.mode === 'other' ? 'other' : 'self')
const code = ref(device.value?.peerCode || '')
const password = ref('')
const hostAddress = ref(device.value?.address || '')
const passwordDraft = assistPasswordDrafts.get(app) || {
  owner: null, ownerRevision: 0,
  connection: '', web: '', connectionEdited: false, webEdited: false,
  connectionRevision: 0, webRevision: 0,
}
assistPasswordDrafts.set(app, passwordDraft)
function confirmedPasswordAccount(): string | null {
  const { authenticated, username } = app.snapshot.account
  return authenticated && username?.trim() ? username : null
}
function syncPasswordDraftOwner(): boolean {
  const owner = confirmedPasswordAccount()
  if (!owner || owner === passwordDraft.owner) return false
  const changed = passwordDraft.owner !== null
  passwordDraft.owner = owner
  passwordDraft.ownerRevision++
  if (!changed) return false
  passwordDraft.connection = ''
  passwordDraft.web = ''
  passwordDraft.connectionEdited = false
  passwordDraft.webEdited = false
  passwordDraft.connectionRevision++
  passwordDraft.webRevision++
  return true
}
syncPasswordDraftOwner()
const localPassword = ref(passwordDraft.connection)
const webPassword = ref(passwordDraft.web)
const savedConnectionPassword = ref('')
const showConnectionPassword = ref(false)
const passwordsLoading = ref(false)
const validation = ref('')
const acceptingChange = ref<'starting' | 'stopping' | null>(null)
const localAccessCode = computed(() => app.snapshot.account.authenticated ? app.snapshot.account.localCode : null)
const localAddress = computed(() => app.snapshot.account.authenticated ? app.snapshot.account.localVpnAddress : null)
const networkActive = computed(() => app.snapshot.account.serverRunning || app.snapshot.devices.some(d => d.networkConnected))
const localAddressState = computed(() => localVpnAddressState(app.snapshot.account.authenticated, networkActive.value))
const sharingActive = computed(() => app.snapshot.preferences.acceptControl && app.snapshot.account.authenticated && app.snapshot.account.serverRunning)
const shareText = computed(() => sharingActive.value && !passwordsLoading.value
  ? formatLocalShare({ accessCode: localAccessCode.value, connectionPassword: savedConnectionPassword.value, vpnAddress: localAddress.value })
  : null)
let active = true, passwordEpoch = 0, passwordReadQueued = false
let passwordRead: Promise<void> | undefined

async function readPasswords(force = false): Promise<void> {
  if (!active || !native || !confirmedPasswordAccount()) return
  if (passwordRead) {
    if (force) passwordReadQueued = true
    return passwordRead
  }
  passwordsLoading.value = true
  passwordRead = (async () => {
    try {
      do {
        passwordReadQueued = false
        const account = confirmedPasswordAccount()
        const epoch = passwordEpoch
        if (!account) break
        try {
          const value = await bridge.localServerPasswords()
          if (!active || epoch !== passwordEpoch || confirmedPasswordAccount() !== account) continue
          savedConnectionPassword.value = value.connectionPassword
          if (!passwordDraft.connectionEdited && value.connectionPassword) {
            localPassword.value = value.connectionPassword
            passwordDraft.connection = value.connectionPassword
          }
          if (!passwordDraft.webEdited && value.webPassword) {
            webPassword.value = value.webPassword
            passwordDraft.web = value.webPassword
          }
        } catch {
          if (active && epoch === passwordEpoch && confirmedPasswordAccount() === account) savedConnectionPassword.value = ''
        }
      } while (active && confirmedPasswordAccount() && passwordReadQueued)
    } finally {
      passwordsLoading.value = false
      passwordRead = undefined
    }
  })()
  await passwordRead
}
onMounted(() => readPasswords())
watch([() => app.snapshot.account.authenticated, () => app.snapshot.account.username, () => app.snapshot.account.serverRunning], () => {
  if (syncPasswordDraftOwner()) {
    localPassword.value = passwordDraft.connection
    webPassword.value = passwordDraft.web
  }
  passwordEpoch++
  savedConnectionPassword.value = ''
  showConnectionPassword.value = false
  void readPasswords(true)
})
async function connect() {
  validation.value = ''
  const cleanCode = code.value.replace(/\s/g, '')
  if (!/^\d{4,32}$/.test(cleanCode)) { validation.value = '请输入 4–32 位数字访问码。'; return }
  if (!password.value && !device.value?.credentialRef) { validation.value = '首次连接需要填写对方的连接密码。'; return }
  const address = hostAddress.value.trim()
  if (!address) { validation.value = '请输入对方的组网地址。'; return }
  await app.connect({ deviceId: device.value?.id, peerCode: cleanCode, password: password.value || undefined, address, app: 'Desktop' })
  if (!app.error) password.value = ''
  if (app.error?.code === 'LOGIN_REQUIRED') await router.push('/login')
}
async function accepting(enabled: boolean) {
  if (acceptingChange.value || app.loading) return
  const account = app.snapshot.account.username, ownerRevision = passwordDraft.ownerRevision
  const connection = localPassword.value, web = webPassword.value
  const connectionRevision = passwordDraft.connectionRevision, webRevision = passwordDraft.webRevision
  acceptingChange.value = enabled ? 'starting' : 'stopping'
  const result = await app.run(() => bridge.acceptControl(enabled, connection, web))
    .finally(() => { acceptingChange.value = null })
  if (!result || ownerRevision !== passwordDraft.ownerRevision || app.snapshot.account.username !== account || (result.account.authenticated && result.account.username !== account)) return
  if (enabled && connectionRevision === passwordDraft.connectionRevision) passwordDraft.connectionEdited = false
  if (enabled && webRevision === passwordDraft.webRevision) passwordDraft.webEdited = false
  app.apply(result)
  await readPasswords(true)
}
function editConnectionPassword(event: Event) {
  localPassword.value = (event.target as HTMLInputElement).value
  passwordDraft.connection = localPassword.value
  passwordDraft.connectionEdited = true
  passwordDraft.connectionRevision++
}
function editWebPassword(event: Event) {
  webPassword.value = (event.target as HTMLInputElement).value
  passwordDraft.web = webPassword.value
  passwordDraft.webEdited = true
  passwordDraft.webRevision++
}
async function copyCode() {
  if (!localAccessCode.value) return
  try { await navigator.clipboard.writeText(localAccessCode.value); app.tell('访问码已复制。') }
  catch { app.tell('无法访问剪贴板，请手动复制访问码。') }
}
async function copyAll() {
  if (!shareText.value) return
  try { await navigator.clipboard.writeText(shareText.value); app.tell('访问码、连接密码和组网地址已全部复制。') }
  catch { app.tell('无法访问剪贴板，请手动复制本机连接信息。') }
}
onBeforeUnmount(() => {
  active = false
  passwordEpoch++
  password.value = ''
  localPassword.value = ''
  webPassword.value = ''
  savedConnectionPassword.value = ''
})
</script>

<template>
  <PageHeading title="开始协助" />
  <div class="inline-tabs assist-tabs">
    <button :class="{ active: mode === 'self' }" @click="mode = 'self'">让对方连接我</button>
    <button :class="{ active: mode === 'other' }" @click="mode = 'other'">连接对方电脑</button>
  </div>
  <section v-if="mode === 'self'" class="assist-card">
    <div class="assist-toolbar">
      <h2>本机接受控制</h2>
      <ToggleSwitch :model-value="app.snapshot.preferences.acceptControl" label="启用本机接受控制" :disabled="app.loading || !!acceptingChange || !native" @update:model-value="accepting" />
    </div>
    <p v-if="acceptingChange" class="setting-detail" role="status"><span class="spinner" aria-hidden="true" />{{ acceptingChange === 'starting' ? '正在开启本机接受控制…' : '正在停止本机接受控制…' }}</p>
    <div class="assist-status" aria-label="本机服务状态">
      <span :class="{ good: app.snapshot.account.serverRunning }">组网{{ app.snapshot.account.serverRunning ? '已开启' : '未开启' }}</span>
      <span :class="{ good: app.snapshot.sunshine.ready }">Sunshine {{ app.snapshot.sunshine.ready ? '已就绪' : app.snapshot.sunshine.running ? '准备中' : '未启动' }}</span>
    </div>
    <div v-if="!app.snapshot.preferences.acceptControl" class="assist-passwords">
      <div>
        <label for="localAssistPassword">连接密码</label>
        <input id="localAssistPassword" :value="localPassword" class="text-input" type="password" autocomplete="new-password" placeholder="设置连接密码" @input="editConnectionPassword" />
      </div>
      <div>
        <label for="assistWebPassword">页面密码（可选）</label>
        <input id="assistWebPassword" :value="webPassword" class="text-input" type="password" autocomplete="new-password" minlength="6" maxlength="20" placeholder="6–20 位，留空复用" @input="editWebPassword" />
      </div>
    </div>
    <div class="assist-info">
      <div class="assist-info-row">
        <span class="assist-label">访问码</span>
        <span class="share-value" aria-label="本机访问码">{{ localAccessCode || '登录后读取访问码' }}</span>
        <button type="button" class="tiny-button" :disabled="!localAccessCode" @click="copyCode">复制访问码</button>
      </div>
      <div class="assist-info-row">
        <span class="assist-label">连接密码</span>
        <template v-if="sharingActive">
          <input :value="savedConnectionPassword" :type="showConnectionPassword ? 'text' : 'password'" class="text-input share-password" aria-label="当前本机连接密码" readonly :placeholder="passwordsLoading ? '读取中' : '尚未取得'" />
          <button type="button" class="plain-icon share-reveal" :aria-label="showConnectionPassword ? '隐藏当前连接密码' : '显示当前连接密码'" :disabled="!savedConnectionPassword" @click="showConnectionPassword = !showConnectionPassword"><EyeOff v-if="showConnectionPassword" :size="17" /><Eye v-else :size="17" /></button>
        </template>
        <span v-else class="share-value">开启后显示</span>
      </div>
      <div class="assist-info-row">
        <span class="assist-label">组网地址</span>
        <span class="share-value" aria-label="本机组网地址">{{ localAddress || localAddressState }}</span>
      </div>
    </div>
    <div class="assist-actions">
      <button type="button" class="secondary icon-button" :disabled="!shareText" @click="copyAll"><Copy :size="15" />全部复制</button>
      <button type="button" class="tiny-button" @click="router.push('/app/sunshine')">配对确认 →</button>
    </div>
  </section>
  <form v-else class="setup-form assist-connect" @submit.prevent="connect">
    <FeedbackMessage v-if="device?.credentialState === 'invalid' || app.snapshot.session.error?.reauthenticate" message="保存的连接授权已失效，请更新连接密码。" kind="warning" />
    <label class="form-label" for="peerCode">对方访问码</label>
    <input id="peerCode" v-model="code" class="text-input" inputmode="numeric" maxlength="40" required placeholder="请输入对方访问码" autocomplete="off" />
    <label class="form-label" for="peerPassword">连接密码</label>
    <input id="peerPassword" v-model="password" class="text-input" type="password" :required="!device?.credentialRef" autocomplete="off" placeholder="对方的连接密码" maxlength="1024" />
    <label class="form-label" for="peerAddress">对方组网地址</label>
    <input id="peerAddress" v-model.trim="hostAddress" class="text-input" required maxlength="253" placeholder="填写对方分享的组网地址" autocomplete="off" spellcheck="false" />
    <FeedbackMessage :message="validation" kind="error" />
    <div class="page-actions start"><button class="primary icon-button" type="submit" :disabled="app.busy || app.loading">连接对方电脑<ArrowRight :size="17" /></button></div>
  </form>
</template>

<style scoped>
.assist-tabs { margin: 12px 0 16px; }
.assist-tabs button { padding: 8px 0; }
.assist-card { max-width: 570px; padding: 16px 18px; border: 1px solid #dde6ee; border-radius: 8px; background: #fff; }
.assist-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 14px; }
.assist-toolbar h2 { margin: 0; font-size: 15px; font-weight: 600; color: #405667; }
.assist-status { display: flex; flex-wrap: wrap; gap: 8px 18px; margin: 10px 0; font-size: 12px; color: #8292a0; }
.assist-status .good { color: #279968; }
.assist-passwords { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; padding: 10px 0 12px; }
.assist-passwords label { display: block; margin-bottom: 6px; font-size: 12px; color: #61788b; }
.assist-passwords .text-input { width: 100%; }
.assist-info { padding-top: 6px; border-top: 1px solid #e8edf1; }
.assist-info-row { display: grid; grid-template-columns: 68px minmax(0, 1fr) auto; align-items: center; gap: 8px; min-height: 38px; font-size: 13px; }
.assist-label { color: #718797; font-size: 12px; }
.share-value { color: #496781; font-family: Consolas, 'Cascadia Mono', monospace; overflow-wrap: anywhere; user-select: text; }
.share-password { width: 100%; height: 30px; font-family: Consolas, 'Cascadia Mono', monospace; }
.share-reveal { display: inline-flex; align-items: center; padding: 5px; }
.assist-actions { display: flex; align-items: center; gap: 14px; padding-top: 12px; }
.assist-actions .secondary { padding: 7px 12px; }
.assist-connect { margin-top: 12px; }
</style>
