import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { router } from './router'
import './styles.css'
import './app.css'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'

const windowLabel = isTauri() ? getCurrentWindow().label : ''
if (['session-controls', 'session-performance'].includes(windowLabel) || new URLSearchParams(location.search).get('window') === 'session') {
  document.documentElement.classList.add('session-window')
}

createApp(App).use(createPinia()).use(router).mount('#app')
