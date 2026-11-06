<script setup lang="ts">
import { onDeactivated, ref } from 'vue';
defineProps<{ label: string; count?: number | string; title?: string; tone?: 'warning' | 'danger' }>();
const opened = ref(false);
onDeactivated(() => { opened.value = false; });
</script>
<template>
  <span class="summary-detail">
    <button type="button" class="detail-trigger" aria-haspopup="dialog" :aria-expanded="opened" :class="tone" @click="opened = true">{{ label }}<strong v-if="count !== undefined">{{ count }}</strong></button>
    <el-dialog v-model="opened" :title="title || label" width="min(720px, calc(100vw - 40px))" append-to-body>
      <div class="detail-dialog-content"><slot /></div>
      <template #footer><el-button @click="opened = false">关闭</el-button></template>
    </el-dialog>
  </span>
</template>
<style scoped>
.summary-detail { display: inline-flex; min-width: 0; max-width: 100%; }
.detail-trigger { display: inline-flex; align-items: center; gap: 7px; max-width: 100%; padding: 4px 0; border: 0; background: transparent; color: var(--text-muted); font: inherit; font-size: 11px; cursor: pointer; white-space: nowrap; }
.detail-trigger:hover { color: var(--accent); }
.detail-trigger.warning { color: var(--warning); } .detail-trigger.danger { color: var(--danger); }
.summary-detail .detail-trigger strong { font-size: 11px; font-variant-numeric: tabular-nums; font-weight: 550; }
.detail-dialog-content { min-width: 0; max-height: 55vh; overflow-y: auto; overflow-x: hidden; color: var(--text-secondary); font-size: 12px; line-height: 1.7; overflow-wrap: anywhere; }
.detail-dialog-content :deep(p) { white-space: pre-line; }
.detail-dialog-content :deep(code) { display: block; min-width: 0; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.detail-dialog-content :deep(pre) { white-space: pre-wrap; overflow-wrap: anywhere; font-size: 11px; }
</style>
