<script setup lang="ts">
import type { ActionReport } from '../cleanup';
defineProps<{ report: ActionReport | null }>();
</script>
<template>
  <details v-if="report" class="action-results" :open="report.failed > 0">
    <summary>处理结果 · {{ report.succeeded }} 成功 / {{ report.failed }} 失败 / {{ report.skipped }} 跳过</summary>
    <div v-for="(item, index) in report.items" :key="`${item.path}:${index}`" class="result" :class="item.status"><span>{{ item.status === 'success' ? '成功' : item.status === 'failed' ? '失败' : '跳过' }}</span><div><code>{{ item.path }}</code><p>{{ item.message }}</p></div></div>
  </details>
</template>
<style scoped>
.action-results { margin: 14px 0; padding: 14px 16px; background: #ffffff04; border: 1px solid #ffffff14; border-radius: 14px; color: #c9bfdc; font-size: 12px; }
summary { cursor: pointer; } .result { display: flex; gap: 12px; padding: 10px 0; border-top: 1px solid #ffffff08; margin-top: 9px; } .result > span { color: #74cfb2; min-width: 32px; } .result.failed > span { color: #f2b2a2; } .result.skipped > span { color: #cdb888; } code { overflow-wrap: anywhere; font-size: 11px; } p { margin: 5px 0 0; color: #998fac; font-size: 11px; }
</style>
