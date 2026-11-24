<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { X } from 'lucide-vue-next'
import { bridge, native } from '../bridge'
import { deviceName, normalizeError } from '../domain'
import { useAppStore } from '../stores/app'
import type { Device } from '../types'
import DeviceWallpaper from './DeviceWallpaper.vue'
import FeedbackMessage from './FeedbackMessage.vue'
const props = defineProps<{ device: Device }>()
const emit = defineEmits<{ close: [] }>()
const app = useAppStore()
const dialog = ref<HTMLElement>()
const nameInput = ref<HTMLInputElement>()
const name = ref(deviceName(props.device))
const favorite = ref(props.device.favorite)
const note = ref(props.device.note)
const wallpaperPath = ref<string | null>(null)
const resetWallpaper = ref(false)
const imagePreview = ref('')
const imageName = ref('')
const loading = ref(false)
const readingImage = ref(false)
const error = ref('')
const previousFocus = document.activeElement as HTMLElement | null
let objectUrl: string | undefined
let imageRequest = 0

function close() { if (!loading.value) emit('close') }
function keydown(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); close() }
  if (event.key !== 'Tab') return
  const elements = Array.from(dialog.value?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea, [tabindex="0"]') || [])
  const first = elements[0], last = elements.at(-1)
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus() }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus() }
}
async function chooseWallpaper() {
  if (!native) { document.getElementById('previewWallpaperInput')?.click(); return }
  const request = ++imageRequest
  readingImage.value = true; error.value = ''
  try {
    const path = await open({ multiple: false, directory: false, filters: [{ name: '图片（不超过 5 MB）', extensions: ['png', 'jpg', 'jpeg', 'webp'] }] })
    if (!path || typeof path !== 'string' || request !== imageRequest) return
    // Commit the draft only after validation; a rejected file keeps the old image.
    const preview = await bridge.previewWallpaper(path)
    if (request !== imageRequest) return
    wallpaperPath.value = path; resetWallpaper.value = false
    imagePreview.value = preview; imageName.value = path.split(/[\\/]/).at(-1) || ''
  } catch (caught) {
    if (request === imageRequest) error.value = normalizeError(caught).message
  } finally {
    if (request === imageRequest) readingImage.value = false
  }
}
function previewFile(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file) return
  if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type) || file.size > 5 * 1024 * 1024) { error.value = '请选择不超过 5 MB 的 PNG、JPEG 或 WebP 图片。'; return }
  error.value = ''
  if (objectUrl) URL.revokeObjectURL(objectUrl)
  objectUrl = URL.createObjectURL(file); imagePreview.value = objectUrl; imageName.value = file.name; resetWallpaper.value = false
}
function restoreDefault() { imageRequest++; readingImage.value = false; error.value = ''; resetWallpaper.value = true; wallpaperPath.value = null; imagePreview.value = ''; imageName.value = '' }
async function save() {
  if (loading.value || readingImage.value) return
  if (!name.value.trim() || Array.from(name.value.trim()).length > 64) { error.value = '设备名称须为 1–64 个字符。'; return }
  loading.value = true; error.value = ''
  try {
    app.apply(await bridge.editDevice({ deviceId: props.device.id, displayName: name.value.trim(), favorite: favorite.value, note: note.value, wallpaperPath: wallpaperPath.value, resetWallpaper: resetWallpaper.value }))
    emit('close'); app.tell('设备设置已保存。')
  } catch (caught) { error.value = normalizeError(caught).message } finally { loading.value = false }
}
onMounted(async () => { document.querySelector('.shell')?.setAttribute('inert', ''); document.addEventListener('keydown', keydown); await nextTick(); nameInput.value?.focus() })
onBeforeUnmount(() => { imageRequest++; document.querySelector('.shell')?.removeAttribute('inert'); document.removeEventListener('keydown', keydown); previousFocus?.focus(); if (objectUrl) URL.revokeObjectURL(objectUrl) })
</script>
<template>
  <Teleport to="body"><div class="device-editor-backdrop" @click.self="close"><section ref="dialog" class="device-editor" role="dialog" aria-modal="true" aria-labelledby="deviceEditorTitle"><div class="editor-heading"><h2 id="deviceEditorTitle">编辑设备</h2><button type="button" class="plain-icon" aria-label="关闭设备编辑弹窗" :disabled="loading" @click="close"><X :size="22"/></button></div>
    <form @submit.prevent="save">
      <label class="form-label" for="editorName">设备名称</label>
      <input id="editorName" ref="nameInput" v-model="name" class="text-input" maxlength="64" required autocomplete="off"/>
      <div class="editor-favorite-row"><label for="editorFavorite">收藏设备</label><input id="editorFavorite" v-model="favorite" type="checkbox"/></div>
      <label class="form-label" for="editorNote">备注</label>
      <input id="editorNote" v-model="note" class="text-input" maxlength="256" placeholder="仅保存在本机"/>
      <div class="editor-wallpaper-heading">设备壁纸（PNG、JPEG、WebP，不超过 5 MB）</div>
      <div class="editor-wallpaper-preview"><img v-if="imagePreview" :src="imagePreview" alt="壁纸预览"/><DeviceWallpaper v-else :path="resetWallpaper ? null : device.wallpaper"/></div>
      <div class="wallpaper-actions">
        <button type="button" class="secondary" :disabled="loading || readingImage" @click="chooseWallpaper">更换壁纸</button>
        <button type="button" class="tiny-button" :disabled="loading" @click="restoreDefault">恢复默认</button>
        <span>{{ imageName || (resetWallpaper || !device.wallpaper ? '默认壁纸' : '已保存的壁纸') }}</span>
      </div>
      <input v-if="!native" id="previewWallpaperInput" type="file" accept="image/png,image/jpeg,image/webp" hidden @change="previewFile"/>
      <FeedbackMessage v-if="wallpaperPath" message="已选择图片，保存后应用到此设备。"/>
      <FeedbackMessage :message="error" kind="error"/>
      <div class="page-actions">
        <button class="secondary" type="button" :disabled="loading" @click="close">取消</button>
        <button class="primary" type="submit" :disabled="loading || readingImage">{{ readingImage ? '正在读取图片…' : loading ? '正在保存…' : '保存' }}</button>
      </div>
    </form>
  </section></div></Teleport>
</template>
