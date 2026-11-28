<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, History, Search } from 'lucide-vue-next'
import { deviceName, sourceLabel } from '../domain'
import { useAppStore } from '../stores/app'
import type { Device, HistoryEntry } from '../types'
import PageHeading from '../components/PageHeading.vue'
import SavedDeviceList from '../components/SavedDeviceList.vue'

type RecordCard = HistoryEntry & { device: Device; savedOnly: boolean }
const app = useAppStore()
const route = useRoute()
const router = useRouter()
const query = ref(typeof route.query.search === 'string' ? route.query.search : '')
const visibleCount = ref(60)
const filterId = computed(() => typeof route.query.device === 'string' ? route.query.device : null)
const filteredDevice = computed(() => app.snapshot.devices.find(device => device.id === filterId.value))

function recordName(entry: RecordCard): string {
  return entry.device ? deviceName(entry.device) : entry.peerCode ? `识别码 ${entry.peerCode}` : '历史设备'
}

const records = computed(() => {
  const devices = new Map(app.snapshot.devices.map(device => [device.id, device]))
  const recordedIds = new Set(app.snapshot.history.map(entry => entry.deviceId))
  const entries: RecordCard[] = app.snapshot.history.flatMap(entry => {
    const device = entry.deviceId ? devices.get(entry.deviceId) : null
    return device ? [{ ...entry, device, savedOnly: false }] : []
  })
  // Keep older saved connections even when they have no detailed history/time.
  for (const device of app.snapshot.devices) {
    if (recordedIds.has(device.id) || device.historyFallbackHidden || (!device.lastConnectedAt && device.source === 'account')) continue
    entries.push({
      id: `saved-${device.id}`, deviceId: device.id, device,
      peerCode: device.peerCode, connectedAt: device.lastConnectedAt || '',
      method: sourceLabel(device.source), result: 'saved', errorCode: null, savedOnly: true,
    })
  }
  const search = query.value.trim().toLowerCase()
  return entries
    .filter(entry => (!filterId.value || entry.deviceId === filterId.value)
      && `${recordName(entry)} ${entry.device?.note || ''} ${entry.peerCode || ''}`.toLowerCase().includes(search))
    .sort((left, right) => (Date.parse(right.connectedAt) || 0) - (Date.parse(left.connectedAt) || 0))
})
const visibleRecords = computed(() => records.value.slice(0, visibleCount.value))
const cards = computed(() => visibleRecords.value.map(entry => ({ ...entry, to: detailRoute(entry) })))
watch([query, filterId], () => { visibleCount.value = 60 })

function detailRoute(entry: RecordCard) {
  return {
    name: entry.savedOnly ? 'saved-history-device' : 'history-device',
    params: { id: entry.savedOnly ? entry.deviceId! : entry.id },
    query: { ...(filterId.value ? { device: filterId.value } : {}), ...(query.value ? { search: query.value } : {}) },
  }
}
</script>

<template>
  <div class="history-heading">
    <PageHeading title="连接记录" description="按连接时间排列，选择一条记录查看设备详情。" />
    <span v-if="records.length" class="record-count">{{ records.length }} 条记录</span>
  </div>
  <div class="history-toolbar">
    <div class="search-input history-search">
      <Search :size="17" aria-hidden="true" />
      <input v-model="query" class="text-input" aria-label="搜索连接记录" placeholder="搜索设备名称、备注或识别码" />
    </div>
    <button v-if="filterId" class="secondary icon-button" @click="router.push('/history')"><ArrowLeft :size="16" />全部连接记录</button>
  </div>
  <p v-if="filteredDevice" class="history-filter">{{ deviceName(filteredDevice) }} 的连接记录</p>
  <SavedDeviceList v-if="records.length" :items="cards" delete-label="仅删除这条连接记录" :disabled="app.loading" @delete="app.deleteHistoryEntry" />
  <div v-else class="empty-state">
    <History :size="54" :stroke-width="1.1" aria-hidden="true" />
    <h2>{{ query ? '没有找到匹配记录' : '还没有连接记录' }}</h2>
    <p>{{ query ? '试试其他设备名称、备注或识别码。' : '连接过的电脑会保存在这里，方便查看和再次连接。' }}</p>
    <div v-if="!query" class="page-actions"><button class="primary" @click="router.push({ path: '/assist', query: { mode: 'other' } })">开始协助</button></div>
  </div>
  <div v-if="visibleRecords.length < records.length" class="history-more"><button class="secondary" @click="visibleCount += 60">显示更多记录</button></div>
</template>

<style scoped>
.history-heading { position: relative; padding-right: 105px; }
.record-count { position: absolute; right: 0; top: 13px; color: var(--muted); font-size: 13px; }
.history-toolbar { display: flex; align-items: center; gap: 12px; margin: 26px 0 23px; }
.history-search { flex: 1; max-width: 460px; }
.history-search input { width: 100%; height: 42px; background: #fff; border-radius: 8px; }
.history-filter { color: #64778a; font-size: 13px; margin: 0 0 17px; }
.history-more { display: flex; justify-content: center; margin-top: 25px; }
@media (max-width: 700px) { .history-toolbar { flex-wrap: wrap; } .history-search { max-width: none; flex-basis: 100%; } }
</style>
