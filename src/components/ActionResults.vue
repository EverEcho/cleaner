<script setup lang="ts">
import type { ActionReport } from '../cleanup';
import SummaryDetails from './SummaryDetails.vue';
defineProps<{ report: ActionReport | null }>();
</script>
<template>
  <SummaryDetails v-if="report" label="处理结果" :count="`${report.succeeded} 成功 / ${report.failed} 失败 / ${report.skipped} 跳过`" :tone="report.failed ? 'danger' : report.skipped ? 'warning' : undefined">
    <div v-for="(item, index) in report.items" :key="`${item.path}:${index}`" class="result" :class="item.status"><span>{{ item.status === 'success' ? '成功' : item.status === 'failed' ? '失败' : '跳过' }}</span><div><code :title="item.path">{{ item.path }}</code><p>{{ item.message }}</p></div></div>
  </SummaryDetails>
</template>
<style scoped>
.result { display: flex; gap: 12px; padding: 10px 0; border-bottom: 1px solid var(--line); } .result > div { flex: 1; min-width: 0; } .result > span { color: var(--success); min-width: 32px; } .result.failed > span { color: var(--danger); } .result.skipped > span { color: var(--warning); } code { font-size: 11px; } p { margin: 5px 0 0; color: var(--text-muted); font-size: 11px; }
</style>
