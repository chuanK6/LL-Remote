<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { bridge, native } from '../bridge'
import { localVpnAddressState } from '../local-share'
import { validateStream } from '../domain'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
import SettingRow from '../components/SettingRow.vue'
import StreamQuality from '../components/StreamQuality.vue'
import ToggleSwitch from '../components/ToggleSwitch.vue'
import FeedbackMessage from '../components/FeedbackMessage.vue'
const app = useAppStore()
const route = useRoute()
const router = useRouter()
const application = computed(() => String(route.params.app))
const localAddress = computed(() => app.snapshot.account.authenticated ? app.snapshot.account.localVpnAddress : null)
const localAddressState = computed(() => localVpnAddressState(app.snapshot.account.authenticated, app.snapshot.account.serverRunning || app.snapshot.devices.some(d => d.networkConnected)))
const stream = ref({ ...(app.selectedDevice?.preferences || app.snapshot.preferences.stream) })
const sunshine = ref({ ...app.snapshot.sunshine.config })
const pin = ref('')
const pairingId = ref(app.snapshot.sunshine.pendingPairings[0]?.id || '')
const pairingName = ref('roth')
const feedback = ref('')
const validation = ref('')
let streamSource = JSON.stringify(stream.value)
let sunshineSource = JSON.stringify(sunshine.value)
let streamTarget = app.selectedDeviceId
let streamPending = 0
let sunshinePending = 0
watch(() => app.selectedDevice?.preferences || app.snapshot.preferences.stream, value => {
  if (streamTarget !== app.selectedDeviceId || (!streamPending && JSON.stringify(stream.value) === streamSource)) {
    stream.value = { ...value }
  }
  streamTarget = app.selectedDeviceId
  streamSource = JSON.stringify(value)
}, { deep: true })
watch(() => app.snapshot.sunshine.config, value => {
  if (!sunshinePending && JSON.stringify(sunshine.value) === sunshineSource) sunshine.value = { ...value }
  sunshineSource = JSON.stringify(value)
}, { deep: true })
watch(() => app.snapshot.sunshine.pendingPairings, pairings => {
  if (!pairings.some(pairing => pairing.id === pairingId.value)) pairingId.value = pairings[0]?.id || ''
})
watch(pairingId, () => { pairingName.value = 'roth' })
async function advanced() { await app.run(async () => { const message = await bridge.openAdvanced(application.value, application.value === 'moonlight' ? app.selectedDeviceId : null); if (message) app.tell(message) }); await app.refreshBackground() }
async function copyLocalAddress() {
  if (!localAddress.value) return
  try { await navigator.clipboard.writeText(localAddress.value); app.tell('本机组网地址已复制。') }
  catch { app.tell('无法访问剪贴板，请手动复制组网地址。') }
}
async function saveStream() {
  validation.value = ''; feedback.value = ''
  const preferences = { ...stream.value }
  const error = validateStream(preferences)
  if (error) { validation.value = error; return }
  const deviceId = app.selectedDeviceId
  streamPending++
  try {
    await app.saveSetting(() => bridge.saveStream(deviceId, preferences))
  } finally {
    streamPending--
    if (!streamPending && app.selectedDeviceId === deviceId) {
      stream.value = { ...(app.selectedDevice?.preferences || app.snapshot.preferences.stream) }
    }
  }
}
async function saveSunshine() {
  const settings = { ...sunshine.value }
  sunshinePending++
  try {
    await app.saveSetting(() => bridge.sunshineSettings(settings, false))
  } finally {
    sunshinePending--
    if (!sunshinePending) sunshine.value = { ...app.snapshot.sunshine.config }
  }
}
function saveAutoRetry(autoRetry: boolean) {
  return app.saveSetting(() => bridge.savePreferences({ autoRetry }))
}
async function control(action: string) { await app.run(async () => { app.apply(await bridge.sunshineControl(action)) }) }
async function confirmPin() {
  if (!/^\d{4}$/.test(pin.value)) { validation.value = 'PIN 须为四位数字。'; return }
  if (!pairingName.value.trim()) { validation.value = '请输入配对设备名称。'; return }
  validation.value = ''
  const selectedId = pairingId.value || app.snapshot.sunshine.pendingPairings[0]?.id || ''
  const result = await app.run(() => bridge.sunshinePair(pin.value, pairingName.value.trim(), selectedId))
  if (result) { pin.value = ''; pairingName.value = 'roth'; app.apply(result); feedback.value = '配对已确认。' }
}
onMounted(async () => {
  if (route.query.section === 'input') { await nextTick(); document.getElementById('inputSection')?.scrollIntoView({ behavior: 'smooth', block: 'start' }) }
})
onBeforeUnmount(() => { pin.value = '' })
</script>
<template><div class="advanced-row"><span>完整的应用设置请前往</span><button class="advanced-button" :disabled="app.loading" @click="advanced">高级设置</button></div>
  <template v-if="application === 'moonlight'"><PageHeading title="Moonlight · 常用设置" :description="app.selectedDevice ? `为 ${app.selectedDevice.displayName} 保存画质、窗口、输入和声音偏好。` : '设置新设备使用的默认画质、窗口、输入和声音。'"/>
    <div class="settings-groups">
    <section class="page-section"><h2>画面与串流</h2><StreamQuality :key="app.selectedDeviceId ?? 'default'" v-model="stream" @commit="saveStream"/><SettingRow label="视频编码"><select v-model="stream.codec" @change="saveStream" aria-label="视频编码"><option value="auto">自动选择</option><option value="h264">H.264</option><option value="hevc">HEVC</option><option value="av1">AV1</option></select></SettingRow><SettingRow label="窗口模式"><select v-model="stream.windowMode" @change="saveStream" aria-label="窗口模式"><option value="fullscreen">全屏</option><option value="windowed">窗口</option><option value="borderless">无边框</option></select></SettingRow></section>
    <section id="inputSection" class="page-section"><h2>鼠标、键盘与手柄</h2><SettingRow label="鼠标控制" detail="游戏鼠标使用相对坐标，办公鼠标使用绝对坐标。"><select v-model="stream.mouseMode" @change="saveStream" aria-label="鼠标控制"><option value="game">游戏鼠标</option><option value="office">办公鼠标</option></select></SettingRow><SettingRow label="系统快捷键"><select v-model="stream.captureKeys" @change="saveStream" aria-label="系统快捷键"><option value="fullscreen">仅全屏捕获</option><option value="always">始终捕获</option><option value="never">不捕获</option></select></SettingRow><SettingRow label="手柄输入"><ToggleSwitch :model-value="stream.gamepad" @update:model-value="stream.gamepad = $event; saveStream()" label="手柄输入"/></SettingRow></section>
    <section class="page-section"><h2>声音与输出</h2><SettingRow label="音频声道"><select v-model="stream.audioConfig" @change="saveStream" aria-label="音频声道"><option value="stereo">立体声</option><option value="5.1">5.1 环绕声</option><option value="7.1">7.1 环绕声</option></select></SettingRow><SettingRow label="主机同时播放声音"><ToggleSwitch :model-value="stream.hostAudio" @update:model-value="stream.hostAudio = $event; saveStream()" label="主机同时播放声音"/></SettingRow><SettingRow label="本机输出设备" detail="留空使用系统默认设备；其他设备须使用 Moonlight 识别的名称。"><input v-model.lazy="stream.audioDevice" @change="saveStream" @keydown.enter.prevent="($event.currentTarget as HTMLInputElement).blur()" class="text-input" aria-label="音频输出设备" placeholder="系统默认设备"/></SettingRow></section>
    </div>
    <FeedbackMessage v-if="app.snapshot.session.stage === 'streaming'" message="保存设备偏好不会中断当前会话。本次连接的即时调整请使用会话悬浮球。"/>
  </template>
  <template v-else-if="application === 'sunshine'"><PageHeading title="Sunshine · 常用设置" description="管理本机采集服务、管理账号与新设备配对。"/>
    <div class="settings-groups">
    <section class="page-section"><h2>管理账号与服务</h2><SettingRow label="管理账号"><span class="value-text">{{ app.snapshot.preferences.sunshineUsername || (app.snapshot.sunshine.accountInitialized ? '已有账号，待验证' : '尚未设置') }}</span><button class="tiny-button" @click="router.push('/setup')">设置 / 验证账号</button></SettingRow><SettingRow label="服务状态"><span :class="['value-text', { good: app.snapshot.sunshine.ready }]">{{ app.snapshot.sunshine.ready ? '已就绪' : app.snapshot.sunshine.running ? '正在准备' : '未启动' }}</span><button class="small-button" :disabled="app.loading" @click="control(app.snapshot.sunshine.running ? 'stop' : 'start')">{{ app.snapshot.sunshine.running ? '停止' : '启动' }}</button><button v-if="app.snapshot.sunshine.running" class="tiny-button" :disabled="app.loading" @click="control('restart')">重启</button></SettingRow><SettingRow label="配置界面语言"><span class="value-text">简体中文</span></SettingRow><SettingRow v-if="app.snapshot.sunshine.external" label="运行方式"><span class="value-text">复用已有 Windows 服务</span></SettingRow></section>
    <section class="page-section"><h2>配对确认</h2><SettingRow label="待配对设备"><select v-if="app.snapshot.sunshine.pendingPairings.length" v-model="pairingId" aria-label="待配对设备"><option v-for="pairing in app.snapshot.sunshine.pendingPairings" :key="pairing.id" :value="pairing.id">{{ pairing.name || '新设备' }}</option></select><span v-else class="value-text">{{ app.snapshot.sunshine.credentialValid ? '暂无配对请求' : '验证管理账号后读取' }}</span></SettingRow><SettingRow label="四位 PIN" detail="输入主控端显示的 PIN，授权该设备进入桌面。"><input v-model="pin" class="text-input small" inputmode="numeric" maxlength="4" aria-label="确认四位 PIN" placeholder="四位 PIN"/><button class="small-button" :disabled="app.loading || !app.snapshot.sunshine.credentialValid" @click="confirmPin">确认</button></SettingRow><SettingRow label="配对设备名称"><input v-model="pairingName" class="text-input" maxlength="128" required aria-label="配对设备名称"/></SettingRow></section>
    <section class="page-section"><h2>采集与音频</h2><SettingRow label="采集显示器" detail="留空使用默认显示器，复杂采集配置请进入高级设置。"><input v-model.lazy="sunshine.outputName" :disabled="!native" @change="saveSunshine" @keydown.enter.prevent="($event.currentTarget as HTMLInputElement).blur()" class="text-input" aria-label="采集显示器" placeholder="默认显示器"/></SettingRow><SettingRow label="音频设备"><input v-model.lazy="sunshine.audioSink" :disabled="!native" @change="saveSunshine" @keydown.enter.prevent="($event.currentTarget as HTMLInputElement).blur()" class="text-input" aria-label="Sunshine 音频设备" placeholder="系统默认设备"/></SettingRow><SettingRow label="显卡适配器"><input v-model.lazy="sunshine.adapterName" :disabled="!native" @change="saveSunshine" @keydown.enter.prevent="($event.currentTarget as HTMLInputElement).blur()" class="text-input" aria-label="显卡适配器" placeholder="自动选择"/></SettingRow></section>
    <section class="page-section"><h2>远程应用</h2><SettingRow label="已发布应用"><span class="value-text">{{ app.snapshot.sunshine.apps.map(a => a.name).join(' · ') || '尚未读取' }}</span></SettingRow></section>
    </div>
    <FeedbackMessage message="采集配置修改后可能需要重启服务；重启会影响正在接受的控制。" kind="warning"/>
  </template>
  <template v-else-if="application === 'jiaoyue'"><PageHeading title="皎月连 · 常用设置" description="使用皎月连账号与组网连接。"/>
    <div class="settings-groups">
    <section class="page-section"><h2>账号</h2><SettingRow label="账号状态"><span :class="['value-text', { good: app.snapshot.account.authenticated }]">{{ app.snapshot.account.authenticated ? '已登录' : '未登录' }}</span><button class="tiny-button" @click="router.push(app.snapshot.account.authenticated ? '/account' : '/login')">{{ app.snapshot.account.authenticated ? '查看账号' : '登录账号' }}</button></SettingRow><SettingRow label="本机名称"><span class="value-text">{{ app.snapshot.account.localName || '登录后读取' }}</span></SettingRow></section>
    <section class="page-section"><h2>组网信息</h2><SettingRow label="主要连接方式"><span class="value-text">组网模式</span></SettingRow><SettingRow label="网络抖动重试" detail="最多尝试三次，密码或授权失败立即停止。"><ToggleSwitch :model-value="app.snapshot.preferences.autoRetry" :disabled="app.loading" @update:model-value="saveAutoRetry" label="网络抖动重试"/></SettingRow><SettingRow label="本机识别码"><span class="value-text">{{ app.snapshot.account.authenticated ? app.snapshot.account.localCode || '尚未读取' : '登录后读取' }}</span></SettingRow><SettingRow label="本机组网地址" detail="组网建立后自动读取，可供对方关联本机。"><span class="value-text" aria-label="本机组网地址">{{ localAddress || localAddressState }}</span><button type="button" class="tiny-button" :disabled="!localAddress" @click="copyLocalAddress">复制地址</button></SettingRow></section><section class="page-section"><h2>远程协助</h2><SettingRow label="本机接受控制"><span :class="['value-text', { good: app.snapshot.account.serverRunning }]">{{ app.snapshot.account.serverRunning ? '服务已开启' : '服务未开启' }}</span><button class="tiny-button" @click="router.push('/assist')">准备本机 →</button></SettingRow><SettingRow label="连接其他电脑"><button class="tiny-button" @click="router.push({ path: '/assist', query: { mode: 'other' } })">识别码、密码与组网地址连接 →</button></SettingRow></section>
    </div>
  </template><PageHeading v-else title="应用不存在"/>
  <FeedbackMessage :message="validation" kind="error"/><FeedbackMessage :message="feedback" kind="success"/>
</template>
