<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ElMessage, ElMessageBox } from 'element-plus';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';

interface AppItem {
  key: string;
  name: string;
  pkg_type: string;
  version: string;
  size: string;
  size_bytes: number;
  install_time: string;
  description: string;
  icon: string;
  desktop_file: string;
  is_system: boolean;
  has_ui: boolean;
}

const scanning = ref(true);
const apps = ref<AppItem[]>([]);
const appSearch = ref('');
const activeCategory = ref<'user' | 'all'>('user'); // 默认只看用户应用，防止卡死与误卸

const filteredApps = computed(() => {
  return apps.value.filter((a) => {
    // 分类筛选
    if (activeCategory.value === 'user' && !a.has_ui) {
      return false;
    }
    // 关键词搜索
    if (appSearch.value) {
      const q = appSearch.value.toLowerCase();
      return (
        a.name.toLowerCase().includes(q) ||
        a.key.toLowerCase().includes(q) ||
        a.description.toLowerCase().includes(q)
      );
    }
    return true;
  });
});

const userAppsCount = computed(() => apps.value.filter((a) => a.has_ui).length);
const allAppsCount = computed(() => apps.value.length);

async function loadApps() {
  scanning.value = true;
  try {
    apps.value = await invoke('get_installed_apps');
  } catch (e: any) {
    ElMessage.error('加载应用列表失败: ' + e);
  } finally {
    scanning.value = false;
  }
}

async function handleUninstallApp(app: AppItem) {
  try {
    const warningMsg = app.is_system
      ? `警告：[${app.name}] 属于系统重要组件！卸载可能导致依赖该组件的应用异常。确定仍要卸载吗？`
      : `确定卸载应用程序 [${app.name}] (${app.pkg_type.toUpperCase()}) 吗？系统将自动清理其孤立依赖。`;

    await ElMessageBox.confirm(warningMsg, '确认卸载应用', {
      confirmButtonText: '立即卸载',
      cancelButtonText: '取消',
      type: app.is_system ? 'warning' : 'info',
    });

    const msg = await invoke('uninstall_app', { pkgType: app.pkg_type, key: app.key });
    ElMessage.success(String(msg));
    await loadApps();
  } catch (e: any) {
    if (e !== 'cancel') ElMessage.error('卸载失败: ' + e);
  }
}

onMounted(() => {
  loadApps();
});
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="应用强力卸载 (Lemon Uninstaller)">
      <template #actions>
        <el-radio-group v-model="activeCategory" size="default">
          <el-radio-button value="user">
            用户桌面应用 ({{ userAppsCount }})
          </el-radio-button>
          <el-radio-button value="all">
            全部手动安装项 ({{ allAppsCount }})
          </el-radio-button>
        </el-radio-group>
        <el-input
          v-model="appSearch"
          placeholder="搜索应用或软件包…"
          clearable
          style="width: 220px"
        />
        <el-button @click="loadApps" :loading="scanning">刷新</el-button>
      </template>
    </PageHeader>

    <div class="subpage-content">
      <!-- 扫描中的雷达波纹动画，消除卡死感 -->
      <ScanningRadar
        v-if="scanning"
        title="正在智能分析已安装应用…"
        sub-title="正在过滤底层依赖库，提取用户图形应用"
        :tips="[
          '正在枚举已安装软件包列表…',
          '正在匹配桌面菜单启动项…',
          '正在隔离过滤系统自动依赖库，保护系统稳定…',
          '正在计算应用体积与版本信息…',
        ]"
      />

      <div v-else class="table-container">
        <el-table :data="filteredApps" height="100%" stripe style="width: 100%">
          <el-table-column label="应用名称" min-width="200">
            <template #default="{ row }">
              <div class="app-name-cell">
                <span class="app-display-name">{{ row.name }}</span>
                <span class="app-pkg-name">{{ row.key }}</span>
              </div>
            </template>
          </el-table-column>

          <el-table-column prop="pkg_type" label="类型" width="100" align="center">
            <template #default="{ row }">
              <el-tag
                :type="row.pkg_type === 'deb' ? 'primary' : row.pkg_type === 'flatpak' ? 'success' : 'warning'"
                size="small"
              >
                {{ row.pkg_type.toUpperCase() }}
              </el-tag>
            </template>
          </el-table-column>

          <el-table-column label="属性标签" width="130">
            <template #default="{ row }">
              <el-tag v-if="row.has_ui" size="small" type="success" effect="plain">桌面应用</el-tag>
              <el-tag v-else size="small" type="info" effect="plain">终端命令行</el-tag>
            </template>
          </el-table-column>

          <el-table-column prop="version" label="版本" min-width="140" show-overflow-tooltip />
          <el-table-column prop="size" label="占用空间" width="110" />
          <el-table-column prop="description" label="描述" min-width="220" show-overflow-tooltip />

          <el-table-column label="操作" width="100" fixed="right" align="center">
            <template #default="{ row }">
              <el-button type="danger" link @click="handleUninstallApp(row)">
                强力卸载
              </el-button>
            </template>
          </el-table-column>
        </el-table>
      </div>
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
  position: relative;
}

.table-container {
  height: 100%;
}

.app-name-cell {
  display: flex;
  flex-direction: column;
}

.app-display-name {
  font-weight: 500;
  color: #1f2937;
}

.app-pkg-name {
  font-size: 11px;
  color: #9ca3af;
}
</style>
