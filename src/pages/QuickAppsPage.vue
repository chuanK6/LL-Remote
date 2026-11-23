<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { bridge } from '../bridge'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
import FeedbackMessage from '../components/FeedbackMessage.vue'
const app = useAppStore()
const route = useRoute()
const device = computed(() => app.snapshot.devices.find(d => d.id === route.params.id))
async function refresh() { if (device.value) await app.run(async () => { app.apply(await bridge.refreshApps(device.value!.id)) }) }
function change(name: string, enabled: boolean) {
  const current = device.value
  if (!current) return
  const apps = enabled ? [...current.quickApps, name] : current.quickApps.filter(app => app !== name)
  return app.saveSetting(() => bridge.saveQuickApps(current.id, apps))
}
</script>
<template><PageHeading title="添加快速启动" description="选择对方 Sunshine 已发布的应用，作为此设备的快速启动入口。"/><div v-if="device" class="settings-groups"><section class="page-section"><h2>已发布应用</h2><div v-for="application in device.apps" :key="application.id" class="setting-row"><label class="checkbox-row"><input type="checkbox" :checked="device.quickApps.includes(application.name)" :disabled="app.loading" @change="change(application.name, ($event.target as HTMLInputElement).checked)"/>{{ application.name }}</label></div><FeedbackMessage v-if="!device.apps.length" message="尚未读取应用。完成配对并连接后可刷新应用列表。"/><div class="page-actions"><button class="secondary" :disabled="app.loading" @click="refresh">刷新应用</button></div></section></div></template>
