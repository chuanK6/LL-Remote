<script setup lang="ts">
import { ref } from 'vue'
import { bridge, native } from '../bridge'
import { defaultShortcuts, formatShortcut, readShortcut, shortcutActions, type ShortcutAction, type Shortcuts } from '../shortcuts'
import { useAppStore } from '../stores/app'
import SettingRow from './SettingRow.vue'
import FeedbackMessage from './FeedbackMessage.vue'

const app = useAppStore()
const recording = ref<ShortcutAction | null>(null)
const hint = ref('')
const items: Record<ShortcutAction, { label: string; detail: string }> = {
  mouse: { label: '鼠标切换', detail: '捕获或释放远控鼠标。' },
  disconnect: { label: '断开连接', detail: '结束当前远控连接。' },
  display: { label: '显示器常亮', detail: '开关本机的「保持显示器常亮」。' },
}

async function save(shortcuts: Shortcuts) {
  const snapshot = await app.run(() => bridge.saveShortcuts(shortcuts))
  if (snapshot) { app.apply(snapshot); app.tell('快捷键已保存。') }
}

function record(action: ShortcutAction) {
  recording.value = action
  hint.value = ''
}

async function capture(event: KeyboardEvent, action: ShortcutAction) {
  if (recording.value !== action) return
  if (event.key === 'Tab') { recording.value = null; return }
  event.preventDefault()
  event.stopPropagation()
  if (event.repeat || ['Control', 'Alt', 'Shift'].includes(event.key)) return
  if (event.key === 'Escape') {
    recording.value = null
    hint.value = ''
    return
  }
  const shortcut = readShortcut(event)
  if (!shortcut) { hint.value = '请按 Ctrl、Alt 或 Shift，再搭配字母、数字或 F1–F12。'; return }
  const selected = app.snapshot.preferences.shortcuts
  if (shortcutActions.some(other => other !== action && formatShortcut(selected[other]) === formatShortcut(shortcut))) {
    hint.value = '这个组合已用于其他动作。'
    return
  }
  recording.value = null
  hint.value = ''
  await save({ ...selected, [action]: shortcut })
}
</script>

<template>
  <section class="page-section">
    <h2>快捷键</h2>
    <SettingRow v-for="action in shortcutActions" :key="action" :label="items[action].label" :detail="items[action].detail">
      <button type="button" class="secondary shortcut-button" :disabled="!native || app.loading"
        :aria-label="`更改${items[action].label}快捷键`" @click="record(action)" @blur="recording === action && (recording = null)" @keydown="capture($event, action)">
        {{ recording === action ? '请按新的组合键 · Esc 取消' : formatShortcut(app.snapshot.preferences.shortcuts[action]) }}
      </button>
    </SettingRow>
    <FeedbackMessage v-if="hint" kind="warning" :message="hint" />
    <div class="setting-row"><span class="setting-detail">在 LL远程主界面或远控窗口获得焦点时使用。</span><button type="button" class="secondary" :disabled="!native || app.loading" @click="save(defaultShortcuts())">恢复默认</button></div>
  </section>
</template>

<style scoped>
.shortcut-button { white-space: nowrap; }
</style>
