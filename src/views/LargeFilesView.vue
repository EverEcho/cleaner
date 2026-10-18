<script setup lang="ts">
import { computed, nextTick, onDeactivated, onMounted, ref, watch } from 'vue';
import { isTauri } from '@tauri-apps/api/core';
import { useRoute, useRouter } from 'vue-router';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { ElMessage, ElMessageBox } from 'element-plus';
import { ArrowDownWideNarrow, Copy, File, HardDrive, Monitor, RefreshCw, Search, Trash2 } from 'lucide-vue-next';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';
import ScanScope from '../components/ScanScope.vue';
import ActionResults from '../components/ActionResults.vue';
import { activeTask, cancelTask, runTask } from '../tasks';
import { preferences, loadPreferences, savePreferences } from '../preferences';
import { copyPath, type ActionReport, type FileScanReport } from '../cleanup';
interface LargeFile { path: string; name: string; size_bytes: number; size_str: string; ext: string }
const desktopConnected = isTauri();
const route = useRoute();
const router = useRouter();
const scanning = ref(false), removing = ref(false), loaded = ref(false), selectionDialog = ref(false);
const error = ref(''), search = ref('');
const largeFiles = ref<LargeFile[]>([]), selected = ref<LargeFile[]>([]), warnings = ref<string[]>([]), scannedRoots = ref<string[]>([]);
const result = ref<ActionReport | null>(null);
const table = ref<{ clearSelection: () => void; toggleRowSelection: (row: LargeFile, selected: boolean) => void }>();
let cancelConfirmation: (() => void) | undefined;
const task = computed(() => activeTask('scan_large_files'));
const totalSize = computed(() => largeFiles.value.reduce((sum, file) => sum + file.size_bytes, 0));
const selectedSize = computed(() => selected.value.reduce((sum, file) => sum + file.size_bytes, 0));
const filteredFiles = computed(() => largeFiles.value.filter(file => `${file.name} ${file.path}`.toLowerCase().includes(search.value.toLowerCase())));
function formatSize(bytes: number) { if (!bytes) return '0 B'; const units = ['B', 'KB', 'MB', 'GB', 'TB']; const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1); return `${(bytes / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}`; }
function selectFiles(rows: LargeFile[]) { selected.value = rows; }
function canSelectFile() { return !removing.value; }
async function reveal(path: string) { try { await revealItemInDir(path); } catch (cause) { ElMessage.error(String(cause)); } }
async function copy(path: string) { try { await copyPath(path); ElMessage.success('路径已复制'); } catch (cause) { ElMessage.error(String(cause)); } }
async function exclude(path: string) { await savePreferences({ exclusions: [...new Set([...preferences.exclusions, path])] }); ElMessage.success('已加入保留名单，下次扫描生效'); }
async function removeSelected() {
  if (!desktopConnected || removing.value || scanning.value || !selected.value.length) return;
  removing.value = true; const paths = selected.value.map(file => file.path);
  try {
    await Promise.race([ElMessageBox.confirm(`将选中的 ${paths.length} 个文件（扫描时逻辑大小 ${formatSize(selectedSize.value)}）移入废纸篓？文件系统实际可用空间变化可能不同。`, '移入废纸篓', { confirmButtonText: '确认移入', cancelButtonText: '保留文件', type: 'warning' }), new Promise<never>((_, reject) => { cancelConfirmation = () => reject('cancel'); })]);
    cancelConfirmation = undefined;
    result.value = await runTask<ActionReport>('trash_files', { paths, kind: 'large' });
    const done = new Set(result.value.items.filter(item => item.status === 'success').map(item => item.path));
    largeFiles.value = largeFiles.value.filter(file => !done.has(file.path));
    const preserved = new Set(result.value.items.filter(item => item.status === 'skipped').map(item => item.path));
    const remaining = selected.value.filter(file => !done.has(file.path) && !preserved.has(file.path)); table.value?.clearSelection(); await nextTick(); for (const row of remaining) table.value?.toggleRowSelection(row, true);
    if (result.value.failed || result.value.skipped) ElMessage.warning(`已处理 ${result.value.succeeded} 个文件，${result.value.failed} 个失败、${result.value.skipped} 个跳过；请查看处理明细`); else ElMessage.success(`已移入废纸篓 ${result.value.succeeded} 个文件`);
  } catch (cause) { if (cause !== 'cancel' && cause !== 'close') ElMessage.error(String(cause)); }
  finally { cancelConfirmation = undefined; removing.value = false; }
}
async function loadLargeFiles() {
  if (!desktopConnected || scanning.value) return;
  scanning.value = true; loaded.value = false; table.value?.clearSelection(); selected.value = []; largeFiles.value = []; error.value = ''; warnings.value = [];
  try { const report = await runTask<FileScanReport<LargeFile>>('scan_large_files', { minMb: preferences.large_min_mb, options: { roots: typeof route.query.root === 'string' ? [route.query.root] : preferences.scan_roots, excluded: preferences.exclusions, min_mb: preferences.large_min_mb } }); largeFiles.value = report.items; warnings.value = report.warnings; scannedRoots.value = report.roots; loaded.value = true; }
  catch (cause) { error.value = String(cause); } finally { scanning.value = false; }
}
watch(() => route.query.root, () => { if (route.path === '/large-files' && desktopConnected && !scanning.value && !removing.value) void loadLargeFiles(); });
onMounted(async () => { await loadPreferences(); await loadLargeFiles(); });
onDeactivated(() => { if (cancelConfirmation) { ElMessageBox.close(); cancelConfirmation(); cancelConfirmation = undefined; } });
</script>

<template>
  <div class="tool-page">
    <PageHeader title="大文件" subtitle="找出占用空间的主角，让每一次清理都有把握。">
      <template #actions><el-button :loading="scanning" :disabled="!desktopConnected || removing" @click="loadLargeFiles"><RefreshCw :size="14" />重新扫描</el-button></template>
    </PageHeader>
    <div class="tool-content">
      <el-alert v-if="route.query.root" :title="`本次临时扫描范围：${route.query.root}`" type="info" :closable="false"><el-button text :disabled="scanning || removing" @click="router.replace({ path: '/large-files' })">使用保存的范围</el-button></el-alert>
      <ScanScope :disabled="scanning || removing || !desktopConnected" :min-mb="preferences.large_min_mb" @update:min-mb="savePreferences({ large_min_mb: $event })" />
      <ActionResults :report="result" />
      <div v-if="!desktopConnected" class="empty-state"><span class="empty-icon"><Monitor :size="30" /></span><h2>给磁盘腾点空间</h2><p>请在桌面应用中打开，扫描本机的大文件。</p><span class="state-label">浏览器预览 · 尚未连接桌面端</span></div>
      <div v-else-if="scanning"><ScanningRadar title="正在寻找空间大户" :sub-title="task?.path || '准备扫描…'" :tips="[]" /><p class="footnote">已检查 {{ task?.current || 0 }} 个文件</p><el-button v-if="task" @click="cancelTask(task.id)">停止扫描，保留已发现结果</el-button></div>
      <template v-else>
        <el-alert v-if="error" title="大文件扫描未完成" :description="error" type="error" :closable="false" show-icon />
        <template v-if="loaded">
          <el-alert v-if="warnings.length" title="部分扫描范围未覆盖" :description="warnings.join('\n')" type="warning" :closable="false" show-icon />
          <div class="summary-grid"><div class="summary-card featured"><HardDrive :size="20" /><span>文件总占用</span><strong>{{ formatSize(totalSize) }}</strong></div><div class="summary-card"><File :size="20" /><span>发现大文件</span><strong>{{ largeFiles.length }}<small>个</small></strong></div><div class="summary-card"><ArrowDownWideNarrow :size="20" /><span>扫描门槛</span><strong>{{ preferences.large_min_mb }}<small>MB 起</small></strong></div></div>
          <div class="result-heading"><div><h2>文件明细</h2><p>按大小排列。选中前请核对名称与所在目录。</p></div><el-input v-model="search" placeholder="搜索文件或路径" clearable :disabled="removing" class="search-input"><template #prefix><Search :size="14" /></template></el-input></div>
          <div v-if="largeFiles.length" class="table-panel">
            <el-table ref="table" :data="filteredFiles" row-key="path" empty-text="没有匹配的文件" @selection-change="selectFiles">
              <el-table-column type="selection" :selectable="canSelectFile" reserve-selection width="44" />
              <el-table-column label="文件" min-width="250"><template #default="{ row }"><div class="file-cell"><span class="file-icon"><File :size="18" /></span><div><strong :title="row.name">{{ row.name }}</strong><span :title="row.path">{{ row.path }}</span></div></div></template></el-table-column>
              <el-table-column prop="size_bytes" label="占用空间" width="130" sortable align="right"><template #default="{ row }"><span class="file-size">{{ row.size_str }}</span></template></el-table-column>
              <el-table-column label="操作" width="150"><template #default="{ row }"><el-button text title="查看位置" @click="reveal(row.path)"><Search :size="14" /></el-button><el-button text title="复制路径" @click="copy(row.path)"><Copy :size="14" /></el-button><el-button text :disabled="removing" title="加入保留名单" @click="exclude(row.path)">保留</el-button></template></el-table-column>
              <el-table-column label="格式" width="82" align="center"><template #default="{ row }"><span class="format-tag">{{ row.ext ? row.ext.toUpperCase() : '无扩展名' }}</span></template></el-table-column>
            </el-table>
          </div>
          <div v-else class="empty-state compact"><span class="empty-icon"><HardDrive :size="28" /></span><h2>这里没有空间大户</h2><p>本次已读取目录中，没有发现 {{ preferences.large_min_mb }} MB 以上的文件。</p></div>
          <p v-if="largeFiles.length" class="footnote">覆盖：{{ scannedRoots.join('、') }}。容量为逻辑大小，移入废纸篓后需清空才能释放空间。</p>
        </template>
        <div v-else-if="!error" class="empty-state"><span class="empty-icon"><HardDrive :size="30" /></span><h2>找出空间大户</h2><p>开始扫描，查看常用目录中的大文件。</p></div>
      </template>
    </div>
    <div v-if="selected.length && !scanning" class="selection-bar"><div><span>已选择 {{ selected.length }} 个文件</span><strong>{{ formatSize(selectedSize) }}</strong></div><div><el-button text @click="selectionDialog = true">查看全部已选</el-button><el-button type="primary" :loading="removing" @click="removeSelected"><Trash2 :size="15" />移入废纸篓</el-button></div></div>
    <el-dialog v-model="selectionDialog" title="全部已选文件" width="70%"><div class="selection-list"><div v-for="file in selected" :key="file.path" class="selected-file"><span>{{ file.path }} · {{ file.size_str }}</span><el-button text @click="reveal(file.path)">查看位置</el-button></div></div></el-dialog>
  </div>
</template>

<style scoped>
.selection-list, .developer-dialog-body { max-height: 55vh; overflow: auto; }
.selected-file { display: flex; justify-content: space-between; gap: 12px; padding: 10px 0; overflow-wrap: anywhere; font-size: 12px; }
.tool-page { height: 100%; display: flex; flex-direction: column; min-width: 0; background: transparent; }
.tool-content { flex: 1; min-height: 0; overflow: auto; padding: 8px 28px 24px; }
.summary-grid { display: grid; grid-template-columns: 1.3fr 1fr 1fr; gap: 12px; margin-bottom: 24px; }
.summary-card { display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 10px; padding: 20px; background: #ffffff04; border: 1px solid #ffffff0d; border-radius: 16px; }
.summary-card > svg { grid-row: 1; grid-column: 2; color: #79728f; }
.summary-card > span { font-size: 11px; color: #9693ad; }
.summary-card strong { grid-column: 1 / -1; font-size: 26px; color: #f1efff; letter-spacing: -.7px; font-weight: 500; white-space: nowrap; }
.summary-card small { font-size: 11px; color: #817b98; letter-spacing: 0; margin-left: 8px; }
.summary-card.featured { background: radial-gradient(ellipse at top right, #a78bfa22, transparent 100%), #ffffff04; border-color: #a78bfa25; }
.featured > svg { color: #b5a0ef; }
.result-heading { display: flex; justify-content: space-between; align-items: center; gap: 16px; margin: 0 0 15px; }
.result-heading h2 { margin: 0 0 6px; font-size: 14px; font-weight: 500; color: #f1efff; }
.result-heading p { color: #9693ad; font-size: 11px; line-height: 1.6; margin: 0; }
.search-input { width: 185px; flex-shrink: 0; }
.table-panel { border: 1px solid #ffffff0d; border-radius: 14px; overflow: hidden; }
.file-cell { display: flex; gap: 11px; align-items: center; min-width: 0; }
.file-icon { width: 34px; height: 39px; display: grid; place-items: center; flex-shrink: 0; background: linear-gradient(145deg, #a78bfa16, #a78bfa04); border: 1px solid #a78bfa18; color: #aa9ccf; border-radius: 9px; }
.file-cell > div { min-width: 0; }
.file-cell strong { display: block; font-size: 12px; font-weight: 500; color: #e5dff7; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.file-cell div span { display: block; font-size: 10px; color: #817b97; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 3px; }
.file-size { color: #c9b8f8; font-size: 12px; font-variant-numeric: tabular-nums; }
.format-tag { font-size: 9px; color: #968da9; border: 1px solid #ffffff0e; background: #ffffff04; border-radius: 5px; padding: 3px 5px; }
.footnote { color: #817c99; font-size: 11px; line-height: 1.8; margin: 15px 0 0; }
.selection-bar { display: flex; justify-content: space-between; align-items: center; gap: 18px; padding: 15px 28px; border-top: 1px solid #a78bfa22; background: #211c34; flex-shrink: 0; }
.selection-bar > div { display: flex; align-items: center; gap: 16px; }
.selection-bar span { font-size: 12px; color: #b3a8cb; }
.selection-bar strong { font-size: 17px; color: #eee8ff; font-weight: 500; }
.empty-state { min-height: 380px; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; }
.empty-state.compact { min-height: 250px; }
.empty-icon { display: grid; place-items: center; width: 78px; height: 78px; border-radius: 24px; background: linear-gradient(135deg, #a78bfa22, #64d5e90b); border: 1px solid #a78bfa22; color: #b5a0ef; margin-bottom: 24px; }
.empty-state h2 { color: #ede9fb; font-size: 20px; font-weight: 500; margin: 0 0 12px; }
.empty-state p { color: #9693ad; font-size: 12px; line-height: 1.8; margin: 0; }
.state-label { margin-top: 22px; padding: 6px 10px; border: 1px solid #ffffff0e; border-radius: 20px; font-size: 10px; color: #807b95; }
:deep(.el-alert) { margin-bottom: 15px; }
:deep(.el-button svg) { margin-right: 7px; }
@media (max-width: 850px) { .tool-content { padding: 8px 20px 20px; } .summary-card { padding: 16px; } .summary-card strong { font-size: 23px; } .selection-bar { padding: 15px 20px; } }
</style>
