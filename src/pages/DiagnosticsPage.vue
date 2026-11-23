<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { bridge, native } from '../bridge'
import { useAppStore } from '../stores/app'
import type { Diagnostic } from '../types'
import PageHeading from '../components/PageHeading.vue'
import SettingRow from '../components/SettingRow.vue'
import FeedbackMessage from '../components/FeedbackMessage.vue'
const app = useAppStore()
const route = useRoute()
const rows = ref<Diagnostic[]>([])
async function check() { const result = await app.run(() => bridge.diagnostics(typeof route.query.device === 'string' ? route.query.device : app.selectedDeviceId)); if (result) rows.value = result }
onMounted(() => { if (native) check() })
</script>
<template><PageHeading title="连接诊断" description="分别检查账号、组网、Sunshine、配对与串流。诊断不包含密码或 PIN。"/><section class="page-section"><h2>内核组件</h2><SettingRow v-for="component in app.snapshot.components" :key="component.name" :label="component.name" :detail="component.detail"><span :class="['value-text', { good: component.ready }]">{{ component.ready ? '已就绪' : component.running ? '正在运行' : component.available ? '已安装' : '资源缺失' }}</span><span class="value-text">{{ component.version }}</span></SettingRow><p v-if="!app.snapshot.components.length" class="setting-detail">{{ native ? '正在读取组件信息。' : '网页预览无法读取 Windows 组件。' }}</p></section><section class="page-section"><h2>连接步骤</h2><SettingRow v-for="row in rows" :key="row.name" :label="row.name" :detail="row.detail"><span :class="['value-text', { good: row.status === 'ok' }]">{{ ({ ok: '正常', unknown: '未确认', error: '需处理', skipped: '未检查' } as Record<string, string>)[row.status] || row.status }}</span></SettingRow></section><FeedbackMessage v-if="!native" message="请在 Windows 桌面版运行诊断，获取真实组件与连接状态。"/><div class="page-actions"><button class="primary" :disabled="app.loading || !native" @click="check">{{ app.loading ? '正在检查…' : '重新检查' }}</button></div></template>
