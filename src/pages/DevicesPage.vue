<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Monitor, Search, RefreshCw } from 'lucide-vue-next'
import { deviceName, formatTime, hasActiveDeviceSession, sourceLabel, stageLabel, statusLabel } from '../domain'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
import SavedDeviceList from '../components/SavedDeviceList.vue'
const app = useAppStore()
const route = useRoute()
const router = useRouter()
const query = ref(typeof route.query.search === 'string' ? route.query.search : '')
const favorites = computed(() => route.name === 'favorites')
const devices = computed(() => {
  const filtered = (favorites.value ? app.snapshot.devices.filter(device => device.favorite) : app.onlineAccountDevices)
    .filter(device => `${deviceName(device)} ${device.note} ${device.peerCode || ''}`.toLowerCase().includes(query.value.trim().toLowerCase()))
  return favorites.value ? filtered.sort((left, right) => (Date.parse(right.lastConnectedAt || '') || 0) - (Date.parse(left.lastConnectedAt || '') || 0)) : filtered
})
const favoriteCards = computed(() => devices.value.map(device => ({
  id: device.id, device, connectedAt: device.lastConnectedAt,
  method: device.accountOnline ? '同账号设备' : sourceLabel(device.source), peerCode: device.peerCode,
  to: { name: 'favorite-device', params: { id: device.id }, query: query.value ? { search: query.value } : {} },
})))
</script>
<template><PageHeading :title="favorites ? '收藏设备' : '全部设备'" :description="favorites ? '已收藏的电脑，方便查看和再次连接。' : '仅显示当前账号的在线主机。'"/><div class="search-bar"><div class="search-input"><Search :size="17"/><input v-model="query" class="text-input" aria-label="搜索设备" placeholder="搜索名称、备注或识别码"/></div><button class="secondary icon-button" :disabled="app.loading" @click="app.refresh"><RefreshCw :size="15" :class="{ spinning: app.loading }"/>刷新</button></div>
  <SavedDeviceList v-if="favorites && devices.length" :items="favoriteCards" :disabled="app.loading || app.busy" @delete="app.deleteSavedDevice" />
  <table v-else-if="devices.length" class="device-table"><thead><tr><th>设备名称</th><th>状态</th><th>最近连接</th><th>操作</th></tr></thead><tbody><tr v-for="device in devices" :key="device.id"><td class="device-table-name">{{ deviceName(device) }}<small class="history-source">{{ device.accountOnline ? '同账号设备' : sourceLabel(device.source) }}<span v-if="device.peerCode"> · {{ device.peerCode }}</span></small><small v-if="device.note" class="history-source">{{ device.note }}</small></td><td><span :class="['table-status', { good: device.accountOnline || device.networkConnected || device.status === 'online' }]"><i/>{{ device.networkConnected ? '皎月连已连接' : hasActiveDeviceSession(device.id, app.snapshot.session) ? stageLabel(app.snapshot.session.stage) : device.accountOnline ? '在线' : statusLabel(device.status) }}</span></td><td>{{ formatTime(device.lastConnectedAt) }}</td><td><button class="tiny-button" @click="router.push(`/device/${device.id}`)">查看设备 →</button><button v-if="device.networkConnected || hasActiveDeviceSession(device.id, app.snapshot.session)" class="tiny-button" :disabled="app.loading" :aria-label="`断开 ${deviceName(device)} 的连接`" @click="app.disconnectDevice(device.id)">{{ app.disconnectingDeviceId === device.id ? '正在断开' : '断开连接' }}</button></td></tr></tbody></table>
  <div v-else class="empty-state"><Monitor :size="54" :stroke-width="1.1"/><h2>{{ query ? '没有找到匹配设备' : favorites ? '还没有收藏设备' : '暂无同账号在线设备' }}</h2><p>{{ favorites ? '在设备详情页点击名称右侧的五角星，即可收藏。' : '登录同一个皎月连账号的在线主机会显示在这里，连接过的电脑可在「连接记录」中查看。' }}</p><div class="page-actions start"><button v-if="!app.snapshot.account.authenticated" class="secondary" @click="router.push('/login')">登录皎月连</button><button class="primary" @click="router.push({ path: '/assist', query: { mode: 'other' } })">开始协助</button></div></div>
</template>
