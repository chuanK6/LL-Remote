<script setup lang="ts">
import { computed, nextTick } from 'vue'
import type { SessionPatch, StreamPreferences } from '../types'

defineProps<{ live?: boolean; disabled?: boolean }>()
const emit = defineEmits<{ apply: [patch: SessionPatch]; commit: [] }>()
const stream = defineModel<StreamPreferences>({ required: true })

const resolutions = ['1280x720', '1600x900', '1920x1080', '1920x1200', '2560x1440', '3440x1440', '3840x2160']
const frameRates = [24, 30, 48, 50, 60, 90, 120, 144, 165, 240]
const minBitrate = 1
const maxBitrate = 500
const resolution = computed({
  get: () => `${stream.value.width}x${stream.value.height}`,
  set: (value: string) => {
    const [width, height] = value.split('x').map(Number)
    stream.value = { ...stream.value, width, height }
  },
})
const fps = computed({
  get: () => stream.value.fps,
  set: (fps: number) => { stream.value = { ...stream.value, fps } },
})
const bitrate = computed({
  get: () => stream.value.bitrateKbps / 1000,
  set: (mbps: number) => { stream.value = { ...stream.value, bitrateKbps: Math.round(mbps * 1000) } },
})
let bitrateAtFocus: number | undefined

function editBitrate(event: Event) {
  const value = (event.target as HTMLInputElement).valueAsNumber
  if (value >= minBitrate && value <= maxBitrate) bitrate.value = value
}

async function commitBitrate(event: Event) {
  const input = event.target as HTMLInputElement
  if (Number.isFinite(input.valueAsNumber)) {
    bitrate.value = Math.min(maxBitrate, Math.max(minBitrate, input.valueAsNumber))
  }
  await nextTick()
  input.value = String(bitrate.value)
  if (bitrateAtFocus !== bitrate.value) await commitQuality('bitrate')
  bitrateAtFocus = undefined
}

async function commitQuality(kind: 'resolution' | 'fps' | 'bitrate') {
  await nextTick()
  const { width, height, fps, bitrateKbps } = stream.value
  switch (kind) {
    case 'resolution': emit('apply', { kind, width, height }); break
    case 'fps': emit('apply', { kind, fps }); break
    case 'bitrate': emit('apply', { kind, bitrateKbps }); break
  }
  emit('commit')
}
</script>

<template>
  <div class="quality-fields" :class="{ compact: live }">
    <div class="quality-row">
      <label for="quality-resolution">分辨率</label>
      <select id="quality-resolution" v-model="resolution" :disabled="disabled" @change="commitQuality('resolution')">
        <option v-for="preset in resolutions" :key="preset" :value="preset">{{ preset.replace('x', ' × ') }}</option>
        <option v-if="!resolutions.includes(resolution)" :value="resolution" disabled>{{ stream.width }} × {{ stream.height }}（已保存）</option>
      </select>
    </div>
    <div class="quality-row">
      <label for="quality-fps">帧率</label>
      <select id="quality-fps" v-model.number="fps" :disabled="disabled" @change="commitQuality('fps')">
        <option v-for="preset in frameRates" :key="preset" :value="preset">{{ preset }} FPS</option>
        <option v-if="!frameRates.includes(fps)" :value="fps" disabled>{{ fps }} FPS（已保存）</option>
      </select>
    </div>
    <div class="quality-row">
      <label for="quality-bitrate">码率</label>
      <div class="bitrate-control">
        <input id="quality-bitrate" v-model.number="bitrate" type="range" :min="minBitrate" :max="maxBitrate" step="any"
          :aria-valuetext="`${bitrate} Mbps`" :disabled="disabled" @change="commitQuality('bitrate')" />
        <input class="bitrate-number" type="number" :min="minBitrate" :max="maxBitrate" step="0.001"
          aria-label="码率数值（Mbps）" :value="bitrate" :disabled="disabled"
          @focus="bitrateAtFocus = bitrate" @input="editBitrate" @blur="commitBitrate" @keydown.enter.prevent="($event.currentTarget as HTMLInputElement).blur()" />
        <span class="bitrate-unit" aria-hidden="true">Mbps</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.quality-row {
  display: grid;
  grid-template-columns: minmax(4em, 1fr) minmax(0, 245px);
  align-items: center;
  gap: 10px;
  min-height: 60px;
  border-bottom: 1px solid #e8edf1;
  font-size: 14px;
}
.quality-row select { width: 100%; min-width: 0; }
.bitrate-control { display: grid; grid-template-columns: minmax(0, 1fr) 64px auto; align-items: center; gap: 7px; }
.bitrate-control input { width: 100%; min-width: 0; margin: 0; }
.bitrate-number { height: 32px; padding: 0 5px; border: 1px solid #d6e0e8; border-radius: 5px; font-size: 12px; color: #314658; }
.bitrate-unit { font-size: 11px; color: #596674; }
.compact .quality-row { grid-template-columns: 3em minmax(0, 1fr); min-height: 42px; border-bottom: 0; font-size: 12px; }
.compact .quality-row select { height: 32px; }
.compact .bitrate-control { grid-template-columns: minmax(0, 1fr) 53px auto; gap: 5px; }
</style>
