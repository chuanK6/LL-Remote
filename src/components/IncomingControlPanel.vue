<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { Maximize2, Minimize2, Monitor, MonitorUp } from 'lucide-vue-next'
import { bridge } from '../bridge'
import { normalizeError } from '../domain'
import { useAppStore } from '../stores/app'
import type { IncomingControl } from '../types'
import FeedbackMessage from './FeedbackMessage.vue'

const props = defineProps<{ control: IncomingControl; compact?: boolean }>()
defineEmits<{ toggle: [] }>()
const app = useAppStore()
const clock = ref(Date.now())
const disconnecting = ref(false)
const error = ref('')
let timer: ReturnType<typeof setInterval> | undefined

const duration = computed(() => {
  const startedAt = props.control.startedAt && Date.parse(props.control.startedAt)
  const seconds = startedAt ? Math.max(0, Math.floor((clock.value - startedAt) / 1000)) : 0
  return [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60]
    .map(value => String(value).padStart(2, '0')).join(':')
})
const connectionLabel = computed(() => `当前有 ${props.control.count} 台设备正在控制此电脑`)
const controllerLabel = computed(() => props.control.deviceName || (props.control.count === 1 ? '控制端设备' : `${props.control.count} 台控制端设备`))

async function disconnect() {
  if (disconnecting.value) return
  disconnecting.value = true
  error.value = ''
  try {
    await bridge.disconnectIncomingControl()
  } catch (caught) {
    error.value = normalizeError(caught).message
  } finally {
    disconnecting.value = false
  }
}

onMounted(() => { if (!props.compact) timer = setInterval(() => { clock.value = Date.now() }, 1000) })
onUnmounted(() => clearInterval(timer))
</script>

<template>
  <button v-if="compact" type="button" class="controlled-card" aria-label="展开本机被控状态" @click="$emit('toggle')">
    <MonitorUp :size="24" aria-hidden="true" />
    <span><strong>本机被控中</strong><small>{{ connectionLabel }}</small></span>
    <Maximize2 :size="16" class="expand-icon" aria-hidden="true" />
  </button>
  <main v-else class="controlled-page" aria-labelledby="controlled-title">
    <header class="controlled-header">
      <div class="controlled-symbol"><MonitorUp :size="31" aria-hidden="true" /></div>
      <div class="controlled-heading">
        <h1 id="controlled-title">本机被控中</h1>
        <p>{{ connectionLabel }}</p>
      </div>
      <time class="controlled-timer" aria-label="本次被控时长">{{ duration }}</time>
      <button type="button" class="collapse-button" aria-label="缩小本机被控状态" title="缩小本机被控状态" @click="$emit('toggle')"><Minimize2 :size="19" /></button>
    </header>
    <section class="controlled-sessions" aria-label="远程控制连接">
      <div class="controlled-actions">
        <h2>远程控制连接</h2>
        <button type="button" class="secondary" :disabled="disconnecting" @click="disconnect">{{ disconnecting ? '正在断开…' : '断开所有远控' }}</button>
      </div>
      <FeedbackMessage v-if="error || app.error" :message="error || app.error?.message || ''" kind="error" />
      <div class="controlled-session">
        <Monitor :size="24" aria-hidden="true" />
        <div><strong>{{ controllerLabel }}</strong><small>连接时长 {{ duration }}</small></div>
        <button v-if="control.count === 1" type="button" class="small-button" :disabled="disconnecting" @click="disconnect">断开远控</button>
      </div>
    </section>
  </main>
</template>

<style scoped>
.controlled-page {
  grid-area: 2 / 1 / 3 / -1;
  min-width: 0;
  overflow: auto;
  padding: clamp(20px, 4vw, 52px);
  background: #f5f8fd;
}
.controlled-header {
  display: flex;
  align-items: center;
  gap: 20px;
  position: relative;
  padding: 28px 66px 28px 28px;
  border: 1px solid #c6d9f9;
  border-radius: 14px;
  color: #183452;
  background: linear-gradient(120deg, #e4efff, #f8fbff);
}
.controlled-symbol {
  display: grid;
  place-items: center;
  width: 58px;
  height: 58px;
  flex: none;
  border-radius: 12px;
  background: #286fdb;
  color: white;
}
.controlled-heading h1 { margin: 0; font-size: clamp(23px, 3vw, 32px); }
.controlled-heading p { margin: 6px 0 0; color: #49617d; font-size: 14px; }
.controlled-timer {
  margin-left: auto;
  color: #225ba5;
  font-size: clamp(23px, 3vw, 34px);
  font-weight: 650;
  font-variant-numeric: tabular-nums;
}
.collapse-button {
  position: absolute;
  top: 12px;
  right: 12px;
  display: grid;
  place-items: center;
  width: 34px;
  height: 34px;
  color: #225ba5;
  border-radius: 7px;
}
.collapse-button:hover { background: #dceaff; }
.controlled-sessions { margin-top: 24px; }
.controlled-actions, .controlled-session { display: flex; align-items: center; gap: 15px; }
.controlled-actions { justify-content: space-between; margin-bottom: 12px; }
.controlled-actions h2 { margin: 0; font-size: 17px; }
.controlled-session {
  padding: 16px 20px;
  border: 1px solid #dce6f1;
  border-radius: 10px;
  background: white;
  color: #214b77;
}
.controlled-session > div { min-width: 0; overflow-wrap: anywhere; }
.controlled-session small { display: block; margin-top: 5px; color: #637c90; }
.controlled-session .small-button { margin-left: auto; flex: none; }
.controlled-card {
  display: flex;
  align-items: center;
  flex: none;
  gap: 10px;
  margin: 8px 10px 12px;
  padding: 13px;
  border-radius: 10px;
  text-align: left;
  color: #fff;
  background: #245aaf;
}
.controlled-card:hover { background: #1d4d9a; }
.controlled-card > svg { flex: none; }
.controlled-card span { flex: 1; }
.controlled-card strong, .controlled-card small { display: block; }
.controlled-card small { margin-top: 4px; font-size: 11px; color: #e6efff; }
.expand-icon { align-self: flex-start; }
</style>
