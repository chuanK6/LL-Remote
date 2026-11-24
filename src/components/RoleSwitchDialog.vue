<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { bridge } from '../bridge'
import { normalizeError } from '../domain'
import { ROLE_SWITCH_DESCRIPTION } from '../role-switch'
import { useAppStore } from '../stores/app'
import FeedbackMessage from './FeedbackMessage.vue'

const app = useAppStore()
const dialog = ref<HTMLDialogElement>()
const enableButton = ref<HTMLButtonElement>()
const pending = ref(false)
const error = ref('')

watch(() => [app.snapshot.session.id, app.snapshot.session.stage], async () => {
  await nextTick()
  if (app.snapshot.session.stage === 'awaitingRoleSwitch') {
    error.value = ''
    if (!dialog.value?.open) dialog.value?.showModal()
    enableButton.value?.focus()
  } else {
    dialog.value?.close()
  }
}, { immediate: true, flush: 'post' })

async function decide(enabled: boolean) {
  const id = app.snapshot.session.id
  if (!id || pending.value) return
  pending.value = true
  error.value = ''
  try {
    await bridge.roleSwitchDecision(id, enabled)
    dialog.value?.close()
  } catch (caught) {
    error.value = normalizeError(caught).message
  } finally {
    pending.value = false
  }
}
</script>

<template>
  <dialog ref="dialog" class="role-switch-dialog" aria-labelledby="roleSwitchTitle" @cancel.prevent="decide(false)">
    <h2 id="roleSwitchTitle">是否开启自动切换角色？</h2>
    <p>本机与被控制端的组网地址相同，需要暂时关闭本机接受控制后继续连接。</p>
    <p>{{ ROLE_SWITCH_DESCRIPTION }}</p>
    <FeedbackMessage :message="error" kind="error" />
    <div class="role-switch-actions">
      <button ref="enableButton" class="enable-role" :disabled="pending" @click="decide(true)">开启功能</button>
      <button class="stop-role" :disabled="pending" @click="decide(false)">停止连接</button>
    </div>
  </dialog>
</template>

<style scoped>
.role-switch-dialog { width: min(460px, calc(100vw - 48px)); padding: 26px; border: 1px solid #d7e4ef; border-radius: 12px; background: #fff; color: #314b61; box-shadow: 0 12px 48px #17334d30; }
.role-switch-dialog::backdrop { background: #152c4566; }
h2 { margin: 0 0 14px; font-size: 20px; font-weight: 600; }
p { margin: 10px 0; font-size: 14px; line-height: 1.7; }
.role-switch-actions { display: flex; justify-content: flex-end; gap: 12px; margin-top: 22px; }
.role-switch-actions button { padding: 10px 18px; border: 1px solid; border-radius: 6px; font-size: 14px; }
.enable-role { background: #e6f2ff; border-color: #c4dcf5 !important; color: #286aa9; }
.enable-role:hover { background: #d7eaff; }
.stop-role { background: #fff0f0; border-color: #f0cece !important; color: #aa5353; }
.stop-role:hover { background: #ffe2e2; }
</style>
