<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';

const scanning = ref(true);
const largeFiles = ref<any[]>([]);

async function loadLargeFiles() {
  scanning.value = true;
  try {
    largeFiles.value = await invoke('scan_large_files', { minMb: 50 });
  } catch (e: any) {
    ElMessage.error('扫描大文件失败: ' + e);
  } finally {
    scanning.value = false;
  }
}

onMounted(() => {
  loadLargeFiles();
});
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="大文件清理 (LemonBigOldFile > 50MB)">
      <template #actions>
        <el-button @click="loadLargeFiles" :loading="scanning">重新扫描</el-button>
      </template>
    </PageHeader>

    <div class="subpage-content">
      <ScanningRadar
        v-if="scanning"
        title="正在检索磁盘大文件…"
        sub-title="深度遍历 Downloads, Documents, Videos 等目录"
        :tips="[
          '正在递归遍历常用工作目录…',
          '正在分析文件体积元数据…',
          '正在过滤排除 node_modules 与工程缓存…',
          '正在按文件体积大小进行降序排列…',
        ]"
      />
      <el-table v-else :data="largeFiles" height="100%" stripe style="width: 100%">
        <el-table-column prop="name" label="文件名" min-width="200" show-overflow-tooltip />
        <el-table-column prop="size_str" label="大小" width="120" sortable />
        <el-table-column prop="ext" label="格式" width="90" />
        <el-table-column prop="path" label="路径" min-width="260" show-overflow-tooltip />
      </el-table>
    </div>
  </div>
</template>

<style scoped>
.subpage-container {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #f9fafb;
}

.subpage-content {
  flex: 1;
  padding: 20px;
  overflow: hidden;
}
</style>

