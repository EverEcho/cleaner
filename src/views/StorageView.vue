<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { ArrowLeft, ArrowUpRight, Copy, File, Folder, FolderOpen, HardDrive, Monitor, RefreshCw } from 'lucide-vue-next';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { ElMessage } from 'element-plus';
import { useRouter } from 'vue-router';
import { activeTask, cancelTask, runTask } from '../tasks';
import { preferences, loadPreferences } from '../preferences';
import { copyPath } from '../cleanup';

interface Usage { path: string; size_bytes: number; size: string; is_dir: boolean }
const router = useRouter();
const task = computed(() => activeTask('analyze_disk'));
const desktopConnected = isTauri();
const roots = ref<string[]>([]);
const root = ref('');
const analyzedRoot = ref('');
const history = ref<string[]>([]);
const directories = ref<Usage[]>([]);
const warning = ref('');
const error = ref('');
const scanning = ref(false);
const loaded = ref(false);
const total = computed(() => directories.value.reduce((sum, row) => sum + row.size_bytes, 0));
const largest = computed(() => directories.value[0]);
const breadcrumbs = computed(() => { const parts = analyzedRoot.value.split('/').filter(Boolean); return [{ label: '/', path: '/' }, ...parts.map((label, index) => ({ label, path: '/' + parts.slice(0, index + 1).join('/') }))]; });
const leading = computed(() => directories.value.slice(0, 5));
const colors = ['#a78bfa', '#64d5e9', '#e69ad9', '#758df1', '#b8b4cf'];
function formatSize(bytes: number) {
  if (!bytes) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}`;
}
function folderName(path: string) { return path.split('/').filter(Boolean).pop() || path; }
async function analyze(path = root.value, remember = true) {
  if (!desktopConnected || !path || scanning.value) return false;
  scanning.value = true;
  error.value = '';
  try {
    const result = await runTask<{ directories: Usage[]; warning: string; cancelled: boolean }>('analyze_disk', { root: path, options: { excluded: preferences.exclusions } });
    if (remember && analyzedRoot.value && path !== analyzedRoot.value) history.value.push(analyzedRoot.value);
    root.value = path;
    analyzedRoot.value = path;
    directories.value = result.directories;
    warning.value = result.warning;
    loaded.value = true;
    return true;
  } catch (cause) { error.value = String(cause); return false; }
  finally { scanning.value = false; }
}
async function goBack() {
  const previous = history.value[history.value.length - 1];
  if (previous && await analyze(previous, false)) history.value.pop();
}
function drillDown(row: Usage) { if (row.is_dir) void analyze(row.path); else void reveal(row.path); }
async function pick() { try { const path = await invoke<string | null>('pick_directory', { title: '选择分析目录' }); if (path) await analyze(path); } catch (cause) { ElMessage.error(String(cause)); } }
async function reveal(path: string) { try { await revealItemInDir(path); } catch (cause) { ElMessage.error(String(cause)); } }
async function copy(path: string) { try { await copyPath(path); ElMessage.success('路径已复制'); } catch (cause) { ElMessage.error(String(cause)); } }
async function scanLarge() { await router.push({ path: '/large-files', query: { root: analyzedRoot.value } }); }
onMounted(async () => {
  if (!desktopConnected) return;
  try {
    await loadPreferences();
    roots.value = await invoke<string[]>('get_storage_roots');
    root.value = roots.value[0] ?? '';
    await analyze();
  } catch (cause) { error.value = String(cause); }
});
</script>

<template>
  <div class="tool-page">
    <PageHeader title="空间透视" subtitle="从目录到文件夹，看清磁盘空间流向。">
      <template #actions>
        <el-button :loading="scanning" :disabled="!desktopConnected || !root" @click="analyze()"><RefreshCw :size="14" />重新分析</el-button>
      </template>
    </PageHeader>
    <div class="tool-content">
      <div class="path-toolbar">
        <button class="back-step" aria-label="返回上次分析的目录" :disabled="!history.length || scanning" @click="goBack"><ArrowLeft :size="17" /></button>
        <Folder :size="17" class="muted-icon" />
        <el-select v-model="root" filterable allow-create placeholder="选择或输入绝对目录路径" :disabled="scanning || !desktopConnected" class="path-select">
          <el-option v-for="path in roots" :key="path" :label="path" :value="path" />
        </el-select>
        <el-button :disabled="!desktopConnected || scanning" title="选择目录" @click="pick"><FolderOpen :size="14" /></el-button>
        <el-button type="primary" :disabled="!desktopConnected || !root || scanning" @click="analyze()">分析目录<ArrowUpRight :size="14" /></el-button>
      </div>
      <div v-if="!desktopConnected" class="empty-state"><span class="empty-icon"><Monitor :size="30" /></span><h2>连接你的磁盘</h2><p>请在桌面应用中打开空间透视，读取本机的目录占用。</p><span class="state-label">浏览器预览 · 尚未连接桌面端</span></div>
      <div v-else-if="scanning"><ScanningRadar title="正在绘制空间分布" :sub-title="task?.path || root" :tips="[]" /><p class="footnote">已检查 {{ task?.current || 0 }} 个文件 · 不跨文件系统</p><el-button v-if="task" @click="cancelTask(task.id)">停止分析</el-button></div>
      <template v-else>
        <el-alert v-if="error" title="目录分析未完成" :description="error" type="error" :closable="false" show-icon />
        <template v-if="loaded">
          <div class="breadcrumbs"><button v-for="crumb in breadcrumbs" :key="crumb.path" @click="analyze(crumb.path)">{{ crumb.label }}</button><el-button text @click="reveal(analyzedRoot)">查看位置</el-button><el-button text @click="scanLarge">扫描此目录大文件</el-button></div>
          <div class="space-overview">
            <div class="space-total"><span class="eyebrow">已统计逻辑大小</span><strong>{{ formatSize(total) }}</strong><span>{{ directories.length }} 个文件与目录<small v-if="warning"> · 部分结果</small></span></div>
            <div class="space-chart">
              <span class="chart-caption"><HardDrive :size="14" />文件与目录分布</span>
              <div v-if="total" class="segment-bar"><button v-for="(item, index) in leading" :key="item.path" :style="{ flexGrow: item.size_bytes, background: colors[index] }" :title="`${item.path} · ${item.size}`" @click="drillDown(item)" /><span v-if="directories.length > 5" :style="{ flexGrow: total - leading.reduce((sum, row) => sum + row.size_bytes, 0) }" /></div>
              <div v-if="total" class="legend"><button v-for="(item, index) in leading.slice(0, 3)" :key="item.path" @click="drillDown(item)"><i :style="{ background: colors[index] }" />{{ folderName(item.path) }}</button></div>
              <p v-else class="chart-empty">没有可统计的文件或目录占用</p>
            </div>
          </div>
          <el-alert v-if="warning" :title="warning" type="warning" :closable="false" show-icon />
          <div class="result-heading"><div><h2>文件与目录明细</h2><p :title="analyzedRoot">{{ analyzedRoot }}</p></div><span v-if="largest">最大项目 · {{ largest.size }}</span></div>
          <div class="table-panel">
            <el-table :data="directories" empty-text="当前目录未发现可读取的文件或子目录" @row-click="drillDown">
              <el-table-column label="目录" min-width="190" show-overflow-tooltip><template #default="{ row }"><div class="folder-cell"><Folder v-if="row.is_dir" :size="18" /><File v-else :size="18" /><span>{{ folderName(row.path) }}</span></div></template></el-table-column>
              <el-table-column label="占比" min-width="160"><template #default="{ row }"><el-progress :percentage="total ? Math.round(row.size_bytes / total * 100) : 0" :stroke-width="5" color="#a78bfa" /></template></el-table-column>
              <el-table-column prop="size" label="占用空间" width="120" align="right" />
              <el-table-column width="100" align="center"><template #default="{ row }"><el-button text title="查看位置" @click.stop="reveal(row.path)"><FolderOpen :size="14" /></el-button><el-button text title="复制路径" @click.stop="copy(row.path)"><Copy :size="14" /></el-button></template></el-table-column>
            </el-table>
          </div>
          <p class="footnote">点击目录继续分析，点击文件查看位置。占比基于已读取的逻辑大小，不等于实际可回收空间；符号链接及其他文件系统会跳过。</p>
        </template>
        <div v-else-if="!error" class="empty-state"><span class="empty-icon"><HardDrive :size="30" /></span><h2>空间，从这里展开</h2><p>选择一个目录，查看它的空间分布。</p></div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.breadcrumbs { display: flex; align-items: center; gap: 5px; flex-wrap: wrap; margin-bottom: 15px; } .breadcrumbs > button { background: #ffffff08; border: 1px solid #ffffff0d; color: #c3b7d8; padding: 5px 9px; border-radius: 7px; font-size: 11px; cursor: pointer; }
.tool-page { height: 100%; display: flex; flex-direction: column; min-width: 0; background: transparent; }
.tool-content { flex: 1; min-height: 0; overflow: auto; padding: 8px 28px 24px; }
.path-toolbar { display: flex; gap: 10px; align-items: center; margin-bottom: 22px; }
.path-select { flex: 1; min-width: 120px; }
.back-step { width: 34px; height: 34px; display: grid; place-items: center; background: #ffffff06; color: #c3bddf; border: 1px solid #ffffff12; border-radius: 10px; cursor: pointer; }
.back-step:disabled { opacity: .35; cursor: default; }
.muted-icon { color: #9693ad; flex-shrink: 0; }
.space-overview { display: flex; align-items: center; gap: 30px; padding: 25px; background: radial-gradient(ellipse at 0 0, #a78bfa18, transparent 70%), #ffffff04; border: 1px solid #ffffff0d; border-radius: 20px; margin-bottom: 20px; }
.space-total { display: flex; flex-direction: column; gap: 9px; min-width: 145px; }
.eyebrow { font-size: 11px; color: #b3aacb; }
.space-total strong { font-size: 32px; color: #f1efff; letter-spacing: -1.2px; font-weight: 600; }
.space-total > span:last-child { font-size: 11px; color: #9693ad; }
.space-total small { color: #e8c084; }
.space-chart { flex: 1; min-width: 0; }
.chart-caption { display: flex; gap: 7px; align-items: center; color: #bcb7cf; font-size: 11px; margin-bottom: 14px; }
.segment-bar { display: flex; height: 25px; gap: 3px; overflow: hidden; border-radius: 7px; }
.segment-bar button, .segment-bar > span { min-width: 2px; padding: 0; border: 0; background: #464158; }
.segment-bar button { cursor: pointer; transition: filter .2s; }
.segment-bar button:hover { filter: brightness(1.3); }
.legend { display: flex; gap: 14px; margin-top: 13px; overflow: hidden; }
.legend button { display: flex; align-items: center; gap: 6px; padding: 0; border: 0; background: none; color: #b5b0c8; font-size: 11px; cursor: pointer; max-width: 120px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.legend i { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
.chart-empty { color: #9693ad; font-size: 12px; }
.result-heading { display: flex; justify-content: space-between; align-items: center; gap: 16px; margin: 24px 0 13px; }
.result-heading h2 { margin: 0 0 6px; font-size: 14px; font-weight: 500; color: #f1efff; }
.result-heading p { color: #9693ad; font-size: 11px; margin: 0; max-width: 350px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.result-heading > span { font-size: 11px; color: #a78bfa; white-space: nowrap; }
.table-panel { border: 1px solid #ffffff0d; border-radius: 14px; overflow: hidden; }
.folder-cell { display: flex; align-items: center; gap: 10px; color: #e6e1f5; }
.folder-cell svg { color: #a78bfa; flex-shrink: 0; }
:deep(.el-table__row) { cursor: pointer; }
.footnote { color: #817c99; font-size: 11px; line-height: 1.8; margin: 14px 0 0; }
.empty-state { min-height: 340px; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; }
.empty-icon { display: grid; place-items: center; width: 78px; height: 78px; border-radius: 24px; background: linear-gradient(135deg, #a78bfa22, #64d5e90b); border: 1px solid #a78bfa22; color: #b5a0ef; margin-bottom: 24px; }
.empty-state h2 { color: #ede9fb; font-size: 20px; font-weight: 500; margin: 0 0 12px; }
.empty-state p { color: #9693ad; font-size: 12px; line-height: 1.8; margin: 0; }
.state-label { margin-top: 22px; padding: 6px 10px; border: 1px solid #ffffff0e; border-radius: 20px; font-size: 10px; color: #807b95; }
:deep(.el-alert) { margin-bottom: 15px; }
:deep(.el-button svg) { margin: 0 4px; }
@media (max-width: 850px) { .tool-content { padding: 8px 20px 20px; } .space-overview { gap: 18px; padding: 20px; } .space-total strong { font-size: 28px; } }
</style>
