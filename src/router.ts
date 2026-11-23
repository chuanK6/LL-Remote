import { createRouter, createWebHashHistory } from 'vue-router'
import { useAppStore } from './stores/app'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import DevicePage from './pages/DevicePage.vue'
import DevicesPage from './pages/DevicesPage.vue'
import SetupPage from './pages/SetupPage.vue'
import AppsPage from './pages/AppsPage.vue'
import AssistPage from './pages/AssistPage.vue'
import AccountPage from './pages/AccountPage.vue'
import SettingsPage from './pages/SettingsPage.vue'
import DiagnosticsPage from './pages/DiagnosticsPage.vue'
import HistoryPage from './pages/HistoryPage.vue'
import QuickAppsPage from './pages/QuickAppsPage.vue'
import PairingPage from './pages/PairingPage.vue'

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: isTauri() ? '/devices' : '/device/preview-home' },
    { path: '/device/:id', name: 'device', component: DevicePage },
    { path: '/devices', name: 'devices', component: DevicesPage },
    { path: '/favorites', name: 'favorites', component: DevicesPage },
    { path: '/setup', name: 'setup', component: SetupPage },
    { path: '/login', name: 'login', component: SetupPage },
    { path: '/app/:app', name: 'apps', component: AppsPage },
    { path: '/assist', name: 'assist', component: AssistPage },
    { path: '/account', name: 'account', component: AccountPage },
    { path: '/settings', name: 'settings', component: SettingsPage },
    { path: '/diagnostics', name: 'diagnostics', component: DiagnosticsPage },
    { path: '/history/:id', name: 'history', component: HistoryPage },
    { path: '/quick-apps/:id', name: 'quick-apps', component: QuickAppsPage },
    { path: '/pairing', name: 'pairing', component: PairingPage },
    { path: '/:pathMatch(.*)*', redirect: '/devices' },
  ],
})
let initialNavigation = true
let navigationGeneration = 0
router.beforeEach(async to => {
  if ((isTauri() && ['session-controls', 'session-performance'].includes(getCurrentWindow().label)) || new URLSearchParams(location.search).get('window') === 'session') return true
  const initial = initialNavigation
  initialNavigation = false
  const generation = ++navigationGeneration
  const app = useAppStore()
  await app.init()
  if (generation !== navigationGeneration) return true
  if (isTauri() && initial && to.name === 'devices' && app.needsOnboarding && app.onboardingPath !== '/devices') return app.onboardingPath
})
