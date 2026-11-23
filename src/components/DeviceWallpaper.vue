<script setup lang="ts">
import { ref, watch } from 'vue'
import { bridge } from '../bridge'
import wallpaper from '../assets/wallpaper.svg'
const props = defineProps<{ path?: string | null }>()
const source = ref(wallpaper)
watch(() => props.path, async path => { source.value = path ? await bridge.wallpaper(path).catch(() => wallpaper) : wallpaper }, { immediate: true })
</script>
<template><img :src="source" class="cover-art" alt="" draggable="false" /></template>
