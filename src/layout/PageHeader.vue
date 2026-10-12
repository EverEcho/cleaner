<script setup lang="ts">
import { useRouter } from 'vue-router';
import LemonMark from './LemonMark.vue';
defineProps<{ title: string }>();
const router = useRouter();
</script>
<template>
  <header class="subpage-header">
    <div class="header-left"><div class="page-mark"><LemonMark /></div><div class="page-heading"><h1>{{ title }}</h1><button class="back-button" @click="router.push('/')">返回首页</button></div></div>
    <div class="header-actions"><slot name="actions" /></div>
  </header>
</template>
<style scoped>
.subpage-header { min-height: 92px; flex-shrink: 0; display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 10px 28px 14px; background: #fff; border-bottom: 1px solid #efefef; flex-wrap: wrap; }
.header-left { display: flex; align-items: center; gap: 12px; flex-shrink: 0; }
.page-mark { width: 64px; height: 64px; }
h1 { font-size: 22px; font-weight: 400; color: #303236; margin: 0 0 7px; }
.back-button { border: 0; background: transparent; color: #a0a4aa; font-size: 11px; padding: 0; cursor: pointer; }
.back-button:hover { color: #00bd86; }
.header-actions { display: flex; align-items: center; gap: 9px; flex-wrap: wrap; justify-content: flex-end; }
</style>
