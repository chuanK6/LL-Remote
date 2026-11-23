<script setup lang="ts">
import { bridge } from '../bridge'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
import FeedbackMessage from '../components/FeedbackMessage.vue'
const app = useAppStore()
async function cancel() { await app.run(async () => { app.snapshot.session = await bridge.cancel() }) }
async function copyPin() {
  const pin = app.snapshot.session.pin
  if (!pin) return
  try { await navigator.clipboard.writeText(pin); app.tell('配对 PIN 已复制，请告知被控端。') }
  catch { app.tell('无法访问剪贴板，请将四位 PIN 告知被控端。') }
}
</script>
<template>
  <PageHeading title="首次配对 · 授权远程桌面" description="皎月连连接已经建立。请将下面的四位 PIN 告知被控端，完成 Sunshine 首次授权。" />
  <template v-if="app.snapshot.session.stage === 'pairing' && app.snapshot.session.pin">
    <div class="pairing-code" aria-label="配对 PIN" role="status">{{ app.snapshot.session.pin }}</div>
    <ol class="pairing-instructions"><li>被控电脑的 LL远程会自动弹出 PIN 确认窗口。</li><li>在被控端输入上面的四位 PIN，点击“确认并允许连接”。</li><li>确认成功后，本机自动进入桌面，后续连接会复用配对授权。</li></ol>
    <FeedbackMessage message="被控端没有弹窗时，请在其 LL远程中验证 Sunshine 管理账号，也可进入 Sunshine 常用设置确认配对。" />
    <div class="page-actions start"><button class="secondary" @click="copyPin">复制 PIN</button><button class="secondary" @click="cancel">取消配对</button></div>
  </template>
  <FeedbackMessage v-else message="当前没有待确认的配对请求。" />
</template>
