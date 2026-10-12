<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ElMessage, ElMessageBox } from 'element-plus';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';

interface UpgradablePkg {
  name: string;
  manager: string;
  current_version: string;
  new_version: string;
  arch: string;
  repository: string;
}

import { listen } from '@tauri-apps/api/event';

const checking = ref(false);
const refreshing = ref(false);
const currentLog = ref('正在连接系统软件源并检索最新元数据…');
const upgradingPkg = ref<string | null>(null);
const updates = ref<UpgradablePkg[]>([]);
const search = ref('');
const filterManager = ref('all');

// 监听 Rust 后端广播的实时日志与进度
listen<string>('update-log', (event) => {
  currentLog.value = event.payload;
});

const availableManagers = computed(() => {
  const set = new Set(updates.value.map((u) => u.manager));
  return Array.from(set);
});

const filteredUpdates = computed(() => {
  return updates.value.filter((pkg) => {
    const matchMgr = filterManager.value === 'all' || pkg.manager === filterManager.value;
    const matchSearch =
      !search.value ||
      pkg.name.toLowerCase().includes(search.value.toLowerCase()) ||
      pkg.repository.toLowerCase().includes(search.value.toLowerCase());
    return matchMgr && matchSearch;
  });
});

async function checkUpdates() {
  checking.value = true;
  currentLog.value = '正在跨包管理器检测可用更新…';
  try {
    const list: UpgradablePkg[] = await invoke('check_software_updates');
    updates.value = list;
    if (list.length === 0) {
      ElMessage.success('系统软件已是最新版本！');
    } else {
      ElMessage.info(`检测到 ${list.length} 个可更新的软件包`);
    }
  } catch (err: any) {
    ElMessage.error('检查更新失败: ' + err);
  } finally {
    checking.value = false;
  }
}

async function refreshSources() {
  refreshing.value = true;
  currentLog.value = '正在请求管理员授权并更新软件源索引…';
  try {
    const msg: string = await invoke('refresh_software_sources');
    ElMessage.success(msg);
    await checkUpdates();
  } catch (err: any) {
    ElMessage.error('刷新源失败: ' + err);
  } finally {
    refreshing.value = false;
  }
}

async function handleUpgrade(pkg: UpgradablePkg) {
  try {
    await ElMessageBox.confirm(
      `确定通过 ${pkg.manager.toUpperCase()} 升级软件包 [${pkg.name}] 至版本 ${pkg.new_version} 吗？系统可能请求管理员授权。`,
      '升级确认',
      { confirmButtonText: '确定升级', cancelButtonText: '取消', type: 'info' }
    );

    upgradingPkg.value = pkg.name;
    const msg: string = await invoke('upgrade_software', {
      manager: pkg.manager,
      name: pkg.name,
    });
    ElMessage.success(msg);
    await checkUpdates();
  } catch (err: any) {
    if (err !== 'cancel') {
      ElMessage.error('升级失败: ' + err);
    }
  } finally {
    upgradingPkg.value = null;
  }
}

onMounted(() => {
  checkUpdates();
});
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="软件更新检测 (APT / DNF / YUM / Pacman / Flatpak)">
      <template #actions>
        <el-input
          v-model="search"
          placeholder="搜索待更新软件..."
          clearable
          style="width: 220px"
        />
        <el-select v-model="filterManager" style="width: 140px">
          <el-option label="所有包管理器" value="all" />
          <el-option
            v-for="mgr in availableManagers"
            :key="mgr"
            :label="mgr.toUpperCase()"
            :value="mgr"
          />
        </el-select>
        <el-button :loading="refreshing" @click="refreshSources">
          刷新软件源
        </el-button>
        <el-button type="primary" :loading="checking" @click="checkUpdates">
          检查更新
        </el-button>
      </template>
    </PageHeader>

    <div class="subpage-content">
      <ScanningRadar
        v-if="checking || refreshing"
        :title="refreshing ? '正在刷新系统软件源索引…' : '正在跨包管理器检测可用更新…'"
        :sub-title="currentLog"
        :tips="[
          '正在向各大镜像源拉取 Release / InRelease 签名索引…',
          '正在检查并验证第三方存储库可用性…',
          '正在计算本地已安装版本与仓库候选版本的更新拓扑…',
          '正在排查安全漏洞修复包与关键补丁…',
        ]"
      />
      <div v-else-if="updates.length === 0" class="empty-status">
        <el-icon :size="54" color="#10b981"><CircleCheck /></el-icon>
        <div class="empty-title">当前所有软件已是最新版本</div>
        <div class="empty-sub">支持自动检测 apt、yum/dnf、pacman、zypper 和 flatpak 包管理器</div>
      </div>

      <el-table
        v-else
        :data="filteredUpdates"
        height="100%"
        stripe
        style="width: 100%"
      >
        <el-table-column prop="name" label="软件包名称" min-width="180" />
        <el-table-column prop="manager" label="包管理器" width="110">
          <template #default="{ row }">
            <el-tag
              :type="row.manager === 'apt' ? 'primary' : row.manager === 'flatpak' ? 'success' : 'warning'"
              size="small"
            >
              {{ row.manager.toUpperCase() }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="current_version" label="当前版本" min-width="160" show-overflow-tooltip>
          <template #default="{ row }">
            <span class="old-ver">{{ row.current_version || '已安装' }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="new_version" label="可更新版本" min-width="160" show-overflow-tooltip>
          <template #default="{ row }">
            <span class="new-ver">{{ row.new_version }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="repository" label="软件源 / 仓库" min-width="140" show-overflow-tooltip />
        <el-table-column label="操作" width="110" fixed="right">
          <template #default="{ row }">
            <el-button
              type="primary"
              size="small"
              :loading="upgradingPkg === row.name"
              @click="handleUpgrade(row)"
            >
              立即升级
            </el-button>
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

.old-ver {
  color: #6b7280;
}

.new-ver {
  color: #059669;
  font-weight: 600;
}

.empty-status {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
}

.empty-title {
  font-size: 18px;
  font-weight: 600;
  color: #111827;
}

.empty-sub {
  font-size: 13px;
  color: #9ca3af;
}
</style>

