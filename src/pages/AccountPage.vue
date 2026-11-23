<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { bridge } from '../bridge'
import { formatTime, quotaLabel } from '../domain'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
import SettingRow from '../components/SettingRow.vue'
const app = useAppStore()
const router = useRouter()
const confirmingLogout = ref(false)
async function logout() { const result = await app.run(() => bridge.logout()); if (result) { app.apply(result); await router.push('/login') } }
</script>
<template><PageHeading title="皎月连账号" description="查看当前账号与可读取的连接时长。"/><div class="settings-groups"><section class="page-section"><h2>账号信息</h2><SettingRow label="当前账号"><span class="value-text">{{ app.snapshot.account.username || '尚未登录' }}</span></SettingRow><SettingRow label="登录状态"><span :class="['value-text', { good: app.snapshot.account.authenticated }]">{{ app.snapshot.account.authenticated ? '已登录' : '未登录' }}</span><button v-if="!app.snapshot.account.authenticated" class="tiny-button" @click="router.push('/login')">登录皎月连</button></SettingRow></section><section class="page-section"><h2>连接时长</h2><SettingRow label="剩余可用时长" :detail="app.snapshot.account.quotaMinutes === null ? '当前接口未提供可确认的余额，可前往官网查询。' : '显示内核返回的余额；套餐到期日期不换算为可用时长。'"><span :class="['account-time', { unknown: app.snapshot.account.quotaMinutes === null, zero: app.snapshot.account.quotaMinutes === 0 }]">{{ quotaLabel(app.snapshot.account) }}</span></SettingRow><SettingRow label="更新时间"><span class="value-text">{{ formatTime(app.snapshot.account.updatedAt) }}</span></SettingRow><SettingRow label="官网签到"><button class="secondary" @click="bridge.openSignin">前往官网签到 ↗</button></SettingRow></section></div><div class="page-actions"><button class="secondary" :disabled="app.loading" @click="app.refresh">刷新时长</button><button v-if="app.snapshot.account.authenticated" class="secondary" @click="confirmingLogout = true">退出登录</button></div><div v-if="confirmingLogout" class="inline-confirm"><span>退出登录会清除 LL 保存的账号登录凭据。设备记录会按账号保留。</span><button class="tiny-button" @click="confirmingLogout = false">取消</button><button class="small-button" :disabled="app.loading || app.busy" @click="logout">确认退出</button></div></template>
