<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Monitor, AudioLines, Activity, MoreVertical, ArrowRight, Plus, Gamepad2, MonitorUp, ChevronDown } from 'lucide-vue-next'
import { bridge } from '../bridge'
import { deviceName, formatTime, hasActiveDeviceSession, sourceLabel, stageLabel, statusLabel } from '../domain'
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
const device = computed(() => app.snapshot.devices.find(d => d.id === route.params.id))
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
  if (app.error?.reauthenticate || app.error?.code === 'ADDRESS_REQUIRED') await router.push({ path: '/assist', query: { mode: 'other', device: deviceId } })
  if (app.error?.code === 'LOGIN_REQUIRED') await router.push('/login')
}
async function associate() { if (device.value && association.value) await app.run(async () => { app.apply(await bridge.associateHost(device.value!.id, association.value)); app.tell('设备关联已保存。') }) }
</script>
<template>
  <template v-if="device"><div class="device-header"><span :class="['online-pill', { offline: !streaming && !networkConnected && device.status !== 'online' }]"><i/>{{ statusLabel(streaming || networkConnected ? 'online' : device.status) }}</span><h1>{{ deviceName(device) }}</h1><button class="more" aria-label="编辑设备" @click="editing = true"><MoreVertical :size="24"/></button></div>
    <div v-if="!streaming" class="device-source-line"><span>{{ sourceLabel(device.source) }}<template v-if="device.peerCode"> · {{ device.peerCode }}</template><template v-if="device.lastConnectedAt"> · 最近连接 {{ formatTime(device.lastConnectedAt) }}</template></span><button @click="router.push(`/history/${device.id}`)">连接记录 →</button></div>
    <div v-if="streaming" class="connection-banner active-control" aria-label="设备连接状态">
      <div class="active-status" role="status"><MonitorUp :size="21" aria-hidden="true" /><strong>被控中</strong><span>本机正在控制此电脑</span></div>
      <div class="active-actions">
        <button class="secondary" :disabled="app.loading" @click="app.disconnectDevice(device.id)">断开所有远控</button>
        <details class="active-options">
          <summary aria-label="更多远控操作" title="更多远控操作"><ChevronDown :size="17" /></summary>
          <div>
            <button :disabled="app.loading" @click="app.run(() => bridge.showSession())">会话设置</button>
            <button @click="router.push(`/history/${device.id}`)">连接记录</button>
          </div>
        </details>
      </div>
    </div>
    <div v-else-if="networkConnected || session" class="connection-banner" role="status" aria-label="设备连接状态">
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
      <button v-else class="cover" type="button" :aria-label="`进入 ${deviceName(device)} 的桌面`" :disabled="app.busy || app.loading" @click="connect()"><DeviceWallpaper :path="device.wallpaper"/><span class="enter">{{ session ? '连接进行中' : '进入桌面' }}<ArrowRight :size="29"/></span></button>
      <div class="tools"><button class="tool" @click="router.push('/app/moonlight')"><Monitor class="icon"/>画质设置</button><button class="tool" @click="router.push({ path: '/app/moonlight', query: { section: 'input' } })"><AudioLines class="icon"/>输入与声音</button><button class="tool" @click="router.push({ path: '/diagnostics', query: { device: device.id } })"><Activity class="icon"/>连接诊断</button></div>
    </section>
    <h2 class="quick-title">快速启动</h2><section class="quick-area"><button v-for="name in quickApps" :key="name" class="quick-card" :disabled="app.busy || app.loading" @click="connect(name)"><span class="quick-icon"><Monitor v-if="name === 'Desktop'" :size="39"/><Gamepad2 v-else :size="39"/></span>{{ name }}</button><button class="quick-card" @click="router.push(`/quick-apps/${device.id}`)"><span class="add-circle"><Plus :size="27"/></span>添加</button></section>
    <FeedbackMessage v-if="device.source === 'code' && device.status === 'unknown'" message="这是已保存的连接设备。历史记录不代表当前在线；连接时会检查对方状态。"/>
    <section v-if="device.source === 'existing' && !device.natId" class="page-section"><h2>关联皎月连主机</h2><p class="setting-detail">已有配对保留在 Moonlight 中，关联后可以自动准备组网。</p><div class="association-row"><select v-model="association" aria-label="关联主机"><option value="">选择同一台设备</option><option v-for="host in associationCandidates" :key="host.id" :value="host.natId || ''">{{ deviceName(host) }}</option></select><button class="small-button" :disabled="!association" @click="associate">保存关联</button></div></section>
    <div class="host-info"><i v-if="app.snapshot.preferences.preventSleep"/><span>{{ app.snapshot.preferences.preventSleep ? '防休眠已开启' : '防休眠已关闭' }} · {{ app.snapshot.preferences.autostart ? '开机自启动已开启' : '开机自启动已关闭' }}</span></div>
    <DeviceEditor v-if="editing" :device="device" @close="editing = false"/>
  </template><template v-else><PageHeading title="设备不存在" description="设备已移除或不属于当前账号。"/><button class="primary" @click="router.push('/devices')">查看全部设备</button></template>
</template>
<style scoped>
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
