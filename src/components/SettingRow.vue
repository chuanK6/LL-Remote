<script setup lang="ts">
defineProps<{ label: string; detail?: string }>()
</script>
<template>
  <div class="setting-row"><div><div class="setting-name">{{ label }}</div><div v-if="detail" class="setting-detail">{{ detail }}</div></div><div class="setting-control"><slot /></div></div>
</template>
