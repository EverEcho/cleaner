<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';

const scanning = ref(true);
const duplicateGroups = ref<any[]>([]);

async function loadDuplicates() {
  scanning.value = true;
  try {
    duplicateGroups.value = await invoke('scan_duplicate_files');
  } catch (e: any) {
    ElMessage.error('扫描重复文件失败: ' + e);
  } finally {
    scanning.value = false;
  }
}

onMounted(() => {
  loadDuplicates();
});
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="重复文件清理 (LemonDuplicateFile SHA-256)">
      <template #actions>
        <el-button @click="loadDuplicates" :loading="scanning">重新比对</el-button>
      </template>
    </PageHeader>

    <div class="subpage-content">
      <ScanningRadar
        v-if="scanning"
        title="正在智能计算文件 SHA-256 哈希比对…"
        sub-title="先按体积粗筛聚类，再进行精确二进制哈希比对"
        :tips="[
          '正在枚举 Downloads, Documents, Pictures…',
          '第一步：按文件字节大小精确归类分组…',
          '第二步：逐个计算同体积文件的 SHA-256 指纹…',
          '正在确认完全相同的多份内容副本…',
        ]"
      />
      <template v-else>
        <div v-if="duplicateGroups.length === 0" class="empty-tip">
          暂未发现完全重复的文件，系统很整洁！
        </div>
        <div v-for="g in duplicateGroups" :key="g.hash" class="dup-group-card">
          <div class="dup-header">
            <span>文件哈希: <code>{{ g.hash.substring(0, 20) }}...</code></span>
            <el-tag size="small" type="danger">单文件大小: {{ g.size_str }} (共 {{ g.files.length }} 份)</el-tag>
          </div>
          <ul class="dup-files-list">
            <li v-for="f in g.files" :key="f">{{ f }}</li>
          </ul>
        </div>
      </template>
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
  overflow-y: auto;
}

.empty-tip {
  text-align: center;
  padding: 60px;
  color: #9ca3af;
  font-size: 15px;
}

.dup-group-card {
  background: #ffffff;
  border-radius: 8px;
  padding: 16px;
  margin-bottom: 16px;
  border: 1px solid #e5e7eb;
}

.dup-header {
  display: flex;
  justify-content: space-between;
  margin-bottom: 10px;
  font-size: 13px;
  color: #6b7280;
}

.dup-files-list {
  list-style: square;
  margin: 0;
  padding-left: 20px;
  font-size: 13px;
  color: #374151;
}
</style>

