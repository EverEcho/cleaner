<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { ArrowLeft, ChevronDown, File, Folder, FolderOpen, HardDrive, Monitor } from 'lucide-vue-next';
import PageHeader from '../layout/PageHeader.vue';
import SummaryDetails from '../components/SummaryDetails.vue';
import ScanAction from '../components/ScanAction.vue';
import StorageTreemap from '../components/StorageTreemap.vue';
import type { StorageItem } from '../treemap';
import ScanningRadar from '../layout/ScanningRadar.vue';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { ElMessage } from 'element-plus';
import { useRouter } from 'vue-router';
import { activeTask, runTask } from '../tasks';
import { preferences, loadPreferences } from '../preferences';
import { copyPath } from '../cleanup';

type Usage = StorageItem;
interface Analysis { root?: string; directories: Usage[]; warning: string; cancelled: boolean; cached?: boolean }
const router = useRouter();
const task = computed(() => activeTask('analyze_disk'));
const desktopConnected = isTauri();
const roots = ref<string[]>([]);
const root = ref('');
const analyzedRoot = ref('');
const snapshotRoot = ref('');
const snapshotExclusions = ref<string[]>([]);
const snapshotPages = new Map<string, Analysis>();
const browsing = ref(false);
const history = ref<string[]>([]);
const directories = ref<Usage[]>([]);
const warning = ref('');
const error = ref('');
const scanning = ref(false);
const loaded = ref(false);
const visual = ref(true);
const highlighted = ref('');
const cached = ref(false);
const page = ref(1);
const pageItems = computed(() => directories.value.slice((page.value - 1) * 100, page.value * 100));
const total = computed(() => directories.value.reduce((sum, row) => sum + row.size_bytes, 0));
const breadcrumbs = computed(() => {
  const base = snapshotRoot.value;
  if (!base) return [];
  const parts = analyzedRoot.value.slice(base === '/' ? 1 : base.length + 1).split('/').filter(Boolean);
  return [{ label: folderName(base), path: base }, ...parts.map((label, index) => ({ label, path: (base === '/' ? '' : base) + '/' + parts.slice(0, index + 1).join('/') }))];
});
function formatSize(bytes: number) {
  if (!bytes) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}`;
}
function folderName(path: string) { return path.split('/').filter(Boolean).pop() || path; }
function showAnalysis(result: Analysis, path: string, remember: boolean) {
  const next = result.root || path;
  if (remember && analyzedRoot.value && next !== analyzedRoot.value) history.value.push(analyzedRoot.value);
  root.value = next;
  analyzedRoot.value = next;
  directories.value = result.directories;
  page.value = 1;
  highlighted.value = '';
  warning.value = result.warning;
  cached.value = Boolean(result.cached);
  loaded.value = true;
}
async function analyze(path = root.value) {
  if (!desktopConnected || !path || scanning.value || browsing.value) return false;
  scanning.value = true;
  error.value = '';
  try {
    const excluded = [...preferences.exclusions];
    const result = await runTask<Analysis>('analyze_disk', { root: path, refresh: true, options: { excluded } });
    snapshotPages.clear();
    snapshotRoot.value = result.root || path;
    snapshotExclusions.value = excluded;
    snapshotPages.set(snapshotRoot.value, result);
    history.value = [];
    showAnalysis(result, path, false);
    return true;
  } catch (cause) { error.value = String(cause); return false; }
  finally { scanning.value = false; }
}
async function browse(path: string, remember = true) {
  if (!desktopConnected || !loaded.value || scanning.value || browsing.value) return false;
  const existing = snapshotPages.get(path);
  if (existing) { showAnalysis({ ...existing, cached: true }, path, remember); return true; }
  browsing.value = true;
  try {
    const result = await invoke<Analysis>('analyze_disk', { root: path, refresh: false, options: { excluded: snapshotExclusions.value } });
    snapshotPages.set(result.root || path, result);
    showAnalysis(result, path, remember);
    return true;
  } catch (cause) { ElMessage.warning(String(cause)); return false; }
  finally { browsing.value = false; }
}
async function goBack() {
  const previous = history.value[history.value.length - 1];
  if (previous && await browse(previous, false)) history.value.pop();
}
function drillDown(row: Usage) { if (row.is_dir) return browse(row.path); return reveal(row.path); }
async function pick() { try { const path = await invoke<string | null>('pick_directory', { title: '选择分析目录' }); if (path) root.value = path; } catch (cause) { ElMessage.error(String(cause)); } }
async function reveal(path: string) { try { await revealItemInDir(path); } catch (cause) { ElMessage.error(String(cause)); } }
async function copy(path: string) { try { await copyPath(path); ElMessage.success('路径已复制'); } catch (cause) { ElMessage.error(String(cause)); } }
async function scanLarge() { await router.push({ path: '/large-files', query: { root: analyzedRoot.value } }); }
onMounted(async () => {
  if (!desktopConnected) return;
  try {
    await loadPreferences();
    roots.value = await invoke<string[]>('get_storage_roots');
    root.value = roots.value[0] ?? '';
  } catch (cause) { error.value = String(cause); }
});
</script>

<template>
  <div class="tool-page">
    <PageHeader title="空间透视" subtitle="从目录到文件夹，看清磁盘空间流向。">
      <template #actions>
        <ScanAction v-if="!scanning && (loaded || error)" :scanning="scanning" :task="task" :disabled="!desktopConnected || !root || browsing" :hint="`已检查 ${task?.current || 0} 个文件 · 不跨文件系统`" :label="root === analyzedRoot ? '重新分析' : '分析目录'" stop-label="停止分析" @start="analyze(root)" />
      </template>
    </PageHeader>
    <div class="tool-content">
      <div class="path-toolbar">
        <button class="back-step" aria-label="返回上个目录" :disabled="!history.length || scanning || browsing" @click="goBack"><ArrowLeft :size="17" /></button>
        <Folder :size="17" class="muted-icon" />
        <el-select v-model="root" filterable allow-create placeholder="选择或输入绝对目录路径" :disabled="scanning || browsing || !desktopConnected" class="path-select" :title="root" @keyup.enter="analyze(root)">
          <el-option v-for="path in roots" :key="path" :label="path" :value="path" />
        </el-select>
        <el-button :disabled="!desktopConnected || scanning || browsing" title="选择目录" @click="pick"><FolderOpen :size="14" /></el-button>
        <el-switch v-model="visual" active-text="可视化" aria-label="可视化模式" :disabled="scanning" />
      </div>
      <div v-if="!desktopConnected" class="empty-state"><span class="empty-icon"><Monitor :size="30" /></span><h2>连接你的磁盘</h2><p>请在桌面应用中打开空间透视，读取本机的目录占用。</p><span class="state-label">浏览器预览 · 尚未连接桌面端</span></div>
      <div v-else-if="scanning"><ScanningRadar title="正在绘制空间分布" :sub-title="task?.path || root" :tips="[]"><ScanAction initial :scanning="true" :task="task" :hint="`已检查 ${task?.current || 0} 个文件 · 不跨文件系统`" stop-label="停止分析" /></ScanningRadar></div>
      <template v-else>
        <div v-if="loaded || error" class="result-summary"><strong>{{ loaded ? formatSize(total) : '分析未完成' }}</strong><span>{{ directories.length }} 个文件与目录<template v-if="warning"> · 部分结果</template></span><small v-if="cached">复用本次分析</small><div class="summary-actions"><SummaryDetails label="分析范围" :count="1"><code :title="analyzedRoot || root">{{ analyzedRoot || root }}</code><p>仅统计当前文件系统，不跟随符号链接。容量为逻辑大小，共享依赖或硬链接不代表可重复释放的空间。</p><p v-if="cached">显示本次分析的目录快照；打开目录和返回不会重新扫描，点击“重新分析”才更新。</p><p>保留名单：{{ snapshotExclusions.length }} 项</p><code v-for="path in snapshotExclusions" :key="path" :title="path">{{ path }}</code></SummaryDetails><SummaryDetails :label="error ? '分析未完成' : '结果提示'" :count="(warning ? warning.split('\n').length : 0) + (error ? 1 : 0)" :tone="error ? 'danger' : warning ? 'warning' : undefined"><p v-if="error" class="detail-message danger">{{ error }}</p><p v-if="warning" class="detail-message warning">{{ warning }}</p><p v-if="!error && !warning">暂无结果提示</p></SummaryDetails></div></div>
        <template v-if="loaded">
          <div class="breadcrumbs"><button v-for="crumb in breadcrumbs" :key="crumb.path" :title="crumb.path" :disabled="browsing" @click="browse(crumb.path)">{{ crumb.label }}</button><el-button text @click="reveal(analyzedRoot)">查看位置</el-button><el-button text @click="scanLarge">查看此目录大文件</el-button></div>

          <div v-if="visual" class="visual-layout">
            <div class="directory-list" aria-label="目录占用排行">
              <button v-for="row in directories.slice(0, 120)" :key="row.path" class="directory-row" :disabled="browsing" :class="{ highlighted: highlighted === row.path }" :title="row.path" @mouseenter="highlighted = row.path" @mouseleave="highlighted = ''" @focus="highlighted = row.path" @blur="highlighted = ''" @click="drillDown(row)">
                <Folder v-if="row.is_dir" :size="21" /><File v-else :size="21" /><span>{{ folderName(row.path) }}</span><small>{{ row.size }}</small>
              </button>
              <button v-if="directories.length > 120" class="directory-row" @click="visual = false">查看其余 {{ directories.length - 120 }} 项</button>
              <p v-if="!directories.length" class="chart-empty">没有可读取的文件或目录</p>
            </div>
            <StorageTreemap :items="directories" :highlighted="highlighted" @open="drillDown" @highlight="highlighted = $event" @show-list="visual = false" />
          </div>
          <div v-else class="table-panel">
            <el-table :data="pageItems" empty-text="当前目录未发现可读取的文件或子目录" @row-click="drillDown">
              <el-table-column label="目录" min-width="190" show-overflow-tooltip><template #default="{ row }"><div class="folder-cell"><Folder v-if="row.is_dir" :size="18" /><File v-else :size="18" /><span>{{ folderName(row.path) }}</span></div></template></el-table-column>
              <el-table-column label="占比" min-width="160"><template #default="{ row }"><el-progress :percentage="total ? Math.round(row.size_bytes / total * 100) : 0" :stroke-width="5" color="var(--accent)" /></template></el-table-column>
              <el-table-column prop="size" label="占用空间" width="120" align="right" />
              <el-table-column width="85" align="center"><template #default="{ row }"><el-dropdown trigger="click" @click.stop><el-button text aria-label="目录或文件操作" @click.stop>更多<ChevronDown :size="13" /></el-button><template #dropdown><el-dropdown-menu><el-dropdown-item @click="reveal(row.path)">查看位置</el-dropdown-item><el-dropdown-item @click="copy(row.path)">复制路径</el-dropdown-item></el-dropdown-menu></template></el-dropdown></template></el-table-column>
            </el-table>
            <el-pagination v-if="directories.length > 100" v-model:current-page="page" small :page-size="100" :total="directories.length" layout="prev, pager, next" class="directory-pages" />
          </div>
        </template>
        <div v-else-if="!error" class="empty-state"><span class="empty-icon"><HardDrive :size="30" /></span><h2>空间，从这里展开</h2><p>选择目录，查看文件与文件夹的空间占用。</p><ScanAction initial :scanning="false" :disabled="!root" label="分析目录" @start="analyze(root)" /></div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.breadcrumbs { display: flex; align-items: center; gap: 5px; flex-wrap: wrap; margin-bottom: 15px; } .breadcrumbs > button { min-width: 0; max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; background: var(--surface); border: 1px solid var(--line); color: var(--text-secondary); padding: 5px 9px; border-radius: 7px; font-size: 11px; cursor: pointer; }
.tool-page { height: 100%; display: flex; flex-direction: column; min-width: 0; background: transparent; }
.tool-content { flex: 1; min-width: 0; min-height: 0; overflow-y: auto; overflow-x: hidden; padding: 8px 28px 24px; }
.path-toolbar { display: flex; gap: 10px; align-items: center; margin-bottom: 12px; }
.path-toolbar > .el-switch { flex-shrink: 0; }
.path-select { flex: 1; min-width: 120px; }
.back-step { width: 34px; height: 34px; display: grid; place-items: center; background: var(--surface); color: var(--text-secondary); border: 1px solid var(--line); border-radius: 10px; cursor: pointer; }
.back-step:disabled { opacity: .35; cursor: default; }
.muted-icon { color: var(--text-muted); flex-shrink: 0; }
.visual-layout { display: grid; grid-template-columns: minmax(150px, 28%) minmax(0, 1fr); gap: 14px; min-width: 0; --storage-map-height: clamp(240px, calc(100vh - 270px), 620px); }
.directory-list { min-width: 0; max-height: var(--storage-map-height); overflow-y: auto; overflow-x: hidden; border: 1px solid var(--line); border-radius: 10px; padding: 4px; background: var(--surface-subtle); }
.directory-row { display: flex; align-items: center; gap: 8px; width: 100%; min-width: 0; padding: 12px 8px; border: 0; border-radius: 7px; background: transparent; color: var(--text-secondary); font: inherit; cursor: pointer; text-align: left; }
.directory-row svg { color: var(--accent); flex-shrink: 0; }
.directory-row span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
.directory-row small { flex-shrink: 0; font-size: 10px; color: var(--text-muted); }
.directory-row:hover, .directory-row.highlighted { background: var(--accent-soft); color: var(--accent); }
.chart-empty { padding: 12px; color: var(--text-muted); font-size: 12px; }
.result-summary { padding-top: 8px; padding-bottom: 12px; }
.directory-pages { justify-content: flex-end; padding: 12px; }
.table-panel { border: 1px solid var(--line); border-radius: 14px; overflow: hidden; }
.folder-cell { min-width: 0; display: flex; align-items: center; gap: 10px; color: var(--text); }
.folder-cell span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.folder-cell svg { color: var(--accent); flex-shrink: 0; }
:deep(.el-table__row) { cursor: pointer; }
.footnote { color: var(--text-faint); font-size: 11px; line-height: 1.8; margin: 14px 0 0; }
.empty-state { min-height: 340px; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; }
.empty-icon { display: grid; place-items: center; width: 78px; height: 78px; border-radius: 24px; background: linear-gradient(135deg, rgba(var(--accent-rgb), 0.13), rgba(var(--cyan-rgb), 0.06)); border: 1px solid var(--accent-border); color: var(--accent); margin-bottom: 24px; }
.empty-state h2 { color: var(--text); font-size: 20px; font-weight: 500; margin: 0 0 12px; }
.empty-state p { color: var(--text-muted); font-size: 12px; line-height: 1.8; margin: 0; }
.state-label { margin-top: 22px; padding: 6px 10px; border: 1px solid var(--line); border-radius: 20px; font-size: 10px; color: var(--text-faint); }
:deep(.el-alert) { margin-bottom: 15px; overflow-wrap: anywhere; }
:deep(.el-button svg) { margin: 0 4px; }
@media (max-width: 850px) { .tool-content { padding: 8px 20px 20px; } }
</style>
