<script setup lang="ts">
defineProps<{ modelValue: boolean; label: string; disabled?: boolean }>()
defineEmits<{ 'update:modelValue': [value: boolean] }>()
</script>
<template><button type="button" :class="['toggle', { on: modelValue }]" role="switch" :aria-label="label" :aria-checked="modelValue" :disabled="disabled" @click="$emit('update:modelValue', !modelValue)"><span /></button></template>
