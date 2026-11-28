<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Search, RefreshCw, Star } from 'lucide-vue-next'
import { deviceName, sourceLabel } from '../domain'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
import SavedDeviceList from '../components/SavedDeviceList.vue'

const app = useAppStore()
const route = useRoute()
const router = useRouter()
const query = ref(typeof route.query.search === 'string' ? route.query.search : '')
const devices = computed(() => app.snapshot.devices
  .filter(device => device.favorite
    && `${deviceName(device)} ${device.note} ${device.peerCode || ''}`.toLowerCase().includes(query.value.trim().toLowerCase()))
  .sort((left, right) => (Date.parse(right.lastConnectedAt || '') || 0) - (Date.parse(left.lastConnectedAt || '') || 0)))
const cards = computed(() => devices.value.map(device => ({
  id: device.id, device, connectedAt: device.lastConnectedAt,
  method: device.accountOnline ? '同账号设备' : sourceLabel(device.source), peerCode: device.peerCode,
  to: { name: 'favorite-device', params: { id: device.id }, query: query.value ? { search: query.value } : {} },
})))
</script>

<template>
  <PageHeading title="收藏设备" description="已收藏的电脑，方便查看和再次连接。" />
  <div class="search-bar">
    <div class="search-input"><Search :size="17" /><input v-model="query" class="text-input" aria-label="搜索设备" placeholder="搜索名称、备注或识别码" /></div>
    <button class="secondary icon-button" :disabled="app.loading" @click="app.refresh"><RefreshCw :size="15" :class="{ spinning: app.loading }" />刷新</button>
  </div>
  <SavedDeviceList v-if="devices.length" :items="cards" delete-label="删除设备和全部连接记录" :disabled="app.loading || app.busy" @delete="app.deleteSavedDevice" />
  <div v-else class="empty-state">
    <Star :size="54" :stroke-width="1.1" />
    <h2>{{ query ? '没有找到匹配设备' : '还没有收藏设备' }}</h2>
    <p>在设备详情页点击名称右侧的五角星，即可收藏。</p>
    <div class="page-actions start">
      <button v-if="!app.snapshot.account.authenticated" class="secondary" @click="router.push('/login')">登录皎月连</button>
      <button class="primary" @click="router.push({ path: '/assist', query: { mode: 'other' } })">开始协助</button>
    </div>
  </div>
</template>
