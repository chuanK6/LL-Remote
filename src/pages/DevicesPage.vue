<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Monitor, Search, RefreshCw } from 'lucide-vue-next'
import { deviceName, formatTime, hasActiveDeviceSession, sourceLabel, stageLabel, statusLabel } from '../domain'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
const app = useAppStore()
const route = useRoute()
const router = useRouter()
const query = ref('')
const favorites = computed(() => route.name === 'favorites')
const devices = computed(() => app.snapshot.devices.filter(d => (!favorites.value || d.favorite) && `${deviceName(d)} ${d.note} ${d.peerCode || ''}`.toLowerCase().includes(query.value.trim().toLowerCase())))
</script>
<template><PageHeading :title="favorites ? '收藏设备' : '全部设备'" description="查看同账号主机、识别码连接历史和已有连接。"/><div class="search-bar"><div class="search-input"><Search :size="17"/><input v-model="query" class="text-input" aria-label="搜索设备" placeholder="搜索名称、备注或识别码"/></div><button class="secondary icon-button" :disabled="app.loading" @click="app.refresh"><RefreshCw :size="15" :class="{ spinning: app.loading }"/>刷新</button></div>
  <table v-if="devices.length" class="device-table"><thead><tr><th>设备名称</th><th>状态</th><th>最近连接</th><th>操作</th></tr></thead><tbody><tr v-for="device in devices" :key="device.id"><td class="device-table-name">{{ deviceName(device) }}<small class="history-source">{{ sourceLabel(device.source) }}<span v-if="device.peerCode"> · {{ device.peerCode }}</span></small><small v-if="device.note" class="history-source">{{ device.note }}</small></td><td><span :class="['table-status', { good: device.networkConnected || device.status === 'online' }]"><i/>{{ device.networkConnected ? '皎月连已连接' : hasActiveDeviceSession(device.id, app.snapshot.session) ? stageLabel(app.snapshot.session.stage) : statusLabel(device.status) }}</span></td><td>{{ formatTime(device.lastConnectedAt) }}</td><td><button class="tiny-button" @click="router.push(`/device/${device.id}`)">查看设备 →</button><button v-if="device.networkConnected || hasActiveDeviceSession(device.id, app.snapshot.session)" class="tiny-button" :disabled="app.loading" :aria-label="`断开 ${deviceName(device)} 的连接`" @click="app.disconnectDevice(device.id)">{{ app.disconnectingDeviceId === device.id ? '正在断开' : '断开连接' }}</button></td></tr></tbody></table>
  <div v-else class="empty-state"><Monitor :size="54" :stroke-width="1.1"/><h2>{{ query ? '没有找到匹配设备' : favorites ? '还没有收藏设备' : '连接你的第一台电脑' }}</h2><p>{{ favorites ? '在设备名称旁的三个点中添加收藏。' : '通过皎月连账号查看设备，或输入识别码开始远程协助。' }}</p><div class="page-actions start"><button v-if="!app.snapshot.account.authenticated" class="secondary" @click="router.push('/login')">登录皎月连</button><button class="primary" @click="router.push({ path: '/assist', query: { mode: 'other' } })">开始协助</button></div></div>
</template>
