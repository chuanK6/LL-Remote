<script setup lang="ts">
import { useRouter } from 'vue-router'
import { bridge, native } from '../bridge'
import { useAppStore } from '../stores/app'
import type { GeneralPreferences } from '../types'
import { ROLE_SWITCH_DESCRIPTION } from '../role-switch'
import PageHeading from '../components/PageHeading.vue'
import SettingRow from '../components/SettingRow.vue'
import ToggleSwitch from '../components/ToggleSwitch.vue'

const app = useAppStore()
const router = useRouter()

function save(key: keyof GeneralPreferences, value: boolean) {
  return app.saveSetting(() => bridge.savePreferences({ [key]: value }))
}
</script>

<template>
  <PageHeading title="设置" description="调整 LL远程的运行与界面偏好。"/>
  <div class="settings-groups">
  <section class="page-section">
    <h2>启动与托盘</h2>
    <SettingRow label="开机自启动" detail="当前 Windows 用户登录后在托盘启动 LL远程。">
      <ToggleSwitch :model-value="app.snapshot.preferences.autostart" label="开机自启动" :disabled="!native || app.loading" @update:model-value="save('autostart', $event)"/>
    </SettingRow>
    <SettingRow label="关闭窗口进入托盘" detail="通过托盘菜单可以完全退出。">
      <ToggleSwitch :model-value="app.snapshot.preferences.keepRunningInTrayOnClose" label="关闭窗口进入托盘" :disabled="app.loading" @update:model-value="save('keepRunningInTrayOnClose', $event)"/>
    </SettingRow>
  </section>
  <section class="page-section">
    <h2>电源与屏幕</h2>
    <SettingRow label="防止电脑自动休眠" :detail="`运行及托盘期间有效。当前状态：${app.snapshot.powerStatus}`">
      <ToggleSwitch :model-value="app.snapshot.preferences.preventSleep" label="防止电脑自动休眠" :disabled="!native || app.loading" @update:model-value="save('preventSleep', $event)"/>
    </SettingRow>
    <SettingRow label="保持显示器常亮" detail="开启时保持显示器与电脑唤醒。">
      <ToggleSwitch :model-value="app.snapshot.preferences.keepDisplayAwake" label="保持显示器常亮" :disabled="!native || app.loading" @update:model-value="save('keepDisplayAwake', $event)"/>
    </SettingRow>
  </section>
  <section class="page-section">
    <h2>账号与登录</h2>
    <SettingRow label="记住账号登录" detail="登录凭据由当前 Windows 用户加密保存。">
      <ToggleSwitch :model-value="app.snapshot.preferences.rememberLogin" label="记住账号登录" :disabled="app.loading" @update:model-value="save('rememberLogin', $event)"/>
    </SettingRow>
    <SettingRow v-if="app.needsOnboarding" label="初次引导"><button class="tiny-button" @click="router.push(app.onboardingPath)">完成首次连接 →</button></SettingRow>
  </section>
  <section class="page-section">
    <h2>本机接受控制</h2>
    <SettingRow label="自动切换角色" :detail="ROLE_SWITCH_DESCRIPTION">
      <ToggleSwitch :model-value="app.snapshot.preferences.automaticRoleSwitch" label="自动切换角色" :disabled="!native || app.loading" @update:model-value="save('automaticRoleSwitch', $event)"/>
    </SettingRow>
    <SettingRow label="始终接受控制" :detail="app.snapshot.preferences.alwaysAcceptControl ? (app.snapshot.preferences.acceptControl && app.snapshot.account.serverRunning ? '已保存 · 本机正在接受控制' : '已保存 · 本机接受控制当前未开启') : '启动 LL远程时，使用已保存的账号和连接密码开启接受控制。'">
      <ToggleSwitch :model-value="app.snapshot.preferences.alwaysAcceptControl" label="始终接受控制" :disabled="!native || app.loading" @update:model-value="save('alwaysAcceptControl', $event)"/>
    </SettingRow>
    <SettingRow label="本机连接信息"><button class="tiny-button" @click="router.push('/assist')">查看与准备 →</button></SettingRow>
    <SettingRow label="Sunshine 管理账号"><button class="tiny-button" @click="router.push('/setup')">设置 / 验证账号 →</button></SettingRow>
  </section>
  <section class="page-section">
    <h2>控制其他电脑</h2>
    <SettingRow label="网络抖动重试" detail="有次数上限，可取消。">
      <ToggleSwitch :model-value="app.snapshot.preferences.autoRetry" label="网络抖动重试" :disabled="app.loading" @update:model-value="save('autoRetry', $event)"/>
    </SettingRow>
    <SettingRow label="远控悬浮球">
      <ToggleSwitch :model-value="app.snapshot.preferences.showFloatBall" label="远控悬浮球" :disabled="app.loading" @update:model-value="save('showFloatBall', $event)"/>
    </SettingRow>
  </section>
  <section class="page-section">
    <h2>组件与存储</h2>
    <SettingRow label="复用已有环境" detail="重新启动后优先复用正在运行的内核与服务。">
      <ToggleSwitch :model-value="app.snapshot.preferences.reuseExisting" label="复用已有环境" :disabled="app.loading" @update:model-value="save('reuseExisting', $event)"/>
    </SettingRow>
    <SettingRow label="LL远程版本"><span class="value-text">{{ app.snapshot.version }}</span></SettingRow>
    <SettingRow label="运行方式"><span class="value-text">{{ app.snapshot.portable ? '便携目录版' : 'Windows 桌面版' }}</span></SettingRow>
    <SettingRow label="内核状态"><button class="tiny-button" @click="router.push('/diagnostics')">查看组件诊断 →</button></SettingRow>
    <SettingRow v-if="app.snapshot.dataDirectory" label="数据目录"><span class="path-text">{{ app.snapshot.dataDirectory }}</span></SettingRow>
  </section>
  </div>
</template>
