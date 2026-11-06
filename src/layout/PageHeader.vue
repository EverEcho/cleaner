<script setup lang="ts">
import { onActivated, onDeactivated, onMounted, ref } from 'vue';
defineProps<{ title: string; subtitle?: string }>();
// KeepAlive retains page instances; only the visible page owns the title bar.
const active = ref(false);
onMounted(() => { active.value = true; });
onActivated(() => { active.value = true; });
onDeactivated(() => { active.value = false; });
</script>

<template>
  <Teleport v-if="active" to="#page-titlebar">
    <header class="subpage-header">
      <h1 :title="subtitle">{{ title }}</h1>
      <div v-if="$slots.actions" class="header-actions"><slot name="actions" /></div>
    </header>
  </Teleport>
</template>

<style scoped>
.subpage-header { display: flex; align-items: center; justify-content: space-between; gap: 16px; min-width: 0; }
h1 { font-size: 16px; font-weight: 600; color: var(--text); margin: 0; white-space: nowrap; }
.header-actions { display: flex; align-items: center; justify-content: flex-end; gap: 8px; min-width: 0; }
@media (max-width: 700px) { .subpage-header { flex-wrap: wrap; gap: 8px; } .header-actions { margin-left: auto; } }
</style>
