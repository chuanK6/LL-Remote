<script setup lang="ts">
import { ChevronRight, Clock3, Trash2 } from 'lucide-vue-next'
import type { RouteLocationRaw } from 'vue-router'
import type { Device } from '../types'
import { deviceName, formatTime } from '../domain'
import DeviceWallpaper from './DeviceWallpaper.vue'

interface SavedDeviceCard {
  id: string
  device: Device
  connectedAt: string | null
  method: string
  peerCode: string | null
  to: RouteLocationRaw
}
defineProps<{ items: SavedDeviceCard[]; deleteLabel: string; disabled?: boolean }>()
defineEmits<{ delete: [itemId: string] }>()
</script>

<template>
  <ol class="saved-device-list" aria-label="设备列表">
    <li v-for="item in items" :key="item.id" class="saved-device-item">
      <RouterLink :to="item.to" class="saved-device-card" :aria-label="`${deviceName(item.device)}，${item.connectedAt ? formatTime(item.connectedAt) : '时间未记录'}，查看详情`">
        <span class="saved-device-thumbnail" aria-hidden="true"><DeviceWallpaper :path="item.device.wallpaper" loading="lazy" decoding="async" /></span>
        <div class="saved-device-content">
          <h2>{{ deviceName(item.device) }}</h2>
          <p class="saved-device-meta"><span>{{ item.method }}</span><span v-if="item.peerCode">识别码 {{ item.peerCode }}</span></p>
          <p v-if="item.device.note" class="saved-device-note">{{ item.device.note }}</p>
        </div>
        <div class="saved-device-time"><Clock3 :size="14" aria-hidden="true" /><time :datetime="item.connectedAt || undefined">{{ item.connectedAt ? formatTime(item.connectedAt) : '时间未记录' }}</time></div>
        <ChevronRight class="saved-device-arrow" :size="21" aria-hidden="true" />
      </RouterLink>
      <button type="button" class="saved-device-delete" :aria-label="`${deleteLabel} · ${deviceName(item.device)}`" :title="deleteLabel" :disabled="disabled" @click.stop="$emit('delete', item.id)"><Trash2 :size="18" aria-hidden="true" /></button>
    </li>
  </ol>
</template>

<style scoped>
.saved-device-list { display: grid; gap: 15px; margin: 0; padding: 0; list-style: none; }
.saved-device-item { position: relative; }
.saved-device-card { display: grid; grid-template-columns: 116px minmax(0, 1fr) auto 38px; align-items: center; gap: 20px; min-height: 120px; padding: 20px 24px; border: 1px solid #dce5ee; border-radius: 12px; color: var(--ink); background: linear-gradient(110deg, #fff, #fcfdff); box-shadow: 0 3px 9px #263e5910, 0 1px 2px #263e5908; transition: background 160ms, border-color 160ms, box-shadow 160ms; }
.saved-device-item:hover .saved-device-card, .saved-device-item:focus-within .saved-device-card { background: #edf5ff; border-color: #91baff; box-shadow: 0 5px 16px #3473bc1c; }
.saved-device-thumbnail { position: relative; display: block; width: 116px; height: 76px; overflow: hidden; border: 1px solid #d7e7fc; border-radius: 10px; background: #eaf3ff; }
.saved-device-thumbnail :deep(.cover-art) { position: static; inset: auto; display: block; width: 100%; height: 100%; object-fit: cover; }
.saved-device-content { min-width: 0; }
.saved-device-card h2 { margin: 0; font-size: 17px; font-weight: 600; overflow-wrap: anywhere; }
.saved-device-meta { display: flex; flex-wrap: wrap; gap: 6px 17px; margin: 9px 0 0; font-size: 12px; color: #75879a; }
.saved-device-note { margin: 7px 0 0; font-size: 12px; color: #738294; overflow-wrap: anywhere; }
.saved-device-time { display: flex; align-items: center; justify-content: end; gap: 7px; color: #738294; font-size: 12px; white-space: nowrap; }
.saved-device-arrow { justify-self: center; color: #a0afbf; }
.saved-device-delete { position: absolute; top: 50%; right: 24px; transform: translateY(-50%); display: grid; place-items: center; width: 38px; height: 38px; border: 1px solid #efcbd0; border-radius: 9px; color: #b04755; background: #fff4f5; opacity: 0; pointer-events: none; transition: opacity 130ms, background 130ms; }
.saved-device-item:hover .saved-device-delete, .saved-device-item:focus-within .saved-device-delete { opacity: 1; pointer-events: auto; }
.saved-device-item:hover .saved-device-arrow, .saved-device-item:focus-within .saved-device-arrow { opacity: 0; }
.saved-device-delete:hover { background: #fbe1e5; }
@media (max-width: 950px) {
  .saved-device-card { grid-template-columns: 96px minmax(0, 1fr) 38px; gap: 11px 16px; padding: 20px; }
  .saved-device-thumbnail { grid-row: 1 / 3; width: 96px; height: 64px; }
  .saved-device-time { grid-column: 2; justify-content: start; }
  .saved-device-arrow { grid-column: 3; grid-row: 1 / 3; }
  .saved-device-delete { right: 20px; }
}
@media (max-width: 700px) { .saved-device-card { grid-template-columns: 88px minmax(0, 1fr) 38px; padding: 16px; gap: 10px 12px; } .saved-device-thumbnail { width: 88px; height: 60px; } .saved-device-delete { right: 16px; } }
@media (hover: none) { .saved-device-delete { opacity: 1; pointer-events: auto; } .saved-device-arrow { opacity: 0; } }
@media (prefers-reduced-motion: reduce) { .saved-device-card, .saved-device-delete { transition: none; } }
</style>
