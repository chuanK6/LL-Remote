<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { bridge, native } from '../bridge'
import { deviceName, normalizeError } from '../domain'
import { useAppStore } from '../stores/app'
import type { DirectAccess } from '../types'
import FeedbackMessage from '../components/FeedbackMessage.vue'
import PageHeading from '../components/PageHeading.vue'

const app = useAppStore()
const route = useRoute()
const router = useRouter()
const device = computed(() => app.snapshot.devices.find(item => item.source === 'direct' && item.id === route.query.device))
const target = ref(device.value?.address || '')
const access = ref<DirectAccess>('lan')
const localAddress = ref('')
const addresses = ref<string[]>([])
const readingAddresses = ref(false)
const addressError = ref('')
const accepting = computed(() => app.snapshot.directAcceptance)
const vpnAccepting = computed(() => app.snapshot.preferences.acceptControl || app.snapshot.account.serverRunning)
const port = computed(() => accepting.value?.port ?? app.snapshot.sunshine.port - 1)
const endpoint = computed(() => {
  const address = accepting.value?.address
  if (!address) return ''
  if (port.value === 47989) return address
  return `${address.includes(':') ? '[' + address + ']' : address}:${port.value}`
})

async function readAddresses() {
  if (!native || readingAddresses.value) return
  readingAddresses.value = true
  addressError.value = ''
  try {
    const available = await bridge.directAddresses()
    addresses.value = available.sort((left, right) => Number(left.includes(':')) - Number(right.includes(':')))
    if (!available.includes(localAddress.value)) localAddress.value = available[0] || ''
    if (!available.length) addressError.value = '未找到可用的本机网卡地址。'
  } catch (error) {
    addressError.value = normalizeError(error).message
  } finally {
    readingAddresses.value = false
  }
}

async function connect() {
  const address = target.value.trim()
  if (!address) return
  await app.connect({ direct: true, deviceId: device.value?.id, address, app: 'Desktop' })
}

async function setAccepting() {
  const snapshot = await app.run(() => accepting.value
    ? bridge.setDirectAcceptance(null, null)
    : bridge.setDirectAcceptance(access.value, localAddress.value))
  if (snapshot) app.apply(snapshot)
  if (app.error?.code === 'SUNSHINE_ACCOUNT_REQUIRED') await router.push({ path: '/setup', query: { from: 'direct' } })
}

async function copyAddress() {
  try {
    await navigator.clipboard.writeText(endpoint.value)
    app.tell('直连地址已复制。')
  } catch {
    app.tell('无法访问剪贴板，请手动复制直连地址。')
  }
}

onMounted(readAddresses)
</script>

<template>
  <PageHeading title="IP 直连" />
  <form class="assist-card direct-card" @submit.prevent="connect">
    <h2>{{ device ? `连接 ${deviceName(device)}` : '连接对方电脑' }}</h2>
    <label class="form-label" for="direct-target">被控电脑地址</label>
    <div class="direct-row">
      <input id="direct-target" v-model="target" type="text" class="text-input" autocomplete="off" maxlength="80" placeholder="局域网或公网 IP，可带端口" required />
      <button class="primary" :disabled="!native || app.busy || app.loading || !target.trim()">连接</button>
    </div>
    <p class="setting-detail">IPv4 示例 <code>192.168.1.20:47989</code>。IPv6 带端口使用 <code>[IP]:端口</code>，例如 <code>[2001:db8::20]:47989</code>。</p>
  </form>
  <section class="assist-card direct-card">
    <h2>本机接受直连</h2>
    <div v-if="accepting" class="setting-row">
      <div>
        <div class="setting-name">{{ accepting.access === 'lan' ? '局域网直连已开启' : '公网直连已开启' }}</div>
        <div class="setting-detail">{{ accepting.access === 'lan' ? '给对方的直连地址' : '路由器转发目标' }} {{ endpoint }}</div>
      </div>
      <div class="setting-control">
        <button class="secondary" @click="copyAddress">复制地址</button>
        <button class="secondary" :disabled="app.loading" @click="setAccepting">关闭直连</button>
      </div>
    </div>
    <template v-else>
      <FeedbackMessage v-if="vpnAccepting" kind="warning" message="请先在「开始协助」关闭皎月连接受控制。" />
      <div class="direct-row">
        <div>
          <label class="form-label" for="direct-access">访问范围</label>
          <select id="direct-access" v-model="access"><option value="lan">局域网</option><option value="internet">公网</option></select>
        </div>
        <div>
          <label class="form-label" for="direct-address">本机网卡地址</label>
          <select id="direct-address" v-model="localAddress" :disabled="readingAddresses">
            <option value="" disabled>{{ readingAddresses ? '正在读取…' : '请选择地址' }}</option>
            <option v-for="address in addresses" :key="address" :value="address">{{ address }}</option>
          </select>
        </div>
        <button class="secondary" :disabled="!native || readingAddresses" @click="readAddresses">刷新地址</button>
        <button class="primary" :disabled="!native || app.loading || vpnAccepting || !localAddress" @click="setAccepting">开启直连</button>
      </div>
      <FeedbackMessage v-if="addressError" kind="warning" :message="addressError" />
    </template>
    <p v-if="(accepting?.access || access) === 'internet'" class="setting-detail direct-help">
      控制端填写被控端可访问的公网 IP。路由器持有公网 IPv4 时，将 TCP {{ port - 5 }}、{{ port }}、{{ port + 21 }} 和 UDP {{ port + 9 }}、{{ port + 10 }}、{{ port + 11 }}、{{ port + 13 }} 转发到所选本机地址。Sunshine 管理网页端口 {{ port + 1 }} 请留在本机。
    </p>
    <p v-else class="setting-detail direct-help">两台电脑接入同一局域网后，控制端使用这里的本机地址连接。首次连接由被控端确认 PIN。</p>
  </section>
</template>

<style scoped>
.direct-card + .direct-card { margin-top: 18px; }
.direct-card h2 { margin: 0; font-size: 16px; }
.direct-row { display: flex; align-items: end; flex-wrap: wrap; gap: 9px; }
.direct-row .text-input { width: min(320px, 100%); }
.direct-row button { white-space: nowrap; }
.direct-help { margin-top: 12px; }
</style>
