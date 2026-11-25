import { createRouter, createWebHashHistory } from 'vue-router'
import { useAppStore } from './stores/app'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import DevicePage from './pages/DevicePage.vue'
import FavoritesPage from './pages/FavoritesPage.vue'
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
    { path: '/', redirect: isTauri() ? '/assist' : '/device/preview-home' },
    { path: '/device/:id', name: 'device', component: DevicePage },
    { path: '/devices', redirect: '/assist' },
    { path: '/favorites', name: 'favorites', component: FavoritesPage },
    { path: '/favorites/device/:id', name: 'favorite-device', component: DevicePage },
    { path: '/setup', name: 'setup', component: SetupPage },
    { path: '/login', name: 'login', component: SetupPage },
    { path: '/app/:app', name: 'apps', component: AppsPage },
    { path: '/assist', name: 'assist', component: AssistPage },
    { path: '/account', name: 'account', component: AccountPage },
    { path: '/settings', name: 'settings', component: SettingsPage },
    { path: '/diagnostics', name: 'diagnostics', component: DiagnosticsPage },
    { path: '/history', name: 'history', component: HistoryPage },
    { path: '/history/record/:id', name: 'history-device', component: DevicePage },
    { path: '/history/device/:id', name: 'saved-history-device', component: DevicePage },
    { path: '/history/:id', redirect: to => ({ path: '/history', query: { device: String(to.params.id) } }) },
    { path: '/quick-apps/:id', name: 'quick-apps', component: QuickAppsPage },
    { path: '/pairing', name: 'pairing', component: PairingPage },
    { path: '/:pathMatch(.*)*', redirect: '/assist' },
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
  if (isTauri() && initial && to.name === 'assist' && app.needsOnboarding && app.onboardingPath !== '/assist') return app.onboardingPath
})
