<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ChevronUp, History, Monitor, PanelsTopLeft, ScanLine, Settings, Star, Trash2 } from 'lucide-vue-next'
import { bridge } from '../bridge'
import logo from '../../src-tauri/icons/128x128.png'
import { deviceName } from '../domain'
import { useAppStore } from '../stores/app'
import type { Device } from '../types'

defineProps<{ hidden?: boolean }>()
const app = useAppStore()
const route = useRoute()
const router = useRouter()
const open = reactive({ devices: true, assistance: true, apps: true })
const devices = computed(() => [...app.onlineAccountDevices].sort((a, b) => Number(b.favorite) - Number(a.favorite)))
const deleteDialog = ref<HTMLDialogElement>()
const deleting = ref<Device>()
const apps = [
  { id: 'jiaoyue', icon: '皎', label: '皎月连' },
  { id: 'moonlight', icon: 'M', label: 'Moonlight' },
  { id: 'sunshine', icon: 'S', label: 'Sunshine' },
]

function askToDelete(device: Device) {
  deleting.value = device
  deleteDialog.value?.showModal()
}

async function confirmDelete() {
  if (!deleting.value) return
  const id = deleting.value.id
  deleteDialog.value?.close()
  const snapshot = await app.run(() => bridge.deleteDevice(id))
  if (!snapshot) return
  app.apply(snapshot)
  if (route.params.id === id) await router.replace('/devices')
  app.tell('已删除设备。')
}
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
          :title="deviceName(device) + ' · 同账号在线设备'"
          @click="router.push(`/device/${device.id}`)">
          <span class="status-dot" aria-hidden="true" />
          <span class="device-icon" aria-hidden="true"><i/><i/><i/><i/></span>
          <span class="device-nav-name">{{ deviceName(device) }}</span>
          <Monitor v-if="app.snapshot.session.stage === 'streaming' && app.snapshot.session.deviceId === device.id" class="active-stream" :size="14" aria-label="控制中" />
          </button>
          <button type="button" class="device-delete" :aria-label="'删除 ' + deviceName(device)" title="删除设备"
            :disabled="app.loading || app.busy" @click="askToDelete(device)">
            <Trash2 :size="15" aria-hidden="true" />
          </button>
        </div>
        <p v-if="!devices.length" class="nav-empty">{{ app.snapshot.account.authenticated ? '暂无同账号在线设备' : '登录后查看同账号在线设备' }}</p>
      </div>
      <button :class="['nav-button', { selected: route.name === 'devices' }]" @click="router.push('/devices')">
        <span class="nav-symbol all-symbol" aria-hidden="true"><i/><i/><i/><i/></span>全部设备
      </button>
    </div>
    <button class="section-title next-section section-toggle" :aria-expanded="open.assistance" aria-controls="nav-assistance" @click="open.assistance = !open.assistance">
      <ScanLine class="icon" />远程协助<ChevronUp class="chevron-icon" />
    </button>
    <div v-show="open.assistance" id="nav-assistance">
      <button :class="['nav-button', { selected: route.name === 'assist' }]" @click="router.push('/assist')"><span class="nav-symbol"><Monitor class="icon"/></span>开始协助</button>
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
  <dialog ref="deleteDialog" class="device-delete-dialog" aria-labelledby="delete-device-title">
    <h2 id="delete-device-title">删除设备</h2>
    <p v-if="deleting">将「{{ deviceName(deleting) }}」从本机列表移除。同账号设备在线时，刷新会重新显示。也可以通过访问码重新连接。</p>
    <div class="page-actions">
      <button type="button" class="secondary" autofocus @click="deleteDialog?.close()">取消</button>
      <button type="button" class="device-delete-confirm" @click="confirmDelete">删除</button>
    </div>
  </dialog>
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
  padding-right: 39px;
  gap: 12px;
  font-size: 15px;
  color: #596674;
}
.device-navigation .device-button::before { display: none; }
.device-navigation .device-button.selected { background: #e2e7ea; }
.device-navigation .device-button.online { color: var(--ink); }
.device-navigation .device-icon { flex: 0 0 auto; background: #a9adb4; }
.device-navigation .online .device-icon { background: var(--blue); }
.device-navigation .status-dot { left: 15px; height: 6px; width: 6px; visibility: hidden; }
.device-navigation .online .status-dot { visibility: visible; background: #32c75b; }
.device-delete {
  position: absolute;
  top: 7px;
  right: 8px;
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 5px;
  color: #6c7581;
  opacity: 0;
}
.device-nav-row:hover .device-delete, .device-nav-row:focus-within .device-delete { opacity: 1; }
.device-delete:hover { background: #f9e6e8; color: #a22d3b; }
@media (hover: none) { .device-delete { opacity: 1; } }
.device-delete-dialog {
  width: min(390px, calc(100vw - 32px));
  padding: 22px;
  border: 1px solid #d7e4ef;
  border-radius: 10px;
  color: var(--ink);
}
.device-delete-dialog::backdrop { background: #152c4566; }
.device-delete-dialog h2 { margin: 0; font-size: 18px; }
.device-delete-dialog p { color: #536272; line-height: 1.65; font-size: 14px; overflow-wrap: anywhere; }
.device-delete-confirm { padding: 9px 15px; border-radius: 6px; font-size: 13px; color: #fff; background: #a22d3b; }
.device-delete-confirm:hover { background: #85222f; }
</style>
