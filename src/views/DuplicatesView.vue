<script setup lang="ts">
import { computed, onDeactivated, onMounted, ref } from 'vue';
import { isTauri } from '@tauri-apps/api/core';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { ElMessage, ElMessageBox } from 'element-plus';
import { ChevronDown, Copy, File, Monitor, Search, ShieldCheck, Trash2 } from 'lucide-vue-next';
import PageHeader from '../layout/PageHeader.vue';
import ScanAction from '../components/ScanAction.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';
import ScanScope from '../components/ScanScope.vue';
import ActionResults from '../components/ActionResults.vue';
import { activeTask, runTask } from '../tasks';
import { preferences, loadPreferences } from '../preferences';
import { copyPath, type ActionReport, type FileScanReport } from '../cleanup';
interface DuplicateGroup { hash: string; size_bytes: number; size_str: string; files: string[] }
const desktopConnected = isTauri();
const scanning = ref(false), removing = ref(false), loaded = ref(false), selectionDialog = ref(false);
const error = ref(''), search = ref('');
const duplicateGroups = ref<DuplicateGroup[]>([]), selected = ref<string[]>([]), warnings = ref<string[]>([]), scannedRoots = ref<string[]>([]);
const retained = ref<Record<string, string>>({}), result = ref<ActionReport | null>(null);
let cancelConfirmation: (() => void) | undefined;
const task = computed(() => activeTask('scan_duplicate_files'));
const selectedSet = computed(() => new Set(selected.value));
const extraSize = computed(() => duplicateGroups.value.reduce((sum, group) => sum + group.size_bytes * (group.files.length - 1), 0));
const fileCount = computed(() => duplicateGroups.value.reduce((sum, group) => sum + group.files.length, 0));
const selectedSize = computed(() => duplicateGroups.value.reduce((sum, group) => sum + group.size_bytes * group.files.filter(file => selectedSet.value.has(file)).length, 0));
const filteredGroups = computed(() => duplicateGroups.value.filter(group => group.files.some(file => file.toLowerCase().includes(search.value.toLowerCase()))));
function formatSize(bytes: number) { if (!bytes) return '0 B'; const units = ['B', 'KB', 'MB', 'GB', 'TB']; const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1); return `${(bytes / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}`; }
function fileName(path: string) { return path.split('/').pop() || path; }
function mustKeep(file: string, group: DuplicateGroup) { return retained.value[group.hash] === file || !selectedSet.value.has(file) && group.files.filter(path => !selectedSet.value.has(path)).length <= 1; }
function selectFile(file: string, group: DuplicateGroup, checked: boolean) {
  if (removing.value || mustKeep(file, group)) return;
  selected.value = checked ? [...new Set([...selected.value, file])] : selected.value.filter(path => path !== file);
}
function keep(file: string, group: DuplicateGroup) { retained.value = { ...retained.value, [group.hash]: file }; selected.value = selected.value.filter(path => path !== file); }
function selectOthers(group: DuplicateGroup) { const chosen = retained.value[group.hash] || group.files[0]; if (!chosen) return; keep(chosen, group); selected.value = [...new Set([...selected.value.filter(path => !group.files.includes(path)), ...group.files.filter(file => file !== chosen)])]; }
async function reveal(path: string) { try { await revealItemInDir(path); } catch (cause) { ElMessage.error(String(cause)); } }
async function copy(path: string) { try { await copyPath(path); ElMessage.success('路径已复制'); } catch (cause) { ElMessage.error(String(cause)); } }
async function removeSelected() {
  if (!desktopConnected || removing.value || scanning.value || !selected.value.length) return;
  const paths = [...selected.value]; const chosen = new Set(paths);
  if (duplicateGroups.value.some(group => group.files.every(file => chosen.has(file)))) { ElMessage.warning('每组至少需要保留一份文件。'); return; }
  removing.value = true;
  try {
    await Promise.race([ElMessageBox.confirm(`将选中的 ${paths.length} 份重复文件（逻辑大小 ${formatSize(selectedSize.value)}）移入废纸篓？每组保留有效副本，后端会重新比对内容。`, '整理重复文件', { confirmButtonText: '确认移入', cancelButtonText: '保留文件', type: 'warning' }), new Promise<never>((_, reject) => { cancelConfirmation = () => reject('cancel'); })]);
    cancelConfirmation = undefined;
    result.value = await runTask<ActionReport>('trash_files', { paths, kind: 'duplicates' });
    const done = new Set(result.value.items.filter(item => item.status === 'success').map(item => item.path));
    duplicateGroups.value = duplicateGroups.value.map(group => ({ ...group, files: group.files.filter(file => !done.has(file)) })).filter(group => group.files.length > 1);
    const preserved = new Set(result.value.items.filter(item => item.status === 'skipped').map(item => item.path));
    selected.value = selected.value.filter(file => !done.has(file) && !preserved.has(file));
    if (result.value.failed || result.value.skipped) ElMessage.warning(`已处理 ${result.value.succeeded} 份，${result.value.failed} 份失败、${result.value.skipped} 份跳过；请查看处理结果`); else ElMessage.success(`已移入废纸篓 ${result.value.succeeded} 份文件`);
  } catch (cause) { if (cause !== 'cancel' && cause !== 'close') ElMessage.error(String(cause)); } finally { cancelConfirmation = undefined; removing.value = false; }
}
async function loadDuplicates() {
  if (!desktopConnected || scanning.value) return;
  scanning.value = true; loaded.value = false; selected.value = []; retained.value = {}; duplicateGroups.value = []; error.value = ''; warnings.value = [];
  try { const report = await runTask<FileScanReport<DuplicateGroup>>('scan_duplicate_files', { options: { roots: preferences.scan_roots, excluded: preferences.exclusions } }); duplicateGroups.value = report.items; warnings.value = report.warnings; scannedRoots.value = report.roots; loaded.value = true; } catch (cause) { error.value = String(cause); } finally { scanning.value = false; }
}
onMounted(async () => { await loadPreferences(); await loadDuplicates(); });
onDeactivated(() => { if (cancelConfirmation) { ElMessageBox.close(); cancelConfirmation(); cancelConfirmation = undefined; } });
</script>

<template>
  <div class="tool-page">
    <PageHeader title="重复文件" subtitle="相同的内容，只留下你想保留的那份。">
      <template #actions><ScanAction :scanning="scanning" :task="task" :disabled="!desktopConnected || removing" :hint="`已检查 ${task?.current || 0} 项`" label="重新比对" @start="loadDuplicates" /></template>
    </PageHeader>
    <div class="tool-content">
      <ScanScope :disabled="scanning || removing || !desktopConnected" />
      <ActionResults :report="result" />
      <div v-if="!desktopConnected" class="empty-state"><span class="empty-icon"><Monitor :size="30" /></span><h2>把多余的副本找出来</h2><p>请在桌面应用中打开，通过文件内容比对重复副本。</p><span class="state-label">浏览器预览 · 尚未连接桌面端</span></div>
      <div v-else-if="scanning"><ScanningRadar title="正在比对文件内容" :sub-title="task?.path || '准备扫描…'" :tips="[]" /></div>
      <template v-else>
        <el-alert v-if="error" title="重复文件比对未完成" :description="error" type="error" :closable="false" show-icon />
        <template v-if="loaded">
          <el-alert v-if="warnings.length" title="部分扫描范围未覆盖" :description="warnings.join('\n')" type="warning" :closable="false" show-icon />
          <div class="result-summary"><strong>{{ formatSize(extraSize) }}</strong><span>多余副本 · {{ duplicateGroups.length }} 组 / {{ fileCount }} 份文件</span></div>
          <div class="result-heading"><div><h2>内容相同的副本</h2><p><ShieldCheck :size="12" />每组至少保留一份，不预选任何文件。</p></div><el-input v-model="search" placeholder="搜索文件或路径" clearable :disabled="removing" class="search-input"><template #prefix><Search :size="14" /></template></el-input></div>
          <div v-for="group in filteredGroups" :key="group.hash" class="duplicate-card">
            <div class="group-heading"><div class="group-identity"><span class="group-icon"><Copy :size="18" /></span><div><h3 :title="fileName(group.files[0] || '')">{{ fileName(group.files[0] || '') }}</h3><span>{{ group.files.length }} 份内容相同 · 单份 {{ group.size_str }}</span></div></div><div><el-button text :disabled="removing" @click="selectOthers(group)">选择其余副本</el-button><span class="duplicate-size">多余 {{ formatSize(group.size_bytes * (group.files.length - 1)) }}</span></div></div>
            <div class="copy-list"><div v-for="file in group.files" :key="file" class="copy-row" :class="{ selected: selectedSet.has(file), retained: mustKeep(file, group) }"><el-checkbox :model-value="selectedSet.has(file)" :disabled="removing || mustKeep(file, group)" @change="selectFile(file, group, Boolean($event))"><span class="copy-detail"><File :size="15" /><span><strong>{{ fileName(file) }}</strong><small :title="file">{{ file }}</small></span></span></el-checkbox><div class="file-actions"><el-button v-if="!mustKeep(file, group)" text :disabled="removing" @click="keep(file, group)">保留这份</el-button><el-dropdown trigger="click"><el-button text :aria-label="`${fileName(file)}的更多操作`">更多<ChevronDown :size="13" /></el-button><template #dropdown><el-dropdown-menu><el-dropdown-item @click="reveal(file)">查看位置</el-dropdown-item><el-dropdown-item @click="copy(file)">复制路径</el-dropdown-item></el-dropdown-menu></template></el-dropdown></div><span v-if="mustKeep(file, group)" class="keep-label"><ShieldCheck :size="11" />保留</span></div></div>
            <div class="group-footer"><span :title="`SHA-256 · ${group.hash}`">内容已核对</span><span>{{ group.files.filter(file => selectedSet.has(file)).length }} 份已选择</span></div>
          </div>
          <div v-if="!filteredGroups.length" class="empty-state compact"><span class="empty-icon"><Copy :size="28" /></span><h2>{{ search ? '没有匹配的副本' : '文件，各有各的内容' }}</h2><p>{{ search ? '尝试其他名称或路径。' : '本次已读取目录中，未发现内容完全相同的重复文件。' }}</p></div>
          <p class="footnote">覆盖：{{ scannedRoots.join('、') }}。比对大于 64 KB 的文件；容量为逻辑大小，同一文件的硬链接不会产生额外可释放空间。</p>
        </template>
        <div v-else-if="!error" class="empty-state"><span class="empty-icon"><Copy :size="30" /></span><h2>发现相同的内容</h2><p>开始比对，找出常用目录中的重复文件。</p></div>
      </template>
    </div>
    <div v-if="selected.length && !scanning" class="selection-bar"><div><span>已选择 {{ selected.length }} 份副本</span><strong>{{ formatSize(selectedSize) }}</strong></div><div class="selection-actions"><el-button text @click="selectionDialog = true">查看已选</el-button><el-button text :disabled="removing" @click="selected = []">取消选择</el-button><el-button type="primary" :loading="removing" @click="removeSelected"><Trash2 :size="15" />移入废纸篓</el-button></div></div>
    <el-dialog v-model="selectionDialog" title="全部已选副本" width="70%"><div class="selection-list"><div v-for="file in selected" :key="file" class="selected-file"><span>{{ file }}</span><el-button text @click="reveal(file)">查看位置</el-button></div></div></el-dialog>
  </div>
</template>

<style scoped>
.selection-list, .developer-dialog-body { max-height: 55vh; overflow: auto; }
.file-actions { display: flex; gap: 2px; } .selected-file { display: flex; justify-content: space-between; gap: 12px; padding: 10px 0; overflow-wrap: anywhere; font-size: 12px; }
.tool-page { height: 100%; display: flex; flex-direction: column; min-width: 0; background: transparent; }
.tool-content { flex: 1; min-height: 0; overflow: auto; padding: 8px 28px 24px; }
.summary-grid { display: grid; grid-template-columns: 1.3fr 1fr 1fr; gap: 12px; margin-bottom: 24px; }
.summary-card { display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 10px; padding: 20px; background: var(--surface-subtle); border: 1px solid var(--line); border-radius: 16px; }
.summary-card > svg { grid-row: 1; grid-column: 2; color: var(--text-faint); }
.summary-card > span { font-size: 11px; color: var(--text-muted); }
.summary-card strong { grid-column: 1 / -1; font-size: 26px; color: var(--text); letter-spacing: -.7px; font-weight: 500; white-space: nowrap; }
.summary-card small { font-size: 11px; color: var(--text-faint); letter-spacing: 0; margin-left: 8px; }
.summary-card.featured { background: radial-gradient(ellipse at top right, rgba(var(--danger-rgb), 0.10), transparent 100%), rgba(var(--neutral-rgb), 0.02); border-color: var(--danger-border); }
.featured > svg { color: var(--pink); }
.result-heading { display: flex; justify-content: space-between; align-items: center; gap: 16px; margin: 0 0 16px; }
.result-heading h2 { margin: 0 0 7px; font-size: 14px; font-weight: 500; color: var(--text); }
.result-heading p { display: flex; align-items: center; gap: 5px; color: var(--text-muted); font-size: 11px; line-height: 1.6; margin: 0; }
.result-heading p svg { color: var(--cyan); }
.search-input { width: 185px; flex-shrink: 0; }
.duplicate-card { border: 1px solid var(--line); background: var(--surface-subtle); border-radius: 15px; overflow: hidden; margin-bottom: 14px; }
.group-heading { display: flex; justify-content: space-between; align-items: center; gap: 16px; padding: 17px 18px; border-bottom: 1px solid var(--line); background: var(--surface-subtle); }
.group-identity { display: flex; align-items: center; gap: 11px; min-width: 0; }
.group-icon { width: 34px; height: 34px; border-radius: 10px; display: grid; place-items: center; flex-shrink: 0; background: var(--danger-soft); color: var(--pink); border: 1px solid var(--danger-border); }
.group-identity > div { min-width: 0; }
.group-identity h3 { font-size: 12px; font-weight: 500; margin: 0 0 4px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.group-identity div span { font-size: 10px; color: var(--text-faint); }
.duplicate-size { color: var(--pink); font-size: 11px; white-space: nowrap; }
.copy-list { padding: 6px 8px; }
.copy-row { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 10px 11px; border-radius: 9px; }
.copy-row + .copy-row { margin-top: 2px; }
.copy-row.selected { background: var(--accent-soft); }
.copy-row :deep(.el-checkbox) { height: auto; margin: 0; flex: 1; min-width: 0; }
.copy-row :deep(.el-checkbox__label) { min-width: 0; flex: 1; padding-left: 12px; }
.copy-detail { display: flex; gap: 9px; align-items: center; min-width: 0; }
.copy-detail > svg { flex-shrink: 0; color: var(--text-faint); }
.copy-detail > span { min-width: 0; }
.copy-detail strong { display: block; color: var(--text-secondary); font-size: 11px; font-weight: 400; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.copy-detail small { display: block; color: var(--text-faint); font-size: 10px; margin-top: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.retained { background: var(--cyan-soft); }
.retained .copy-detail strong { color: var(--cyan); }
.keep-label { display: flex; align-items: center; gap: 4px; font-size: 10px; color: var(--cyan); white-space: nowrap; }
.group-footer { display: flex; justify-content: space-between; padding: 10px 18px; background: var(--surface-subtle); border-top: 1px solid var(--line); font-size: 9px; color: var(--text-faint); }
.footnote { color: var(--text-faint); font-size: 11px; line-height: 1.8; margin: 15px 0 0; }
.selection-bar { display: flex; justify-content: space-between; align-items: center; gap: 18px; padding: 15px 28px; border-top: 1px solid var(--accent-border); background: var(--surface-raised); flex-shrink: 0; }
.selection-bar > div { display: flex; align-items: center; gap: 16px; }
.selection-bar span { font-size: 12px; color: var(--text-secondary); }
.selection-bar strong { font-size: 17px; color: var(--text); font-weight: 500; }
.selection-actions { gap: 4px !important; }
.empty-state { min-height: 380px; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; }
.empty-state.compact { min-height: 250px; }
.empty-icon { display: grid; place-items: center; width: 78px; height: 78px; border-radius: 24px; background: linear-gradient(135deg, rgba(var(--accent-rgb), 0.13), rgba(var(--cyan-rgb), 0.06)); border: 1px solid var(--accent-border); color: var(--accent); margin-bottom: 24px; }
.empty-state h2 { color: var(--text); font-size: 20px; font-weight: 500; margin: 0 0 12px; }
.empty-state p { color: var(--text-muted); font-size: 12px; line-height: 1.8; margin: 0; }
.state-label { margin-top: 22px; padding: 6px 10px; border: 1px solid var(--line); border-radius: 20px; font-size: 10px; color: var(--text-faint); }
:deep(.el-alert) { margin-bottom: 15px; }
:deep(.el-button svg) { margin-right: 7px; }
@media (max-width: 850px) { .tool-content { padding: 8px 20px 20px; } .summary-card { padding: 16px; } .summary-card strong { font-size: 23px; } .selection-bar { padding: 15px 20px; gap: 10px; } .selection-bar > div { gap: 8px; } }
</style>
