<script setup lang="ts">defineProps<{ title: string; description?: string }>()</script>
<template><h1 class="page-title">{{ title }}</h1><p v-if="description" class="page-description">{{ description }}</p></template>
