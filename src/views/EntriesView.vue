<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import PageHeader from '../layout/PageHeader.vue';

const scanning = ref(false);
const desktopEntries = ref<any[]>([]);
const entrySearch = ref('');

const filteredEntries = computed(() => {
  if (!entrySearch.value) return desktopEntries.value;
  const q = entrySearch.value.toLowerCase();
  return desktopEntries.value.filter((e) => e.name.toLowerCase().includes(q) || e.basename.toLowerCase().includes(q));
});

async function loadDesktopEntries() {
  scanning.value = true;
  try {
    desktopEntries.value = await invoke('get_desktop_entries');
  } catch (e: any) {
    ElMessage.error('加载启动器失败: ' + e);
  } finally {
    scanning.value = false;
  }
}

async function handleToggleEntry(entry: any) {
  try {
    await invoke('toggle_desktop_visibility', { path: entry.path, visible: entry.visible });
    ElMessage.success(`已在菜单中${entry.visible ? '显示' : '隐藏'}: ${entry.name}`);
  } catch (e: any) {
    entry.visible = !entry.visible;
    ElMessage.error('修改失败: ' + e);
  }
}

onMounted(() => {
  loadDesktopEntries();
});
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="桌面与菜单图标">
      <template #actions>
        <el-input v-model="entrySearch" placeholder="搜索启动器名称..." clearable style="width: 260px" />
        <el-button @click="loadDesktopEntries" :loading="scanning">刷新</el-button>
      </template>
    </PageHeader>

    <div class="subpage-content">
      <el-table :data="filteredEntries" height="100%" stripe style="width: 100%" v-loading="scanning">
        <el-table-column prop="name" label="启动器名称" min-width="180" />
        <el-table-column prop="scope" label="作用域" width="90">
          <template #default="{ row }">
            <el-tag size="small">{{ row.scope }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="exec" label="运行命令" min-width="220" show-overflow-tooltip />
        <el-table-column prop="basename" label="文件名称" min-width="160" show-overflow-tooltip />
        <el-table-column label="应用菜单显示" width="130" align="center">
          <template #default="{ row }">
            <el-switch
              v-model="row.visible"
              active-color="#10b981"
              @change="handleToggleEntry(row)"
            />
          </template>
        </el-table-column>
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

