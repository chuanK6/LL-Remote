<script setup lang="ts">
import { useRouter } from 'vue-router'
import { bridge } from '../bridge'
import { isConnecting, stageLabel } from '../domain'
import { useAppStore } from '../stores/app'
const app = useAppStore()
const router = useRouter()
async function cancel() { await app.run(async () => { app.snapshot.session = await bridge.cancel() }) }
async function disconnect() { if (app.snapshot.session.id) await app.run(async () => { app.snapshot.session = await bridge.disconnect(app.snapshot.session.id!) }) }
function updatePassword() { router.push({ path: '/assist', query: { mode: 'other', device: app.snapshot.session.deviceId || '' } }) }
</script>
<template>
  <div v-if="!['idle', 'canceled', 'disconnected', ''].includes(app.snapshot.session.stage)" :class="['connection-banner', { failed: ['failed', 'disconnectFailed'].includes(app.snapshot.session.stage) }]" role="status">
    <div>
      <span v-if="isConnecting(app.snapshot.session.stage) || app.snapshot.session.stage === 'disconnecting'" class="spinner" />
      <strong>{{ stageLabel(app.snapshot.session.stage) }}</strong>
      <p v-if="app.snapshot.session.error">{{ app.snapshot.session.error.message }}</p>
      <p v-else-if="app.snapshot.session.deviceName">{{ app.snapshot.session.deviceName }}<span v-if="app.snapshot.session.attempt > 1"> · 第 {{ app.snapshot.session.attempt }} 次尝试</span></p>
      <p v-if="app.snapshot.session.stage === 'pairing' && app.snapshot.session.pin" class="banner-pin">配对 PIN <strong>{{ app.snapshot.session.pin }}</strong></p>
      <p v-if="app.snapshot.session.localAcceptancePaused" class="role-paused">本机接受控制暂时暂停，远控结束后延迟检查并恢复。</p>
    </div>
    <div class="banner-actions">
      <button v-if="app.snapshot.session.stage === 'pairing'" class="small-button" @click="router.push('/pairing')">查看配对 PIN</button>
      <button v-if="isConnecting(app.snapshot.session.stage)" class="tiny-button" :disabled="app.loading" @click="cancel">取消</button>
      <template v-if="app.snapshot.session.stage === 'streaming'"><button class="small-button" @click="app.run(() => bridge.showSession())">会话设置</button><button class="tiny-button" @click="disconnect">断开</button></template>
      <button v-if="app.snapshot.session.stage === 'disconnectFailed'" class="tiny-button" :disabled="app.loading" @click="disconnect">重试断开</button>
      <button v-if="app.snapshot.session.error?.reauthenticate" class="small-button" @click="updatePassword">更新连接密码</button>
      <template v-if="['QUOTA_INSUFFICIENT', 'SERVICE_EXPIRED'].includes(app.snapshot.session.error?.code || '')"><button class="small-button" @click="bridge.openSignin()">前往签到</button><button class="tiny-button" @click="app.refresh">刷新时长</button></template>
      <button v-if="app.snapshot.session.stage === 'failed' && app.snapshot.session.error?.retryable && app.snapshot.session.deviceId" class="small-button" @click="app.connect({ deviceId: app.snapshot.session.deviceId! })">重试</button>
    </div>
  </div>
</template>
<style scoped>
.connection-banner .banner-pin { color: #2a6fc7; font-size: 18px; }
.banner-pin strong { margin-left: 10px; letter-spacing: 4px; font-variant-numeric: tabular-nums; font-weight: 600; }
.role-paused { margin: 0; font-size: 12px; color: #577b9e; }
</style>
