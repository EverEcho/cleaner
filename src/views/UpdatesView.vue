<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ElMessage, ElMessageBox } from 'element-plus';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';
import { openUrl } from '@tauri-apps/plugin-opener';

interface UpgradablePkg {
  name: string;
  manager: string;
  current_version: string;
  new_version: string;
  arch: string;
  repository: string;
}

import { platformInfo } from '../platform';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

const providerLabels: Record<string, string> = { 'github-release': '独立安装', 'github-branch': '源码安装', npm: 'npm', uv: 'uv', pipx: 'pipx' };
const checking = ref(false);
const refreshing = ref(false);
const currentLog = ref('正在连接系统软件源并检索最新元数据…');
const upgradingPkg = ref<string | null>(null);
interface ToolUpdate { id: string; name: string; provider: string; current_version: string; latest_version: string; status: string; install_path: string; source_url: string; update_hint: string; note: string }
interface UpdateReport { packages: UpgradablePkg[]; tools: ToolUpdate[]; warnings: string[] }
const updates = ref<UpgradablePkg[]>([]);
const tools = ref<ToolUpdate[]>([]);
const warnings = ref<string[]>([]);
const onlyAvailable = ref(false);
const toolStatuses: Record<string, { label: string; type: 'success' | 'warning' | 'info' | 'danger' }> = {
  'no-update': { label: '未发现更新', type: 'success' },
  available: { label: '有更新', type: 'warning' }, current: { label: '已是最新', type: 'success' },
  ahead: { label: '本地版本较新', type: 'info' }, unknown: { label: '需核对', type: 'info' }, failed: { label: '检测未完成', type: 'danger' },
};
const filteredTools = computed(() => tools.value.filter(tool => (!onlyAvailable.value || tool.status === 'available')
  && (filterManager.value === 'all' || tool.provider === filterManager.value)
  && (!search.value || `${tool.name} ${tool.install_path}`.toLowerCase().includes(search.value.toLowerCase()))));
const availableCount = computed(() => updates.value.length + tools.value.filter(tool => tool.status === 'available').length);
async function copyHint(tool: ToolUpdate) {
  try { await navigator.clipboard.writeText(tool.update_hint); ElMessage.success('更新方式已复制，请在终端按原安装渠道执行'); }
  catch { await ElMessageBox.alert(tool.update_hint, `${tool.name} 更新方式`, { confirmButtonText: '关闭' }); }
}
async function showSource(tool: ToolUpdate) {
  try { await openUrl(tool.source_url); } catch (error) { ElMessage.error('打开软件源失败: ' + error); }
}
const search = ref('');
const filterManager = ref('all');

const errorMessage = ref('');
let unlisten: UnlistenFn | undefined;
let disposed = false;
onUnmounted(() => { disposed = true; unlisten?.(); });

const availableManagers = computed(() => {
  const set = new Set([...updates.value.map(u => u.manager), ...tools.value.map(tool => tool.provider)]);
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
  errorMessage.value = '';
  currentLog.value = '正在检查系统软件源、全局工具包和独立安装软件…';
  try {
    const report: UpdateReport = await invoke('check_software_updates');
    updates.value = report.packages; tools.value = report.tools; warnings.value = report.warnings;
    if (!availableManagers.value.includes(filterManager.value)) filterManager.value = 'all';
    const incomplete = warnings.value.length || tools.value.some(tool => ['failed', 'unknown'].includes(tool.status));
    if (incomplete) ElMessage.warning(`已检测到 ${availableCount.value} 项更新，部分渠道需要核对`);
    else ElMessage.info(availableCount.value ? `检测到 ${availableCount.value} 项可用更新` : '已检查的安装渠道未发现更新');
  } catch (err: any) {
    errorMessage.value = String(err);
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
  listen<string>('update-log', event => { currentLog.value = event.payload; })
    .then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(() => {});
  checkUpdates();
});
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="软件更新检测">
      <template #actions>
        <el-input
          v-model="search"
          placeholder="搜索软件或安装路径..."
          clearable
          style="width: 180px"
        />
        <el-select v-model="filterManager" style="width: 140px">
          <el-option label="所有安装渠道" value="all" />
          <el-option
            v-for="mgr in availableManagers"
            :key="mgr"
            :label="providerLabels[mgr] ?? mgr.toUpperCase()"
            :value="mgr"
          />
        </el-select>
        <el-button :loading="refreshing" :disabled="checking || !!upgradingPkg" @click="refreshSources">
          刷新软件源
        </el-button>
        <el-button type="primary" :loading="checking" :disabled="refreshing || !!upgradingPkg" @click="checkUpdates">
          检查更新
        </el-button>
      </template>
    </PageHeader>

    <div class="subpage-content">
      <ScanningRadar
        v-if="checking || refreshing"
        :title="refreshing ? '正在刷新系统软件源索引…' : '正在检测软件与工具更新…'"
        :sub-title="currentLog"
        :tips="[
          '读取系统包管理器的候选版本…',
          '检查 npm 全局包与 uv/pipx 安装的工具…',
          '核对独立二进制与官方源码仓库…',
          '网络不可用时保留其他已完成的检测结果…',
        ]"
      />
      <div v-else-if="updates.length === 0 && tools.length === 0 && !warnings.length" class="empty-status">
        <el-icon :size="54" :color="errorMessage ? '#e6a23c' : '#00bd86'"><Warning v-if="errorMessage" /><CircleCheck v-else /></el-icon>
        <div class="empty-title">{{ errorMessage ? '检查更新未完成' : '当前软件源未发现可用更新' }}</div>
        <div class="empty-sub">{{ errorMessage || platformInfo?.updates_note }}</div>
      </div>

      <template v-else-if="!checking && !refreshing">
      <p class="update-summary">检测到 {{ availableCount }} 项更新 · 已识别 {{ tools.length }} 个全局或独立工具</p>
      <el-alert v-if="errorMessage" :title="errorMessage" type="error" :closable="false" />
      <details v-if="warnings.length" class="update-warnings"><summary>{{ warnings.length }} 个渠道检测未完成（查看原因）</summary><p v-for="warning in warnings" :key="warning">{{ warning }}</p></details>
      <h3>系统软件包 <span>{{ updates.length }} 项可更新</span></h3>
      <p v-if="!updates.length" class="update-note">{{ warnings.length ? '已完成检测的系统渠道未发现更新，未完成渠道见上方提示。' : '当前系统软件源未发现更新。' }}</p>
      <el-table
        v-else
        :data="filteredUpdates"
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
              :disabled="row.manager === 'pacman' || (!!upgradingPkg && upgradingPkg !== row.name)"
              @click="handleUpgrade(row)"
            >
              立即升级
            </el-button>
          </template>
        </el-table-column>
      </el-table>
      <div class="tool-heading"><h3>全局工具与独立软件 <span>{{ tools.length }} 项</span></h3><el-checkbox v-model="onlyAvailable">只看有更新</el-checkbox></div>
      <p class="update-note">按安装渠道检测；独立工具提供更新方式，检测过程不安装软件。源码安装显示提交版本。</p>
      <el-table :data="filteredTools" row-key="id" style="width: 100%">
        <el-table-column type="expand"><template #default="{ row }"><div class="tool-details"><p><b>安装位置：</b>{{ row.install_path || '由工具管理器维护' }}</p><p v-if="row.note">{{ row.note }}</p><p><b>更新方式：</b><code>{{ row.update_hint }}</code></p><el-button size="small" @click="showSource(row)">查看官方来源</el-button></div></template></el-table-column>
        <el-table-column prop="name" label="软件" min-width="165" show-overflow-tooltip />
        <el-table-column label="安装渠道" min-width="110"><template #default="{ row }">{{ providerLabels[row.provider] ?? row.provider }}</template></el-table-column>
        <el-table-column prop="current_version" label="当前版本 / 提交" min-width="150"><template #default="{ row }">{{ row.current_version || '无法确认' }}</template></el-table-column>
        <el-table-column prop="latest_version" label="上游版本 / 提交" min-width="150"><template #default="{ row }">{{ row.latest_version || '未获取' }}</template></el-table-column>
        <el-table-column label="状态" width="130"><template #default="{ row }"><el-tag :type="toolStatuses[row.status]?.type ?? 'info'" size="small">{{ toolStatuses[row.status]?.label ?? '需核对' }}</el-tag></template></el-table-column>
        <el-table-column label="操作" width="125"><template #default="{ row }"><el-button size="small" @click="copyHint(row)">复制更新方式</el-button></template></el-table-column>
      </el-table>
      </template>
    </div>
  </div>
</template>

<style scoped>
h3 { font-size: 14px; color: #4e555a; font-weight: 500; margin: 22px 0 12px; }
h3 span, .update-note { color: #92989c; font-size: 12px; font-weight: 400; }
.update-summary { font-size: 14px; color: #4e555a; }
.update-warnings { font-size: 12px; color: #b58024; padding: 12px 0; }
.update-warnings summary { cursor: pointer; }
.tool-heading { display: flex; align-items: center; justify-content: space-between; }
.tool-details { padding: 8px 24px; font-size: 12px; color: #798187; overflow-wrap: anywhere; }
.tool-details code { color: #4e555a; user-select: text; }

.subpage-container {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #f9fafb;
}

.subpage-content {
  flex: 1;
  padding: 20px;
  overflow: auto;
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

