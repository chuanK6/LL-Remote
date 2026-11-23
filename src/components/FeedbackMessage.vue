<script setup lang="ts">defineProps<{ message?: string; kind?: 'warning' | 'error' | 'success' | '' }>()</script>
<template><div v-if="message || $slots.default" :class="['inline-message', kind]" :role="kind === 'error' ? 'alert' : 'status'"><slot>{{ message }}</slot></div></template>
