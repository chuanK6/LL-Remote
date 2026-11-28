<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { bridge, native } from '../bridge'
import { formatTime } from '../domain'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
import SettingRow from '../components/SettingRow.vue'
const app = useAppStore()
const router = useRouter()
const confirmingLogout = ref(false)
async function logout() { const result = await app.run(() => bridge.logout()); if (result) { app.apply(result); await router.push('/login') } }
async function refreshAccount() {
  await app.refresh()
  if (native) await app.refreshSigninStatus()
}
</script>
<template>
  <PageHeading title="皎月连账号" description="查看当前账号与剩余服务时长。" />
  <div class="settings-groups">
    <section class="page-section">
      <h2>账号信息</h2>
      <SettingRow label="当前账号"><span class="value-text">{{ app.snapshot.account.username || '尚未登录' }}</span></SettingRow>
      <SettingRow label="登录状态">
        <span :class="['value-text', { good: app.snapshot.account.authenticated }]">{{ app.snapshot.account.authenticated ? '已登录' : '未登录' }}</span>
        <button v-if="!app.snapshot.account.authenticated" class="tiny-button" @click="router.push('/login')">登录皎月连</button>
      </SettingRow>
    </section>
    <section class="page-section">
      <h2>服务时长</h2>
      <SettingRow label="剩余服务时长" :detail="app.signinStatus?.expiresAt ? '根据系统时间与官网服务到期时间计算。' : app.signinAccount ? '请打开签到页读取官网服务到期时间。' : '登录后查看此账号的服务时长。'">
        <span :class="['account-time', { unknown: !app.signinAccount, zero: app.signinAccount && app.serviceMinutes === 0 }]">{{ app.signinAccount ? `${Math.floor(app.serviceMinutes / 60)} 小时 ${app.serviceMinutes % 60} 分钟` : '—' }}</span>
      </SettingRow>
      <SettingRow label="更新时间"><span class="value-text">{{ formatTime(app.snapshot.account.updatedAt) }}</span></SettingRow>
      <SettingRow v-if="app.signinStatus?.expiresAt" label="服务到期时间">
        <span class="value-text">{{ formatTime(app.signinStatus.expiresAt) }}</span>
      </SettingRow>
      <SettingRow v-if="app.signinStatus?.availableAt" label="官网下次可签到时间">
        <span class="value-text">{{ formatTime(app.signinStatus.availableAt) }}</span>
      </SettingRow>
      <SettingRow label="官网签到" detail="使用当前皎月连账号签到。如需验证，请按签到窗口的提示操作。">
        <button class="secondary" :data-signin="app.signinAvailability" @click="app.openSignin">签到</button>
      </SettingRow>
    </section>
  </div>
  <div class="page-actions">
    <button class="secondary" :disabled="app.loading" @click="refreshAccount">刷新时长</button>
    <button v-if="app.snapshot.account.authenticated" class="secondary" @click="confirmingLogout = true">退出登录</button>
  </div>
  <div v-if="confirmingLogout" class="inline-confirm">
    <span>退出登录会清除 LL 保存的账号登录凭据。设备记录会按账号保留。</span>
    <button class="tiny-button" @click="confirmingLogout = false">取消</button>
    <button class="small-button" :disabled="app.loading || app.busy" @click="logout">确认退出</button>
  </div>
</template>
