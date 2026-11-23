<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { emptyMetrics, type Metrics } from '../types'

const metrics = ref(emptyMetrics())
const rows = [
  { key: 'fps', label: '帧率', unit: 'FPS' },
  { key: 'bitrateMbps', label: '实际码率', unit: 'Mbps' },
  { key: 'networkMs', label: '网络延迟', unit: 'ms' },
  { key: 'decodeMs', label: '解码延迟', unit: 'ms' },
  { key: 'renderMs', label: '渲染延迟', unit: 'ms' },
  { key: 'droppedPercent', label: '丢帧', unit: '%' },
] as const
let stop: UnlistenFn | undefined
let active = true

function format(value: number | null, unit: string) {
  if (value === null) return '—'
  return value.toFixed(1) + (unit === '%' ? '%' : ` ${unit}`)
}

onMounted(async () => {
  let received = false
  const unlisten = await listen<Metrics>('ll:performance', ({ payload }) => {
    received = true
    metrics.value = payload
  })
  if (!active) {
    unlisten()
    return
  }
  stop = unlisten
  const initial = await invoke<Metrics>('get_session_performance').catch(() => null)
  if (initial && active && !received) metrics.value = initial
})
onBeforeUnmount(() => { active = false; stop?.() })
</script>

<template>
  <section class="performance" aria-label="远控性能监测">
    <h2>远控性能</h2>
    <dl>
      <template v-for="row in rows" :key="row.key">
        <dt>{{ row.label }}</dt>
        <dd>{{ format(metrics[row.key], row.unit) }}</dd>
      </template>
    </dl>
  </section>
</template>

<style scoped>
.performance {
  height: 100%;
  padding: 8px 12px;
  overflow: hidden;
  border: 1px solid #ffffff20;
  border-radius: 8px;
  background: #42424255;
  color: #ffffffb3;
  text-shadow: 0 1px 2px #0005;
}
h2 { margin: 0 0 5px; font-size: 13px; font-weight: 650; }
dl {
  display: grid;
  grid-template-columns: 1fr max-content;
  gap: 2px 12px;
  margin: 0;
  font-size: 11px;
  line-height: 16px;
}
dt { color: #ffffff99; }
dd { margin: 0; text-align: right; font-variant-numeric: tabular-nums; }
</style>
