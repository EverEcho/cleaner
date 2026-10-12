<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import { platformInfo } from '../platform';
import PageHeader from '../layout/PageHeader.vue';

const scanning = ref(false);
const autostartList = ref<any[]>([]);

async function loadAutostart() {
  scanning.value = true;
  try {
    autostartList.value = await invoke('get_autostart_items');
  } catch (e: any) {
    ElMessage.error('加载开机启动项失败: ' + e);
  } finally {
    scanning.value = false;
  }
}

async function handleToggleAutostart(item: any) {
  try {
    await invoke('toggle_autostart_item', { path: item.path, enable: item.enabled });
    await loadAutostart();
    ElMessage.success(`已${item.enabled ? '启用' : '禁用'}启动项: ${item.name}`);
  } catch (e: any) {
    item.enabled = !item.enabled;
    ElMessage.error('修改失败: ' + e);
  }
}

onMounted(() => {
  loadAutostart();
});
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="开机启动项管理">
      <template #actions>
        <el-button @click="loadAutostart" :loading="scanning">刷新</el-button>
      </template>
    </PageHeader>

    <div class="subpage-content">
      <p class="platform-note">{{ platformInfo?.autostart_note }}</p>
      <el-table :data="autostartList" height="100%" stripe style="width: 100%" v-loading="scanning">
        <el-table-column prop="name" label="启动项名称" min-width="160" />
        <el-table-column prop="exec" label="执行命令" min-width="240" show-overflow-tooltip />
        <el-table-column prop="is_user" label="级别" width="100">
          <template #default="{ row }">
            <el-tag size="small" :type="row.is_user ? 'info' : 'warning'">
              {{ row.is_user ? '用户级' : '系统级' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="开机启动状态" width="140" align="center">
          <template #default="{ row }">
            <el-switch
              v-model="row.enabled"
              :disabled="platformInfo?.os === 'macos' && !row.is_user"
              active-color="#10b981"
              @change="handleToggleAutostart(row)"
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

