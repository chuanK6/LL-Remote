<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Monitor, AudioLines, Activity, MoreVertical, ArrowLeft, ArrowRight, Plus, Gamepad2, MonitorUp, ChevronDown, Star } from 'lucide-vue-next'
import { bridge } from '../bridge'
import { deviceName, formatTime, hasActiveDeviceSession, onlineDeviceName, sourceLabel, stageLabel, statusLabel } from '../domain'
import { useAppStore } from '../stores/app'
import DeviceWallpaper from '../components/DeviceWallpaper.vue'
import DeviceEditor from '../components/DeviceEditor.vue'
import FeedbackMessage from '../components/FeedbackMessage.vue'
import PageHeading from '../components/PageHeading.vue'
const app = useAppStore()
const route = useRoute()
const router = useRouter()
const editing = ref(false)
const association = ref('')
const favoriteDetail = computed(() => route.name === 'favorite-device')
const historyDetail = computed(() => route.name === 'history-device' || route.name === 'saved-history-device' || favoriteDetail.value)
const record = computed(() => route.name === 'history-device' ? app.snapshot.history.find(entry => entry.id === route.params.id) : null)
const device = computed(() => {
  const id = route.name === 'history-device' ? record.value?.deviceId : route.params.id
  return app.snapshot.devices.find(device => device.id === id)
})
const recordedAt = computed(() => record.value?.connectedAt || device.value?.lastConnectedAt)
const heading = computed(() => {
  const selected = device.value
  if (!selected) return ''
  return !historyDetail.value && selected.accountOnline ? onlineDeviceName(selected) : deviceName(selected)
})
const session = computed(() => device.value && hasActiveDeviceSession(device.value.id, app.snapshot.session) ? app.snapshot.session : null)
const streaming = computed(() => session.value?.stage === 'streaming')
const networkConnected = computed(() => device.value?.networkConnected === true)
const associationCandidates = computed(() => app.snapshot.devices.filter(d => d.natId && d.id !== device.value?.id))
const quickApps = computed(() => device.value?.quickApps.filter(name => device.value!.apps.some(a => a.name === name)) || [])
watch(device, value => { if (value) app.selectedDeviceId = value.id }, { immediate: true })
async function connect(application = 'Desktop') {
  const deviceId = device.value?.id
  if (!deviceId) return
  await app.connect({ deviceId, app: application })
  if (device.value?.source === 'direct') {
    if (app.error?.code === 'ADDRESS_REQUIRED') await router.push({ name: 'direct', query: { device: deviceId } })
    return
  }
  if (app.error?.reauthenticate || app.error?.code === 'ADDRESS_REQUIRED') await router.push({ path: '/assist', query: { mode: 'other', device: deviceId } })
  if (app.error?.code === 'LOGIN_REQUIRED') await router.push('/login')
}
async function associate() { if (device.value && association.value) await app.run(async () => { app.apply(await bridge.associateHost(device.value!.id, association.value)); app.tell('设备关联已保存。') }) }
async function toggleFavorite() {
  const selected = device.value
  if (!selected) return
  await app.run(async () => {
    app.apply(await bridge.editDevice({
      deviceId: selected.id, displayName: deviceName(selected), favorite: !selected.favorite,
      note: selected.note, wallpaperPath: null, resetWallpaper: false,
    }))
    app.tell(selected.favorite ? '已取消收藏。' : '已收藏设备。')
  })
}
function backToHistory() {
  return router.push({ path: favoriteDetail.value ? '/favorites' : '/history', query: {
    ...(!favoriteDetail.value && typeof route.query.device === 'string' ? { device: route.query.device } : {}),
    ...(typeof route.query.search === 'string' ? { search: route.query.search } : {}),
  } })
}
function showHistory() { if (device.value) return router.push({ path: '/history', query: { device: device.value.id } }) }
</script>
<template>
  <button v-if="historyDetail" class="history-return" @click="backToHistory"><ArrowLeft :size="17" aria-hidden="true" />{{ favoriteDetail ? '返回收藏设备' : '返回连接记录' }}</button>
  <template v-if="device"><div class="device-header">
    <span v-if="!historyDetail" :class="['online-pill', { offline: !streaming && !networkConnected && !device.accountOnline && device.status !== 'online' }]"><i/>{{ statusLabel(streaming || networkConnected || device.accountOnline ? 'online' : device.status) }}</span>
    <h1>{{ heading }}</h1>
    <button class="favorite-button" :class="{ favorited: device.favorite }" :aria-label="device.favorite ? '取消收藏' : '收藏设备'" :title="device.favorite ? '取消收藏' : '收藏设备'" :aria-pressed="device.favorite" :disabled="app.loading" @click="toggleFavorite"><Star :size="23" :fill="device.favorite ? 'currentColor' : 'none'" :stroke-width="1.7" aria-hidden="true" /></button>
    <button class="more" aria-label="编辑设备" @click="editing = true"><MoreVertical :size="24"/></button></div>
    <div v-if="historyDetail || !streaming" class="device-source-line">
      <span v-if="historyDetail">{{ favoriteDetail ? '收藏设备' : record?.method || '连接记录' }}<template v-if="recordedAt"> · {{ formatTime(recordedAt) }}</template><template v-if="device.peerCode"> · 识别码 {{ device.peerCode }}</template></span>
      <span v-else>{{ device.accountOnline ? '同账号设备' : sourceLabel(device.source) }}<template v-if="device.peerCode"> · {{ device.peerCode }}</template><template v-if="device.lastConnectedAt"> · 最近连接 {{ formatTime(device.lastConnectedAt) }}</template></span>
      <button v-if="device.source === 'direct' && !session" @click="router.push({ name: 'direct', query: { device: device.id } })">更换直连地址</button>
      <button v-if="!historyDetail" @click="showHistory">连接记录 →</button>
    </div>
    <p v-if="!historyDetail && device.accountOnline && device.note" class="setting-detail device-note">{{ device.note }}</p>
    <div v-if="historyDetail && (session || networkConnected)" class="record-session-actions">
      <button v-if="streaming" class="secondary" :disabled="app.loading" @click="app.run(() => bridge.showSession())">会话设置</button>
      <button class="secondary" :disabled="app.loading" @click="app.disconnectDevice(device.id)">断开连接</button>
    </div>
    <div v-if="streaming && !historyDetail" class="connection-banner active-control" aria-label="设备连接状态">
      <div class="active-status" role="status"><MonitorUp :size="21" aria-hidden="true" /><strong>被控中</strong><span>本机正在控制此电脑</span></div>
      <div class="active-actions">
        <button class="secondary" :disabled="app.loading" @click="app.disconnectDevice(device.id)">断开所有远控</button>
        <details class="active-options">
          <summary aria-label="更多远控操作" title="更多远控操作"><ChevronDown :size="17" /></summary>
          <div>
            <button :disabled="app.loading" @click="app.run(() => bridge.showSession())">会话设置</button>
            <button @click="showHistory">连接记录</button>
          </div>
        </details>
      </div>
    </div>
    <div v-else-if="!historyDetail && (networkConnected || session)" class="connection-banner" role="status" aria-label="设备连接状态">
      <div><strong>{{ networkConnected ? '皎月连已连接' : stageLabel(session?.stage || 'idle') }}</strong><p v-if="networkConnected && session">{{ stageLabel(session.stage) }}</p></div>
      <div class="banner-actions">
        <button v-if="session?.stage === 'pairing'" class="small-button" @click="router.push('/pairing')">查看配对 PIN</button>
        <button class="secondary" :disabled="app.loading" @click="app.disconnectDevice(device.id)">{{ app.disconnectingDeviceId === device.id ? '正在断开' : '断开连接' }}</button>
      </div>
    </div>
    <section class="desktop-card">
      <div v-if="streaming" class="cover active-cover">
        <DeviceWallpaper :path="device.wallpaper" />
        <div class="active-cover-content">
          <MonitorUp :size="64" :stroke-width="1.5" aria-hidden="true" />
          <p>已建立远程连接</p>
          <button class="primary" :disabled="app.loading" @click="app.run(() => bridge.showRemoteWindow())">显示远控窗口</button>
        </div>
      </div>
      <button v-else class="cover" type="button" :aria-label="`进入 ${heading} 的桌面`" :disabled="app.busy || app.loading" @click="connect()"><DeviceWallpaper :path="device.wallpaper"/><span class="enter">{{ session ? '连接进行中' : '进入桌面' }}<ArrowRight :size="29"/></span></button>
      <div class="tools"><button class="tool" @click="router.push('/app/moonlight')"><Monitor class="icon"/>画质设置</button><button class="tool" @click="router.push({ path: '/app/moonlight', query: { section: 'input' } })"><AudioLines class="icon"/>输入与声音</button><button class="tool" @click="router.push({ path: '/diagnostics', query: { device: device.id } })"><Activity class="icon"/>连接诊断</button></div>
    </section>
    <h2 class="quick-title">快速启动</h2><section class="quick-area"><button v-for="name in quickApps" :key="name" class="quick-card" :disabled="app.busy || app.loading" @click="connect(name)"><span class="quick-icon"><Monitor v-if="name === 'Desktop'" :size="39"/><Gamepad2 v-else :size="39"/></span>{{ name }}</button><button class="quick-card" @click="router.push(`/quick-apps/${device.id}`)"><span class="add-circle"><Plus :size="27"/></span>添加</button></section>
    <FeedbackMessage v-if="!historyDetail && device.source === 'code' && device.status === 'unknown'" message="这是已保存的连接设备。连接时会检查对方状态。"/>
    <section v-if="device.source === 'existing' && !device.natId" class="page-section"><h2>关联皎月连主机</h2><p class="setting-detail">已有配对保留在 Moonlight 中，关联后可以自动准备组网。</p><div class="association-row"><select v-model="association" aria-label="关联主机"><option value="">选择同一台设备</option><option v-for="host in associationCandidates" :key="host.id" :value="host.natId || ''">{{ deviceName(host) }}</option></select><button class="small-button" :disabled="!association" @click="associate">保存关联</button></div></section>
    <div class="host-info"><i v-if="app.snapshot.preferences.preventSleep"/><span>{{ app.snapshot.preferences.preventSleep ? '防休眠已开启' : '防休眠已关闭' }} · {{ app.snapshot.preferences.autostart ? '开机自启动已开启' : '开机自启动已关闭' }}</span></div>
    <DeviceEditor v-if="editing" :device="device" @close="editing = false"/>
  </template><template v-else>
    <PageHeading v-if="historyDetail" :title="record?.peerCode ? `识别码 ${record.peerCode}` : '连接记录详情'" description="此记录的设备资料已移除。连接记录仍保留，可返回列表查看其他记录。"/>
    <PageHeading v-else title="设备不存在" description="设备已移除或不属于当前账号。"/>
    <p v-if="historyDetail && record" class="setting-detail">{{ record.method }} · {{ formatTime(record.connectedAt) }}</p>
    <button v-if="!historyDetail" class="primary" @click="router.push('/assist')">开始协助</button>
  </template>
</template>
<style scoped>
.device-note { overflow-wrap: anywhere; }
.device-header { min-width: 0; height: auto; min-height: 38px; gap: 12px; }
.device-header h1 { min-width: 0; overflow-wrap: anywhere; }
.device-header .more { margin-left: auto; flex-shrink: 0; }
.favorite-button { display: grid; place-items: center; flex: none; width: 38px; height: 38px; padding: 0; border: 1px solid #dce5ee; border-radius: 9px; color: #8996a4; background: #fff; transition: color 150ms, background 150ms, border-color 150ms; }
.favorite-button:hover { color: #c38b12; background: #fff8e8; border-color: #f0d591; }
.favorite-button.favorited { color: #e8ad26; background: #fff8e5; border-color: #f1dba7; }
.history-return { display: inline-flex; align-items: center; gap: 8px; padding: 7px 0; margin-bottom: 22px; color: #5680ab; font-size: 13px; }
.history-return:hover { color: var(--blue); }
.record-session-actions { display: flex; justify-content: end; gap: 9px; margin-bottom: 18px; }
.active-control {
  color: #1b579d;
  background: #e6f0ff;
  border-color: #d1e3ff;
  border-left: 3px solid #3785ff;
}
.active-status, .active-actions { display: flex; align-items: center; gap: 10px; }
.active-status strong { font-weight: 650; white-space: nowrap; }
.active-status span { padding-left: 10px; border-left: 1px solid #bed7fa; }
.active-actions { flex: none; }
.active-actions > .secondary { white-space: nowrap; }
.active-options { position: relative; }
.active-options summary { display: grid; place-items: center; width: 30px; height: 30px; cursor: pointer; list-style: none; }
.active-options summary::-webkit-details-marker { display: none; }
.active-options[open] summary { background: #d8e8ff; border-radius: 5px; }
.active-options > div {
  position: absolute;
  z-index: 2;
  right: 0;
  top: calc(100% + 5px);
  display: grid;
  min-width: 115px;
  padding: 4px;
  border: 1px solid #d1e3ff;
  border-radius: 6px;
  background: #fff;
  box-shadow: 0 4px 14px #17334d22;
}
.active-options button { padding: 7px; text-align: left; border-radius: 4px; }
.active-options button:hover { background: #e6f0ff; }
.active-cover { display: grid; align-items: end; justify-items: start; cursor: default; }
.active-cover::after, .active-cover:hover::after {
  inset: 0;
  background: linear-gradient(90deg, #0d203eec, #0d203e40 82%);
}
.active-cover :deep(.cover-art), .active-cover:hover :deep(.cover-art) { filter: none; transform: none; }
.active-cover-content {
  position: relative;
  z-index: 1;
  display: grid;
  justify-items: start;
  gap: 12px;
  padding: 28px 32px;
  color: #fff;
}
.active-cover-content p { margin: 0 0 5px; font-size: 19px; font-weight: 600; }
</style>
