<script setup lang="ts">
import { computed, reactive } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ChevronUp, History, Monitor, Network, PanelsTopLeft, ScanLine, Settings, Star } from 'lucide-vue-next'
import logo from '../../src-tauri/icons/128x128.png'
import { onlineDeviceName } from '../domain'
import { useAppStore } from '../stores/app'

defineProps<{ hidden?: boolean }>()
const app = useAppStore()
const route = useRoute()
const router = useRouter()
const open = reactive({ devices: true, assistance: true, apps: true })
const devices = computed(() => [...app.onlineAccountDevices].sort((a, b) => Number(b.favorite) - Number(a.favorite)))
const apps = [
  { id: 'jiaoyue', icon: '皎', label: '皎月连' },
  { id: 'moonlight', icon: 'M', label: 'Moonlight' },
  { id: 'sunshine', icon: 'S', label: 'Sunshine' },
]

</script>

<template>
  <aside v-show="!hidden" class="sidebar" aria-label="主导航">
    <div class="sidebar-brand"><img :src="logo" alt="" width="28" height="28" /><span>LL远程</span></div>
    <div class="sidebar-scroll">
    <button class="section-title section-toggle" :aria-expanded="open.devices" aria-controls="nav-devices" @click="open.devices = !open.devices">
      <PanelsTopLeft class="icon" />我的设备<ChevronUp class="chevron-icon" />
    </button>
    <div v-show="open.devices" id="nav-devices">
      <div class="device-navigation">
        <div v-for="device in devices" :key="device.id" class="device-nav-row">
          <button
          :class="['device-button', { selected: route.name === 'device' && route.params.id === device.id, online: device.accountOnline }]"
          :aria-current="route.name === 'device' && route.params.id === device.id ? 'page' : undefined"
          :title="device.note ? `${onlineDeviceName(device)} · ${device.note}` : `${onlineDeviceName(device)} · 同账号在线设备`"
          @click="router.push(`/device/${device.id}`)">
          <span class="status-dot" aria-hidden="true" />
          <span class="device-icon" aria-hidden="true"><i/><i/><i/><i/></span>
          <span class="device-nav-label">
            <span class="device-nav-name">{{ onlineDeviceName(device) }}</span>
            <span v-if="device.note" class="device-nav-note">{{ device.note }}</span>
          </span>
          <Monitor v-if="app.snapshot.session.stage === 'streaming' && app.snapshot.session.deviceId === device.id" class="active-stream" :size="14" aria-label="控制中" />
          </button>
        </div>
        <p v-if="!devices.length" class="nav-empty">{{ app.snapshot.account.authenticated ? '暂无同账号在线设备' : '登录后查看同账号在线设备' }}</p>
      </div>
    </div>
    <button class="section-title next-section section-toggle" :aria-expanded="open.assistance" aria-controls="nav-assistance" @click="open.assistance = !open.assistance">
      <ScanLine class="icon" />远程协助<ChevronUp class="chevron-icon" />
    </button>
    <div v-show="open.assistance" id="nav-assistance">
      <button :class="['nav-button', { selected: route.name === 'assist' }]" @click="router.push('/assist')"><span class="nav-symbol"><Monitor class="icon"/></span>开始协助</button>
      <button :class="['nav-button', { selected: route.name === 'direct' }]" @click="router.push('/direct')"><span class="nav-symbol"><Network class="icon"/></span>IP 直连</button>
      <button :class="['nav-button', { selected: route.name === 'history' || route.name === 'history-device' || route.name === 'saved-history-device' }]" @click="router.push('/history')"><span class="nav-symbol"><History class="icon"/></span>连接记录</button>
      <button :class="['nav-button', { selected: route.name === 'favorites' || route.name === 'favorite-device' }]" @click="router.push('/favorites')"><span class="nav-symbol"><Star class="icon filled"/></span>收藏设备</button>
    </div>
    <button class="section-title app-section section-toggle" :aria-expanded="open.apps" aria-controls="nav-apps" @click="open.apps = !open.apps">
      应用功能<ChevronUp class="chevron-icon" />
    </button>
    <div v-show="open.apps" id="nav-apps">
      <button v-for="engine in apps" :key="engine.id"
        :class="['nav-button', 'app-nav', { selected: route.name === 'apps' && route.params.app === engine.id }]"
        @click="router.push('/app/' + engine.id)">
        <span :class="['app-tag', { moon: engine.id === 'moonlight', sun: engine.id === 'sunshine' }]" aria-hidden="true">{{ engine.icon }}</span>{{ engine.label }}
      </button>
    </div>
    </div>
    <slot />
    <div class="footer-nav"><button :class="{ selected: route.name === 'settings' }" @click="router.push('/settings')"><Settings class="icon"/>设置</button></div>
  </aside>
</template>

<style scoped>
.sidebar { grid-area: 1 / 1 / -1 / 2; padding-top: 0; }
.sidebar-brand {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 57px;
  flex-shrink: 0;
  padding: 0 19px;
  font-size: 16px;
  font-weight: 600;
}
.sidebar-brand img { display: block; flex-shrink: 0; }
.sidebar-scroll { flex: 1; min-height: 0; overflow-y: auto; scrollbar-width: thin; }
.active-stream { color: var(--blue); flex: none; }
.section-toggle {
  width: 100%;
  text-align: left;
  flex-shrink: 0;
}
.section-toggle:hover { background: #e7edf2; }
.section-toggle[aria-expanded="false"] .chevron-icon { transform: rotate(180deg); }
.device-nav-row { position: relative; }
.device-navigation .device-button {
  height: 40px;
  padding-left: 34px;
  padding-right: 14px;
  gap: 12px;
  font-size: 15px;
  color: #596674;
}
.device-navigation .device-button::before { display: none; }
.device-navigation .device-button.selected { background: #e2e7ea; }
.device-navigation .device-button.online { color: var(--ink); }
.device-nav-label { display: grid; flex: 1; min-width: 0; gap: 2px; }
.device-nav-name, .device-nav-note { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.device-nav-note { font-size: 11px; color: #586b7a; }
.device-navigation .device-icon { flex: 0 0 auto; background: #a9adb4; }
.device-navigation .online .device-icon { background: var(--blue); }
.device-navigation .status-dot { left: 15px; height: 6px; width: 6px; visibility: hidden; }
.device-navigation .online .status-dot { visibility: visible; background: #32c75b; }
</style>
