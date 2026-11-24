<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { X } from 'lucide-vue-next'
import { bridge, native } from '../bridge'
import { normalizeError } from '../domain'
import { defaultStream, emptySession, type SessionPatch, type WindowMode, type MouseMode } from '../types'
import FeedbackMessage from '../components/FeedbackMessage.vue'
import StreamQuality from '../components/StreamQuality.vue'
import ToggleSwitch from '../components/ToggleSwitch.vue'
const session = ref(emptySession())
const draft = ref(defaultStream())
const expanded = ref(false)
type Edge = 'left' | 'right' | 'top' | 'bottom'
const dockedEdge = ref<Edge>('right')
const dragging = ref(false)
const applying = ref(false)
const layingOut = ref(false)
const disconnecting = ref(false)
const performanceEnabled = ref(false)
const savingPerformance = ref(false)
const error = ref('')
let polling: ReturnType<typeof setInterval> | undefined
let stopListening: (() => void) | undefined
let stopFocusListener: (() => void) | undefined
let pointerOrigin: { id: number; x: number; y: number } | undefined
let moving: Promise<void> | undefined
let suppressClick = false
async function layout(open: boolean) {
  if (layingOut.value || open === expanded.value) return
  layingOut.value = true
  if (open) error.value = ''
  try {
    if (native) performanceEnabled.value = await invoke<boolean>('layout_session_controls', { expanded: open })
    expanded.value = open
  } catch (caught) {
    error.value = normalizeError(caught).message
  } finally {
    layingOut.value = false
  }
}
async function setPerformanceEnabled(enabled: boolean) {
  const sessionId = session.value.id
  if (!sessionId || savingPerformance.value) return
  savingPerformance.value = true
  error.value = ''
  try {
    await invoke('set_session_performance_enabled', { sessionId, enabled })
    performanceEnabled.value = enabled
  } catch (caught) {
    error.value = normalizeError(caught).message
  } finally {
    savingPerformance.value = false
  }
}
function openPanel(event: MouseEvent) {
  if (!suppressClick || event.detail === 0) void layout(true)
}
function startDrag(event: PointerEvent) {
  if (event.button !== 0 || !event.isPrimary) return
  suppressClick = false
  pointerOrigin = { id: event.pointerId, x: event.screenX, y: event.screenY }
  const button = event.currentTarget as HTMLElement
  button.setPointerCapture(event.pointerId)
}
function drag(event: PointerEvent) {
  if (!pointerOrigin || !native || event.pointerId !== pointerOrigin.id) return
  if (!dragging.value && Math.hypot(event.screenX - pointerOrigin.x, event.screenY - pointerOrigin.y) < 5) return
  dragging.value = suppressClick = true
  if (!moving) {
    moving = invoke<void>('drag_session_controls', { finished: false })
      .catch(() => {})
      .finally(() => { moving = undefined })
  }
}
async function finishDrag(event: PointerEvent) {
  if (event.pointerId !== pointerOrigin?.id) return
  pointerOrigin = undefined
  if (!dragging.value) return
  await moving
  try {
    dockedEdge.value = await invoke<Edge>('drag_session_controls', { finished: true })
  } catch (caught) {
    error.value = normalizeError(caught).message
  } finally {
    dragging.value = false
  }
}
async function apply(patch: SessionPatch) {
  if (!session.value.id || applying.value) return
  applying.value = true; error.value = ''
  try {
    session.value = await bridge.updateSession(session.value.id, patch)
    if (session.value.preferences) draft.value = { ...session.value.preferences }
  } catch (caught) {
    error.value = normalizeError(caught).message
  } finally {
    applying.value = false
  }
}
async function disconnect() {
  if (!session.value.id || disconnecting.value) return
  disconnecting.value = true
  error.value = ''
  try {
    session.value = await bridge.disconnect(session.value.id)
  } catch (caught) {
    error.value = normalizeError(caught).message
  } finally {
    disconnecting.value = false
  }
}
function audio() { return apply({ kind: 'audio', volume: draft.value.volume, muted: draft.value.muted, device: draft.value.audioDevice, hostAudio: draft.value.hostAudio }) }
onMounted(async () => {
  document.documentElement.classList.add('session-window')
  if (!native) return
  session.value = await bridge.session()
  if (session.value.preferences) draft.value = { ...session.value.preferences }
  stopListening = await bridge.listenSession(value => { session.value = value })
  stopFocusListener = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (!focused) void layout(false)
  })
  polling = setInterval(async () => {
    if (!applying.value && !disconnecting.value) session.value = await bridge.session().catch(() => session.value)
  }, 1000)
})
onBeforeUnmount(() => { clearInterval(polling); stopListening?.(); stopFocusListener?.() })
</script>
<template>
  <div class="session-surface" @click.self="layout(false)" @keydown.esc.prevent="layout(false)">
    <button v-if="!expanded" class="session-ball" :data-edge="dragging ? undefined : dockedEdge" aria-label="打开会话悬浮球设置"
      :title="error || '会话设置 · Ctrl+Alt+Shift+Z 释放游戏鼠标'" :disabled="layingOut"
      @click="openPanel" @pointerdown="startDrag" @pointermove="drag"
      @pointerup="finishDrag" @pointercancel="finishDrag" @lostpointercapture="finishDrag"><span>LL</span></button>
    <section v-else class="session-settings" aria-label="本次远控设置">
      <header class="float-head">
        <h2 :title="session.deviceName || undefined">{{ session.deviceName || '本次连接' }}</h2>
        <button class="plain-icon" aria-label="收起悬浮面板" @click="layout(false)"><X :size="18" /></button>
      </header>
      <button class="disconnect-button" :disabled="disconnecting || !session.id" @click="disconnect">{{ disconnecting ? '正在断开…' : '断开本次连接' }}</button>
      <FeedbackMessage :message="error" kind="error" />
      <div class="performance-setting">
        <span>性能监测</span>
        <ToggleSwitch label="性能监测" :model-value="performanceEnabled" :disabled="!session.id || savingPerformance"
          @update:model-value="setPerformanceEnabled" />
      </div>
      <section class="session-group">
        <h3>画质</h3>
        <StreamQuality v-model="draft" live :disabled="applying" @apply="apply" />
        <p class="session-hint">更改画质会短暂重连。</p>
      </section>
      <section class="session-group">
        <h3>窗口与鼠标</h3>
        <div class="segments" role="group" aria-label="窗口模式">
          <button v-for="(name, mode) in { fullscreen: '全屏', windowed: '窗口', borderless: '无边框' }" :key="mode"
            :class="{ active: session.preferences?.windowMode === mode }" :aria-pressed="session.preferences?.windowMode === mode"
            :disabled="applying" @click="apply({ kind: 'window', mode: mode as WindowMode })">{{ name }}</button>
        </div>
        <div class="segments" role="group" aria-label="鼠标模式">
          <button v-for="(name, mode) in { game: '游戏鼠标', office: '办公鼠标' }" :key="mode"
            :class="{ active: session.preferences?.mouseMode === mode }" :aria-pressed="session.preferences?.mouseMode === mode"
            :disabled="applying" @click="apply({ kind: 'mouse', mode: mode as MouseMode })">{{ name }}</button>
        </div>
      </section>
      <section class="session-group">
        <h3>音频</h3>
        <div class="float-volume">
          <label for="sessionVolume">音量</label>
          <input id="sessionVolume" v-model.number="draft.volume" type="range" min="0" max="100" :disabled="applying" @change="audio" />
          <span>{{ draft.volume }}%</span>
          <button class="small-button" :disabled="applying" @click="draft.muted = !draft.muted; audio()">{{ draft.muted ? '取消静音' : '静音' }}</button>
        </div>
      </section>
    </section>
  </div>
</template>

<style scoped>
.session-ball {
  inset: 0;
  width: 100%;
  height: 100%;
  padding: 0;
  overflow: hidden;
  border: 0;
  border-radius: 0;
  color: #292929cf;
  background: transparent;
  box-shadow: none;
  font-size: 9px;
  letter-spacing: -.8px;
}
.session-ball::before {
  content: '';
  position: absolute;
  inset: 0;
  border: 1px solid #ffffff75;
  border-radius: 50%;
  background: #ffffff73;
  transition: background 160ms ease;
}
.session-ball:hover::before, .session-ball:focus-visible::before { background: #ffffffb3; }
.session-ball span { position: relative; }
.session-ball[data-edge='left']::before { left: -100%; }
.session-ball[data-edge='right']::before { right: -100%; }
.session-ball[data-edge='top']::before { top: -100%; }
.session-ball[data-edge='bottom']::before { bottom: -100%; }
.session-settings {
  inset: 4px;
  padding: 13px 14px;
}
.session-settings .float-head { align-items: center; gap: 10px; }
.session-settings .float-head h2 { min-width: 0; margin: 0; }
.session-settings .float-head button { flex-shrink: 0; }
.performance-setting {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 12px;
  padding-bottom: 10px;
  border-bottom: 1px solid #dce5ee;
  font-size: 13px;
  font-weight: 600;
}
.session-group + .session-group { margin-top: 13px; padding-top: 12px; border-top: 1px solid #dce5ee; }
.session-group h3 {
  margin: 0 0 6px;
  color: var(--ink);
  font-size: 13px;
  font-weight: 600;
}
.session-group .segments + .segments { margin-top: 6px; }
.session-group .float-volume { gap: 5px; }
.session-group .float-volume button { padding-inline: 7px; }
.disconnect-button { margin-bottom: 12px; }
</style>
