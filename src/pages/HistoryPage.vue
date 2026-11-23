<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { deviceName, formatTime, statusLabel } from '../domain'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
import SettingRow from '../components/SettingRow.vue'
const app = useAppStore()
const route = useRoute()
const router = useRouter()
const device = computed(() => app.snapshot.devices.find(d => d.id === route.params.id))
const history = computed(() => app.snapshot.history.filter(h => h.deviceId === route.params.id).slice().reverse())
</script>
<template><PageHeading :title="`连接记录${device ? ` · ${deviceName(device)}` : ''}`" description="识别码成功历史与收藏分别保存，历史不代表实时在线。"/><section v-if="device" class="page-section"><h2>设备信息</h2><SettingRow label="识别码"><span class="value-text">{{ device.peerCode || '—' }}</span></SettingRow><SettingRow label="最近连接"><span class="value-text">{{ formatTime(device.lastConnectedAt) }}</span></SettingRow><SettingRow label="保存的授权"><span :class="['value-text', { good: device.credentialState === 'valid' }]">{{ device.credentialState === 'valid' ? '直接重连，无需重复输入密码' : device.credentialState === 'invalid' ? '需要更新连接密码' : '尚无可复用凭据' }}</span></SettingRow><SettingRow label="在线状态"><span class="value-text">{{ statusLabel(device.status) }}</span></SettingRow></section><section class="page-section"><h2>连接历史</h2><table v-if="history.length" class="device-table history-table"><thead><tr><th>时间</th><th>方式</th><th>结果</th></tr></thead><tbody><tr v-for="entry in history" :key="entry.id"><td>{{ formatTime(entry.connectedAt) }}</td><td>{{ entry.method }}</td><td>{{ entry.result === 'success' ? '成功' : entry.result === 'canceled' ? '已取消' : '失败' }}</td></tr></tbody></table><div v-else class="empty-history">暂无此设备的连接记录</div></section><div v-if="device" class="page-actions"><button class="secondary" @click="router.push(`/device/${device.id}`)">查看设备</button><button class="primary" :disabled="app.busy" @click="app.connect({ deviceId: device.id })">再次连接</button></div></template>
