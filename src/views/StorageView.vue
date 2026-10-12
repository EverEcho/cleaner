<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';

interface Usage { path: string; size_bytes: number; size: string }
const roots = ref<string[]>([]);
const root = ref('');
const directories = ref<Usage[]>([]);
const warning = ref('');
const scanning = ref(false);
const total = computed(() => directories.value.reduce((sum, row) => sum + row.size_bytes, 0));
async function analyze(path = root.value) {
  if (!path || scanning.value) return;
  root.value = path;
  scanning.value = true;
  directories.value = [];
  warning.value = '';
  try {
    const result = await invoke<{ directories: Usage[]; warning: string }>('analyze_disk', { root: path });
    directories.value = result.directories;
    warning.value = result.warning;
  } catch (error) { ElMessage.error(String(error)); }
  finally { scanning.value = false; }
}
function drillDown(row: Usage) { void analyze(row.path); }
onMounted(async () => {
  try {
    roots.value = await invoke<string[]>('get_storage_roots');
    root.value = roots.value[0] ?? '';
    await analyze();
  } catch (error) { ElMessage.error(String(error)); }
});
</script>

<template>
  <div class="storage-page">
    <PageHeader title="磁盘空间分析">
      <template #actions>
        <el-select v-model="root" filterable allow-create style="width: 250px" :disabled="scanning">
          <el-option v-for="path in roots" :key="path" :label="path" :value="path" />
        </el-select>
        <el-button type="primary" :loading="scanning" @click="analyze()">分析目录</el-button>
      </template>
    </PageHeader>
    <div class="storage-content">
      <ScanningRadar v-if="scanning" title="正在分析目录占用…" :tips="['统计当前目录下的存储占用…', '扫描时不跨越其他文件系统…', '按占用空间排列文件夹…']" />
      <template v-else>
        <p class="hint">先找到占用较大的目录，再到系统垃圾清理中按应用或系统规则清理。点击目录可继续分析。</p>
        <el-alert v-if="warning" :title="warning" type="warning" :closable="false" />
        <el-table :data="directories" @row-click="drillDown">
          <el-table-column prop="path" label="目录" min-width="300" show-overflow-tooltip />
          <el-table-column label="空间占比" min-width="160"><template #default="{ row }"><el-progress :percentage="total ? Math.round(row.size_bytes / total * 100) : 0" :stroke-width="6" color="#00c98d" /></template></el-table-column>
          <el-table-column prop="size" label="占用空间" width="120" />
        </el-table>
      </template>
    </div>
  </div>
</template>

<style scoped>
.storage-page { display: flex; flex-direction: column; height: 100%; background: #fff; }
.storage-content { padding: 14px 24px; flex: 1; min-height: 0; overflow: auto; }
.hint { font-size: 13px; color: #9599a0; line-height: 1.6; }
</style>
