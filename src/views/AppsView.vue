<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Package } from 'lucide-vue-next';
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
  can_uninstall: boolean;
  install_path: string;
  uninstall_note: string;
}

const failedIcons = ref(new Set<string>());
function iconFailed(key: string) { failedIcons.value = new Set([...failedIcons.value, key]); }
interface UninstallItem { path: string; name: string; kind: string; size_bytes: number; size: string; selected: boolean; required: boolean }
interface UninstallPlan { id: string; name: string; note: string; items: UninstallItem[] }
const uninstallDialog = ref(false);
const planLoading = ref(false);
const uninstalling = ref(false);
interface UninstallProgress { request_id: string; message: string; percentage: number | null; done?: number; total?: number }
const uninstallProgress = ref<UninstallProgress>({ request_id: '', message: '', percentage: null });
const uninstallName = ref('');
let unlistenProgress: UnlistenFn | undefined;
let disposed = false;
function beginUninstall(name: string) {
  uninstalling.value = true; uninstallName.value = name;
  const requestId = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  uninstallProgress.value = { request_id: requestId, message: '正在核对卸载范围…', percentage: null };
  return requestId;
}
function appRowKey(app: AppItem) { return `${app.pkg_type}:${app.key}`; }
function removeApp(app: AppItem) { apps.value = apps.value.filter(row => appRowKey(row) !== appRowKey(app)); }
function removeManualApps() {
  const targets = uninstallPlan.value?.items.filter(item => ['program','ide'].includes(item.kind) && uninstallPaths.value.includes(item.path)).map(item => item.path) ?? [];
  const key = manualApp.value ? appRowKey(manualApp.value) : '';
  apps.value = apps.value.filter(row => appRowKey(row) !== key && !targets.some(path => row.install_path && (row.install_path === path || row.install_path.startsWith(path + '/'))));
}
const uninstallPlan = ref<UninstallPlan | null>(null);
const manualApp = ref<AppItem | null>(null);
const uninstallPaths = ref<string[]>([]);
const selectedManagedIdes = computed(() => uninstallPlan.value?.items.filter(item => item.kind === 'ide' && uninstallPaths.value.includes(item.path)).length ?? 0);
const selectedUninstallSize = computed(() => {
  const bytes = uninstallPlan.value?.items.filter(item => uninstallPaths.value.includes(item.path)).reduce((sum,item) => sum + item.size_bytes,0) ?? 0;
  return bytes >= 1024 ** 3 ? (bytes / 1024 ** 3).toFixed(2) + ' GB' : (bytes / 1024 ** 2).toFixed(1) + ' MB';
});
async function loadUninstallPlan(app: AppItem) {
  manualApp.value = app; uninstallDialog.value = true; planLoading.value = true; uninstallPlan.value = null;
  try {
    uninstallPlan.value = await invoke<UninstallPlan>('get_manual_uninstall_plan', { key: app.key });
    uninstallPaths.value = uninstallPlan.value.items.filter(item => item.selected || item.required).map(item => item.path);
  } catch (error) { ElMessage.error('卸载详情读取失败: ' + error); }
  finally { planLoading.value = false; }
}
function selectPlanItem(item: UninstallItem, checked: boolean) {
  if (item.required) return;
  uninstallPaths.value = checked ? [...new Set([...uninstallPaths.value,item.path])] : uninstallPaths.value.filter(path => path !== item.path);
}
async function confirmManualUninstall() {
  if (!uninstallPlan.value || uninstalling.value) return;
  const requestId = beginUninstall(uninstallPlan.value.name);
  try {
    const message = await invoke<string>('uninstall_manual_app', { planId: uninstallPlan.value.id, paths: uninstallPaths.value, requestId });
    uninstallProgress.value.percentage = 100;
    removeManualApps();
    ElMessage.success(message); uninstallDialog.value = false;
  } catch (error) { ElMessage.error('卸载未完成: ' + error); }
  finally { uninstalling.value = false; }
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
  if (uninstalling.value || planLoading.value) return;
  if (app.pkg_type === 'manual') { await loadUninstallPlan(app); return; }
  try {
    const warningMsg = app.is_system
      ? `警告：[${app.name}] 属于系统重要组件！卸载可能导致依赖该组件的应用异常。确定仍要卸载吗？`
      : `确定卸载应用程序 [${app.name}] (${app.pkg_type.toUpperCase()}) 吗？应用文档与配置不会自动删除。`;

    await ElMessageBox.confirm(warningMsg, '确认卸载应用', {
      confirmButtonText: '立即卸载',
      cancelButtonText: '取消',
      type: app.is_system ? 'warning' : 'info',
    });

    const requestId = beginUninstall(app.name);
    const msg = await invoke('uninstall_app', { pkgType: app.pkg_type, key: app.key, requestId });
    uninstallProgress.value.percentage = 100;
    removeApp(app);
    ElMessage.success(String(msg));
  } catch (e: any) {
    if (e !== 'cancel' && e !== 'close') ElMessage.error('卸载失败: ' + e);
  } finally { uninstalling.value = false; }
}

onMounted(async () => {
  if (isTauri()) {
    try {
      const stop = await listen<UninstallProgress>('uninstall-progress', event => {
        if (event.payload.request_id === uninstallProgress.value.request_id) uninstallProgress.value = event.payload;
      });
      if (disposed) { stop(); return; }
      unlistenProgress = stop;
    } catch {}
  }
  if (!disposed) await loadApps();
});
onUnmounted(() => { disposed = true; unlistenProgress?.(); });
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="应用卸载">
      <template #actions>
        <el-radio-group v-model="activeCategory" size="default">
          <el-radio-button value="user">
            用户桌面应用 ({{ userAppsCount }})
          </el-radio-button>
          <el-radio-button value="all">
            全部应用及工具 ({{ allAppsCount }})
          </el-radio-button>
        </el-radio-group>
        <el-input
          v-model="appSearch"
          placeholder="搜索应用或软件包…"
          clearable
          style="width: 220px"
        />
        <el-button @click="loadApps" :loading="scanning" :disabled="uninstalling">刷新</el-button>
      </template>
    </PageHeader>

    <div class="subpage-content">
      <div v-if="uninstalling && !uninstallDialog" class="uninstall-progress" role="status"><div><strong>正在卸载 {{ uninstallName }}</strong><span v-if="uninstallProgress.total">{{ uninstallProgress.done ?? 0 }} / {{ uninstallProgress.total }}</span></div><el-progress :percentage="uninstallProgress.percentage ?? 100" :indeterminate="uninstallProgress.percentage === null" :show-text="uninstallProgress.percentage !== null" /><p :title="uninstallProgress.message">{{ uninstallProgress.message }}</p></div>
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
        <el-table :data="filteredApps" :row-key="appRowKey" height="100%" stripe style="width: 100%">
          <el-table-column label="应用名称" min-width="250">
            <template #default="{ row }">
              <div class="app-identity">
                <img v-if="row.icon.startsWith('data:image/') && !failedIcons.has(row.key)" :src="row.icon" alt="" class="app-icon" @error="iconFailed(row.key)" />
                <span v-else class="app-icon app-icon-fallback"><Package :size="23" /></span>
              <div class="app-name-cell">
                <span class="app-display-name">{{ row.name }}</span>
                <span class="app-pkg-name" :title="row.install_path || row.key">{{ row.install_path || row.key }}</span>
              </div></div>
            </template>
          </el-table-column>

          <el-table-column prop="pkg_type" label="类型" width="100" align="center">
            <template #default="{ row }">
              <el-tag
                :type="row.pkg_type === 'deb' ? 'primary' : row.pkg_type === 'flatpak' ? 'success' : 'warning'"
                size="small"
              >
                {{ row.pkg_type === 'manual' ? '独立应用' : row.pkg_type.toUpperCase() }}
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
              <el-tooltip :content="row.uninstall_note || (row.is_system ? '系统组件受保护' : '卸载应用')" placement="left"><span><el-button type="danger" :disabled="row.is_system || row.can_uninstall === false || uninstalling || planLoading" link @click="handleUninstallApp(row)">卸载</el-button></span></el-tooltip>
            </template>
          </el-table-column>
        </el-table>
      </div>
    </div>
    <el-dialog v-model="uninstallDialog" :title="`${manualApp?.name ?? '应用'} · 卸载详情`" width="780px" :close-on-click-modal="false" :close-on-press-escape="!uninstalling && !planLoading" :show-close="!uninstalling && !planLoading">
      <div v-loading="planLoading" class="uninstall-preview">
        <div v-if="uninstalling" class="uninstall-progress" role="status"><div><strong>正在卸载 {{ uninstallName }}</strong><span v-if="uninstallProgress.total">{{ uninstallProgress.done ?? 0 }} / {{ uninstallProgress.total }}</span></div><el-progress :percentage="uninstallProgress.percentage ?? 100" :indeterminate="uninstallProgress.percentage === null" :show-text="uninstallProgress.percentage !== null" /><p :title="uninstallProgress.message">{{ uninstallProgress.message }}</p></div>
        <template v-if="uninstallPlan">
          <p class="uninstall-note">{{ uninstallPlan.note }}</p>
          <p class="uninstall-summary">已选择 {{ uninstallPaths.length }} 项 · {{ selectedUninstallSize }}<span v-if="selectedManagedIdes"> · 包含 {{ selectedManagedIdes }} 个 IDE</span></p>
          <div class="uninstall-items"><div v-for="item in uninstallPlan.items" :key="item.path" class="uninstall-item">
            <el-checkbox :model-value="uninstallPaths.includes(item.path)" :disabled="item.required || uninstalling" :aria-label="item.name" @change="selectPlanItem(item, Boolean($event))" />
            <div class="uninstall-item-copy"><strong>{{ item.name }}<el-tag v-if="item.kind === 'ide'" size="small" type="warning">IDE</el-tag></strong><span>{{ item.path }}</span></div><span class="uninstall-size">{{ item.size }}</span>
          </div></div>
        </template>
        <p v-else-if="!planLoading" class="uninstall-note">无法生成卸载详情，请刷新后重试。</p>
      </div>
      <template #footer><el-button :disabled="uninstalling || planLoading" @click="uninstallDialog = false">取消</el-button><el-button :disabled="uninstalling || planLoading || !manualApp" @click="manualApp && loadUninstallPlan(manualApp)">刷新详情</el-button><el-button type="danger" :loading="uninstalling" :disabled="!uninstallPlan || planLoading" @click="confirmManualUninstall">确认移入废纸篓</el-button></template>
    </el-dialog>
  </div>
</template>

<style scoped>
.uninstall-progress { padding: 12px 16px; margin-bottom: 12px; border: 1px solid #e0f1e9; background: #f5fcf8; border-radius: 3px; }
.uninstall-progress > div:first-child { display: flex; justify-content: space-between; margin-bottom: 9px; font-size: 12px; }
.uninstall-progress strong { font-weight: 500; color: #25846a; }
.uninstall-progress p { margin: 7px 0 0; font-size: 11px; color: #919a97; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.uninstall-preview { min-height: 110px; }
.uninstall-note { color: #92989e; font-size: 12px; line-height: 1.8; }
.uninstall-summary { color: #e5a12e; font-size: 13px; }
.uninstall-items { max-height: 330px; overflow: auto; }
.uninstall-item { display: flex; gap: 12px; align-items: center; padding: 10px 4px; border-top: 1px solid #f0f0f0; }
.uninstall-item-copy { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 5px; }
.uninstall-item-copy strong { display: flex; gap: 8px; align-items: center; font-size: 13px; font-weight: 500; }
.uninstall-item-copy span { font-size: 11px; color: #9b9fa5; overflow-wrap: anywhere; user-select: text; }
.uninstall-size { width: 85px; text-align: right; font-size: 12px; color: #747980; }
.subpage-container {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #f9fafb;
}

.subpage-content {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
  padding: 20px;
  overflow: hidden;
  position: relative;
}

.table-container {
  flex: 1;
  min-height: 0;
  height: 100%;
}

.app-identity { display: flex; align-items: center; gap: 12px; min-width: 0; }
.app-icon { width: 36px; height: 36px; object-fit: contain; flex-shrink: 0; }
.app-icon-fallback { display: grid; place-items: center; border-radius: 8px; background: #f0f8f5; color: #57b797; }
.app-name-cell {
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.app-display-name, .app-pkg-name { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.app-display-name {
  font-weight: 500;
  color: #1f2937;
}

.app-pkg-name {
  font-size: 11px;
  color: #9ca3af;
}
</style>
