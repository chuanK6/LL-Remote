<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { bridge, native } from '../bridge'
import { useAppStore } from '../stores/app'
import PageHeading from '../components/PageHeading.vue'
import FeedbackMessage from '../components/FeedbackMessage.vue'
const app = useAppStore()
const route = useRoute()
const router = useRouter()
const login = computed(() => route.name === 'login')
const returnToDirect = computed(() => !login.value && route.query.from === 'direct')
const mode = ref(app.snapshot.sunshine.accountInitialized ? 'existing' : 'create')
const username = ref(login.value ? app.snapshot.account.username || '' : app.snapshot.preferences.sunshineUsername || 'sunshine')
const reusingLogin = computed(() => login.value && app.snapshot.account.authenticated && username.value.trim() === app.snapshot.account.username)
const password = ref('')
const confirm = ref('')
const remember = ref(app.snapshot.preferences.rememberLogin)
const acceptAfterLogin = ref(app.snapshot.preferences.alwaysAcceptControl)
const initialAcceptAfterLogin = acceptAfterLogin.value
const connectionPassword = ref('')
const webPassword = ref('')
const validation = ref('')
const localAuthorization = ref(false)
const localPassword = ref('')
let active = true
let passwordsLoading = false, lastConnectionPassword = '', lastWebPassword = ''
async function readPasswords() {
  if (!native || !login.value || passwordsLoading) return
  passwordsLoading = true
  try {
    const value = await bridge.localServerPasswords()
    if (!active) return
    if (!connectionPassword.value || connectionPassword.value === lastConnectionPassword) connectionPassword.value = value.connectionPassword
    if (!webPassword.value || webPassword.value === lastWebPassword) webPassword.value = value.webPassword
    lastConnectionPassword = value.connectionPassword; lastWebPassword = value.webPassword
  } catch {} finally { passwordsLoading = false }
}
onMounted(readPasswords)
watch(() => app.snapshot.account.updatedAt, readPasswords)
watch(() => app.snapshot.sunshine.accountInitialized, initialized => { if (!login.value && initialized && mode.value === 'create') mode.value = 'existing' })
async function continueLogin() {
  await app.refresh()
  if (active && app.snapshot.preferences.setupComplete) await router.push(returnToDirect.value ? '/direct' : app.onboardingPath)
}
async function submit() {
  validation.value = ''
  if (!username.value.trim() || (!reusingLogin.value && !password.value)) { validation.value = '请填写用户名和密码。'; return }
  if (!login.value && mode.value !== 'existing' && password.value !== confirm.value) { validation.value = '两次输入的密码不一致。'; return }
  const result = await app.run(() => login.value ? reusingLogin.value ? bridge.refresh() : bridge.login(username.value.trim(), password.value, remember.value) : bridge.sunshineAccount(username.value.trim(), password.value, confirm.value, mode.value))
  if (!active) return
  if (!result) {
    if (app.error?.code === 'LOCAL_AUTHORIZATION_REQUIRED') localAuthorization.value = true
    if (!login.value && app.error?.code === 'SUNSHINE_ACCOUNT_REQUIRED') {
      if (app.error.message.includes('已检测到')) mode.value = 'existing'
      if (app.error.message.includes('尚未设置')) mode.value = 'create'
    }
    return
  }
  password.value = ''; confirm.value = ''; app.apply(result)
  if (login.value && acceptAfterLogin.value && !result.account.serverRunning) {
    const accepted = await app.run(() => bridge.acceptControl(true, connectionPassword.value, webPassword.value))
    if (!active) return
    if (accepted) { app.apply(accepted); connectionPassword.value = ''; webPassword.value = '' }
    else return
  }
  if (login.value && (acceptAfterLogin.value !== initialAcceptAfterLogin || remember.value !== app.snapshot.preferences.rememberLogin)) {
    const saved = await app.run(() => bridge.savePreferences({ alwaysAcceptControl: acceptAfterLogin.value, rememberLogin: remember.value }))
    if (!active || !saved) return
    app.apply(saved)
  }
  await router.push(returnToDirect.value ? '/direct' : result.account.authenticated ? '/assist' : '/login')
}
async function authorize() {
  const result = await app.run(() => bridge.authorize(localPassword.value))
  localPassword.value = ''
  if (result) { app.apply(result); localAuthorization.value = false }
}
onBeforeUnmount(() => { active = false; password.value = ''; confirm.value = ''; localPassword.value = ''; connectionPassword.value = ''; webPassword.value = '' })
</script>
<template><div class="setup-stepper"><span :class="{ current: !login, complete: app.snapshot.preferences.setupComplete }">1 Sunshine 管理账号{{ app.snapshot.preferences.setupComplete ? ' ✓' : '' }}</span><span :class="{ current: login, complete: app.snapshot.account.authenticated }">2 皎月连登录{{ app.snapshot.account.authenticated ? ' ✓' : '' }}</span><span>3 设备连接</span></div><PageHeading :title="login ? '登录皎月连' : app.snapshot.preferences.setupComplete ? 'Sunshine 管理账号' : '首次使用 · 设置 Sunshine 账号'" :description="login ? '使用皎月连账号获取在线主机，已有登录状态会自动复用。' : '创建或验证本机管理账号；高级设置中的账号状态会自动同步。'"/>
  <FeedbackMessage v-if="!login && app.snapshot.preferences.setupComplete" message="Sunshine 管理账号已设置，可以继续登录皎月连。在 LL 内确认 PIN 或修改采集设置时，可在此验证并保存管理凭据。" kind="success"/>
  <div v-if="!login && app.snapshot.preferences.setupComplete" class="page-actions start"><button type="button" class="primary" :disabled="app.loading" @click="continueLogin">{{ returnToDirect ? '返回 IP 直连' : app.snapshot.account.authenticated ? '进入设备列表' : '继续登录皎月连' }}</button></div>
  <FeedbackMessage v-if="login && app.snapshot.preferences.setupComplete" message="第一步已完成，已同步 Sunshine 的实际账号状态。" kind="success"/>
  <div v-if="login && !app.snapshot.preferences.setupComplete" class="inline-confirm"><span>Sunshine 管理账号尚未设置。</span><button type="button" class="tiny-button" @click="router.push('/setup')">设置 Sunshine 管理账号</button></div>
  <FeedbackMessage v-if="!login && app.snapshot.sunshine.external && !app.snapshot.sunshine.running" message="Sunshine 服务尚未启动，保存或验证账号时会由 LL远程请求 Windows 授权并完成启动。"/>
  <div v-if="!login" class="inline-tabs"><button type="button" :class="{ active: mode === 'create' }" @click="mode = 'create'">创建管理账号</button><button type="button" :class="{ active: mode === 'existing' }" @click="mode = 'existing'">使用已有账号</button><button v-if="app.snapshot.sunshine.credentialValid" type="button" :class="{ active: mode === 'change' }" @click="mode = 'change'">修改管理账号</button></div>
  <form class="setup-form" @submit.prevent="submit"><label class="form-label" for="setupUser">{{ login ? '皎月连账号' : '用户名' }}</label><input id="setupUser" v-model="username" class="text-input" required autocomplete="username" maxlength="128"/><label class="form-label" for="setupPassword">{{ mode === 'change' && !login ? '新密码' : '密码' }}</label><input id="setupPassword" v-model="password" class="text-input" type="password" :required="!reusingLogin" :autocomplete="login || mode === 'existing' ? 'current-password' : 'new-password'" maxlength="1024"/><template v-if="!login && mode !== 'existing'"><label class="form-label" for="setupConfirm">确认密码</label><input id="setupConfirm" v-model="confirm" class="text-input" type="password" required autocomplete="new-password" maxlength="1024"/></template>
    <template v-if="login"><label class="checkbox-row"><input v-model="remember" type="checkbox"/>在本机记住登录</label><div class="role-choice"><span class="form-label">本机用途</span><label><input v-model="acceptAfterLogin" type="radio" :value="false"/>仅控制其他电脑</label><label><input v-model="acceptAfterLogin" type="radio" :value="true"/>本机也接受控制</label></div><template v-if="acceptAfterLogin && !app.snapshot.account.serverRunning"><label class="form-label" for="localConnectionPassword">本机连接密码</label><input id="localConnectionPassword" v-model="connectionPassword" class="text-input" type="password" placeholder="已有有效配置时可留空" autocomplete="new-password"/></template></template>
    <template v-if="login && acceptAfterLogin && !app.snapshot.account.serverRunning"><label class="form-label" for="setupWebPassword">组网页面访问密码（可选）</label><input id="setupWebPassword" v-model="webPassword" class="text-input" type="password" autocomplete="new-password" minlength="6" maxlength="20" placeholder="留空时使用已有页面密码或本机连接密码"/><p class="form-note">页面访问密码需要 6–20 位。本机连接密码较长时，请单独设置页面密码。</p></template>
    <p class="form-note">{{ login ? '记住登录使用当前 Windows 用户的加密凭据存储。' : 'Sunshine 管理密码、皎月连密码、连接密码和四位 PIN 分别使用。' }}</p><FeedbackMessage :message="validation" kind="error"/><div class="page-actions start"><button class="primary" type="submit" :disabled="app.loading || (login && !app.snapshot.preferences.setupComplete)">{{ app.loading ? '正在验证…' : login ? '登录并继续' : mode === 'existing' ? '验证并继续' : '保存并继续' }}</button><button type="button" class="tiny-button" @click="router.push('/diagnostics')">连接诊断</button></div></form>
  <form v-if="localAuthorization" class="setup-form" @submit.prevent="authorize"><FeedbackMessage message="皎月连的本地管理接口设有访问密码，请验证后继续。" kind="warning"/><label class="form-label" for="natWebPassword">本地管理访问密码</label><input id="natWebPassword" v-model="localPassword" type="password" class="text-input" autocomplete="current-password"/><div class="page-actions start"><button class="secondary" type="submit" :disabled="app.loading">验证访问密码</button></div></form>
</template>
