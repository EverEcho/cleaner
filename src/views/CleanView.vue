<script setup lang="ts">
import { ref, computed, onMounted, onDeactivated, watch } from 'vue';
import { useRoute } from 'vue-router';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { ElMessage, ElMessageBox } from 'element-plus';
import { ChevronDown, ChevronUp, Folder, File, Search, Copy, Settings, Blocks, Globe, Container, Braces, RotateCw, ShieldCheck, Sparkles, HardDrive, Monitor } from 'lucide-vue-next';
import { platformInfo, loadPlatform } from '../platform';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';
import ScanScope from '../components/ScanScope.vue';
import ActionResults from '../components/ActionResults.vue';
import { activeTask, cancelTask, runTask } from '../tasks';
import { preferences, loadPreferences, savePreferences } from '../preferences';
import { copyPath, type ActionReport } from '../cleanup';

interface FileDetail { name: string; path: string; size_bytes: number; size_str: string }
interface CleanItem { id: string; path: string; name: string; size_bytes: number; size_str: string; reason: string; can_delete: boolean; selected: boolean; size_known: boolean; action?: string; details?: string; files: FileDetail[] }
interface CleanCategory { warning?: string; scan_summary?: string; id: string; title: string; desc: string; total_size_bytes: number; total_size_str: string; count: number; items: CleanItem[] }
const desktopAvailable = isTauri();
const route = useRoute();
const scanning = ref(desktopAvailable);
const cleaning = ref(false);
const authorizing = ref(false);
const categories = ref<CleanCategory[]>([]);
const developerDialog = ref(false);
const developerScanning = ref(false);
const developerDays = computed({ get: () => preferences.developer_days, set: value => { void savePreferences({ developer_days: value }); } });
const scanWarnings = computed(() => categories.value.filter(category => category.warning));
const task = computed(() => activeTask('scan_junk'));
const developerTask = computed(() => activeTask('scan_developer_junk'));
const result = ref<ActionReport | null>(null);
const search = ref('');
const sortBySize = ref(true);
const selectionDialog = ref(false);
async function openDeveloper() {
  developerDialog.value = true;

}
async function scanDeveloper() {
  developerScanning.value = true;
  try {
    const category = await runTask<CleanCategory>('scan_developer_junk', { roots: preferences.developer_roots, minAgeDays: developerDays.value, excluded: preferences.exclusions });
    const previous = new Set(categories.value.filter(cat => cat.id === 'developer').flatMap(cat => cat.items.flatMap(item => [item.path, ...item.files.map(file => file.path)])));
    selected.value = selected.value.filter(path => !previous.has(path));
    categories.value = [...categories.value.filter(cat => cat.id !== 'developer'), category];
    loadedDetails.value = new Set();
    expandedItems.value = new Set();
    expandedGroups.value = new Set([...expandedGroups.value, 'developer']);
    developerDialog.value = false;
    if (category.warning) ElMessage.warning('部分目录未覆盖，已保留扫描结果；请查看列表提示');
    else ElMessage.success(`已统计 ${category.count} 项开发者垃圾，请查看详情后勾选清理`);
  } catch (error) { ElMessage.error('开发者扫描失败: ' + error); }
  finally { developerScanning.value = false; }
}
const selected = ref<string[]>([]);
const expandedGroups = ref(new Set(['system','applications','browser','docker','developer']));
const expandedItems = ref(new Set<string>());
const loadingDetails = ref(new Set<string>());
const loadedDetails = ref(new Set<string>());
const detailLimits = ref<Record<string, number>>({});
function isDeveloperGroup(item: CleanItem) { return item.path.startsWith('developer:group:'); }
function itemChecked(item: CleanItem) { return selectedSet.value.has(item.path) || isDeveloperGroup(item) && item.files.length > 0 && item.files.every(file => selectedSet.value.has(file.path)); }
function itemPartial(item: CleanItem) { return isDeveloperGroup(item) && !itemChecked(item) && item.files.some(file => selectedSet.value.has(file.path)); }
function itemSelectedBytes(item: CleanItem) {
  if (selectedSet.value.has(item.path)) return item.size_bytes;
  return isDeveloperGroup(item) ? item.files.filter(file => selectedSet.value.has(file.path)).reduce((sum,file) => sum + file.size_bytes,0) : 0;
}
function visibleFiles(item: CleanItem) { return item.files.slice(0, detailLimits.value[item.id] ?? 100); }
function moreDetails(item: CleanItem) { detailLimits.value = { ...detailLimits.value, [item.id]: (detailLimits.value[item.id] ?? 100) + 100 }; }
function fileSelected(item: CleanItem, file: FileDetail) { return selectedSet.value.has(item.path) || selectedSet.value.has(file.path); }
function toggleDeveloperFile(item: CleanItem, file: FileDetail) {
  const set = new Set(selected.value);
  const checked = fileSelected(item,file);
  if (set.delete(item.path)) for (const child of item.files) set.add(child.path);
  if (checked) set.delete(file.path); else set.add(file.path);
  if (item.files.every(child => set.has(child.path))) { for (const child of item.files) set.delete(child.path); set.add(item.path); }
  selected.value = [...set];
}
const scanError = ref('');
const selectedSet = computed(() => new Set(selected.value));
const groups = computed(() => [
  { id: 'system', title: '系统垃圾', icon: Settings, cats: ['apt','journal','tmp','trash','system'] },
  { id: 'applications', title: '应用垃圾', icon: Blocks, cats: ['app_cache','logs'] },
  { id: 'developer', title: '开发者垃圾', icon: Braces, cats: ['developer'] },
  { id: 'browser', title: '上网垃圾', icon: Globe, cats: ['browser_cache'] },
  { id: 'docker', title: '容器垃圾', icon: Container, cats: ['docker'] },
].map(group => ({ ...group, items: categories.value.filter(cat => group.cats.includes(cat.id)).flatMap(cat => cat.items).filter(item => `${item.name} ${item.path} ${item.reason} ${item.details || ''}`.toLowerCase().includes(search.value.toLowerCase())).sort((a, b) => sortBySize.value ? b.size_bytes - a.size_bytes : a.name.localeCompare(b.name)) })).filter(group => group.items.length > 0 || (!scanning.value && !scanError.value && group.id === 'applications')));
const totalBytes = computed(() => categories.value.reduce((sum, cat) => sum + cat.total_size_bytes, 0));
const selectedBytes = computed(() => categories.value.flatMap(cat => cat.items).reduce((sum, item) => sum + itemSelectedBytes(item), 0));
const hasScanFailures = computed(() => categories.value.some(cat => cat.warning || cat.items.some(item => !item.size_known)));
const selectedDetails = computed(() => categories.value.flatMap(cat => cat.items).flatMap(item => isDeveloperGroup(item) ? item.files.filter(file => fileSelected(item, file)) : selectedSet.value.has(item.path) ? [item] : []));
const allItems = computed(() => categories.value.flatMap(cat => cat.items).filter(item => item.can_delete));
const allSelected = computed(() => allItems.value.length > 0 && allItems.value.every(itemChecked));
function size(bytes: number) {
  if (bytes >= 1024 ** 3) return (bytes / 1024 ** 3).toFixed(2) + ' GB';
  if (bytes >= 1024 ** 2) return (bytes / 1024 ** 2).toFixed(1) + ' MB';
  if (bytes >= 1024) return (bytes / 1024).toFixed(1) + ' KB';
  return bytes + ' B';
}
function groupBytes(items: CleanItem[], onlySelected = false) { return items.reduce((sum,item) => sum + (onlySelected ? itemSelectedBytes(item) : item.size_bytes),0); }
function groupChecked(items: CleanItem[]) { const eligible = items.filter(item => item.can_delete); return eligible.length > 0 && eligible.every(itemChecked); }
function groupPartial(items: CleanItem[]) { return !groupChecked(items) && items.some(item => itemChecked(item) || itemPartial(item)); }
function toggleSelection(items: CleanItem[]) {
  const set = new Set(selected.value);
  const remove = groupChecked(items);
  for (const item of items.filter(item => item.can_delete)) {
    if (remove) set.delete(item.path); else set.add(item.path);
    if (isDeveloperGroup(item)) for (const file of item.files) set.delete(file.path);
  }
  selected.value = [...set];
}
function toggleAll() { selected.value = allSelected.value ? [] : allItems.value.map(item => item.path); }
function toggleGroup(id: string) { const set = new Set(expandedGroups.value); if (set.has(id)) set.delete(id); else set.add(id); expandedGroups.value = set; }
async function toggleDetails(item: CleanItem) {
  const set = new Set(expandedItems.value);
  if (set.has(item.id)) { set.delete(item.id); expandedItems.value = set; return; }
  set.add(item.id); expandedItems.value = set;
  if (item.action || item.path.startsWith('system:') || loadedDetails.value.has(item.id)) return;
  loadingDetails.value = new Set([...loadingDetails.value,item.id]);
  try { item.files = await invoke<FileDetail[]>('get_clean_item_details', { path: item.path }); loadedDetails.value = new Set([...loadedDetails.value,item.id]); }
  catch (error) { ElMessage.error('详情读取失败: ' + error); }
  finally { const pending = new Set(loadingDetails.value); pending.delete(item.id); loadingDetails.value = pending; }
}
async function copy(path: string) { try { await copyPath(path); ElMessage.success('路径已复制'); } catch (error) { ElMessage.error(String(error)); } }
async function exclude(path: string) { await savePreferences({ exclusions: [...new Set([...preferences.exclusions, path])] }); ElMessage.success('已加入保留名单，下次扫描生效'); }
async function reveal(file: { path: string }) {
  try { await revealItemInDir(file.path); }
  catch (error) { ElMessage.error('无法查看位置: ' + error); }
}
async function authorize() {
  if (authorizing.value || cleaning.value || scanning.value) return;
  authorizing.value = true;
  try {
    const message = await invoke<string>('request_admin_authorization');
    ElMessage.success(message);
    // Refresh capability state after installation; loadPlatform's initial request is cached.
    platformInfo.value = await invoke('get_platform_info');
    if (platformInfo.value?.os === 'linux') await scan();
  } catch (error) { ElMessage.error('授权未完成: ' + error); }
  finally { authorizing.value = false; }
}
async function scan() {
  if (cleaning.value) return;
  scanning.value = true; scanError.value = '';
  categories.value = []; selected.value = []; expandedItems.value = new Set(); loadedDetails.value = new Set(); detailLimits.value = {};
  try {
    categories.value = await runTask<CleanCategory[]>('scan_junk', { excluded: preferences.exclusions });
    selected.value = categories.value.flatMap(cat => cat.items).filter(item => item.can_delete && item.selected).map(item => item.path);
  } catch (error) { scanError.value = String(error); }
  finally { scanning.value = false; }
}
let cancelConfirmation: (() => void) | undefined;
async function clean() {
  if (!selected.value.length || cleaning.value || authorizing.value || cancelConfirmation) return;
  try {
    const scope = allItems.value.filter(item => item.action && selectedSet.value.has(item.path)).map(item => `${item.name}：${item.reason}`).join('\n');
    await Promise.race([
      ElMessageBox.confirm(`清理所选 ${selected.value.length} 项？缓存和系统清理无法撤销。${scope ? '\n' + scope : ''}`, '确认清理', { type: 'warning', confirmButtonText: '立即清理', cancelButtonText: '取消' }),
      new Promise<never>((_, reject) => { cancelConfirmation = () => reject('cancel'); }),
    ]);
    cancelConfirmation = undefined;
    cleaning.value = true;
    result.value = await runTask<ActionReport>('clean_items', { paths: selected.value });
    const done = new Set(result.value.items.filter(item => item.status === 'success').map(item => item.path));
    const preserved = new Set(result.value.items.filter(item => item.status === 'skipped').map(item => item.path));
    const remaining: string[] = [];
    for (const category of categories.value) {
      category.items = category.items.flatMap(item => {
        if (done.has(item.path)) return [];
        if (isDeveloperGroup(item)) {
          const wasSelected = selectedSet.value.has(item.path);
          item.files = item.files.filter(file => !done.has(file.path));
          if (!item.files.length) return [];
          item.size_bytes = item.files.reduce((sum, file) => sum + file.size_bytes, 0); item.size_str = size(item.size_bytes);
          if (wasSelected) remaining.push(...item.files.map(file => file.path));
        } else if (selectedSet.value.has(item.path)) remaining.push(item.path);
        return [item];
      });
      category.total_size_bytes = category.items.reduce((sum, item) => sum + item.size_bytes, 0); category.count = category.items.length;
    }
    selected.value = [...new Set([...remaining, ...selected.value.filter(path => !path.startsWith('developer:group:') && !done.has(path))])].filter(path => !preserved.has(path));
    if (result.value.failed || result.value.skipped) ElMessage.warning(`已完成 ${result.value.succeeded} 项，${result.value.failed} 项失败、${result.value.skipped} 项跳过；请查看处理明细`); else ElMessage.success(`已完成 ${result.value.succeeded} 项清理`);
  } catch (error) { if (error !== 'cancel' && error !== 'close') ElMessage.error('清理失败: ' + error); }
  finally { cancelConfirmation = undefined; cleaning.value = false; }
}
watch(() => route.query.scan, value => {
  if (route.path === '/clean' && value && desktopAvailable && !scanning.value && !cleaning.value && !authorizing.value && !developerScanning.value) scan();
});
onMounted(async () => {
  if (!desktopAvailable) return;
  await Promise.all([loadPlatform().catch(() => {}), loadPreferences()]);
  await scan();
});
onDeactivated(() => {
  if (cancelConfirmation) { ElMessageBox.close(); cancelConfirmation(); cancelConfirmation = undefined; }
  if (!developerScanning.value) developerDialog.value = false;
});
</script>

<template>
  <main class="clean-page">
    <PageHeader title="垃圾清理" subtitle="找回磁盘空间，让系统轻装运行。">
      <template #actions>
        <el-button :disabled="!desktopAvailable || scanning || cleaning || authorizing || developerScanning" @click="scan"><RotateCw :size="15" />重新扫描</el-button>
      </template>
    </PageHeader>
    <div class="clean-content">
      <div v-if="!desktopAvailable" class="desktop-notice"><Monitor :size="38" /><h2>连接你的电脑，开启清理</h2><p>请在 Cleaner 桌面应用中扫描本机文件。</p><span>浏览器预览不会读取你的磁盘</span></div>
      <template v-else>
        <section class="scan-overview" :class="{ 'is-scanning': scanning }">
          <div class="overview-symbol"><HardDrive :size="28" /><span /></div>
          <div class="overview-copy"><span class="eyebrow">{{ scanning ? 'SCANNING YOUR SPACE' : 'SPACE TO RECLAIM' }}</span><h2>{{ scanError ? '扫描未完成' : scanning ? `${task?.current || 0} 个文件` : size(totalBytes) }}</h2><p>{{ scanning ? '正在发现可以释放的空间' : '扫描时逻辑大小 · 实际释放空间可能不同' }}<span v-if="!scanning && hasScanFailures" class="scan-failure"> · 部分项目待核对</span></p></div>
          <div v-if="scanning && task" class="overview-status"><el-button text @click="cancelTask(task.id)">停止扫描</el-button></div><div v-else class="overview-status"><ShieldCheck :size="16" /><span>先确认，再清理</span></div>
        </section>
        <div class="list-toolbar">
          <el-checkbox :model-value="allSelected" :disabled="scanning || cleaning || !allItems.length" @change="toggleAll">全部选择</el-checkbox>
          <div class="toolbar-actions"><button class="quiet-button" :disabled="scanning || cleaning || authorizing || developerScanning" @click="openDeveloper"><Braces :size="14" />开发者深度扫描</button><button class="quiet-button authorization-button" :disabled="scanning || cleaning || authorizing || developerScanning" @click="authorize"><ShieldCheck :size="14" />{{ authorizing ? '正在授权…' : '管理员授权' }}</button></div>
        </div>
        <div class="clean-list" :aria-busy="scanning">
          <ScanScope :disabled="scanning || cleaning || developerScanning" />
          <ActionResults :report="result" />
          <div class="filter-toolbar"><el-input v-model="search" clearable placeholder="搜索项目、路径或来源" :disabled="cleaning"><template #prefix><Search :size="14" /></template></el-input><el-switch v-model="sortBySize" active-text="按大小排序" /></div>
          <el-alert v-if="scanError" title="扫描未完成" :description="scanError" type="error" :closable="false" />
          <details v-for="category in scanWarnings" :key="category.id" class="developer-warning"><summary>扫描覆盖提示 · 查看未覆盖目录</summary><p>{{ category.scan_summary }}</p><p>{{ category.warning }}</p></details>
          <div v-if="scanning && !groups.length" class="scan-state"><ScanningRadar title="正在探索你的磁盘" :sub-title="task?.path || '准备扫描…'" :tips="[]" /><p class="scan-path" :title="task?.path">{{ task?.path }}</p></div>
          <div v-if="!scanning && !scanError && !groups.length" class="clean-empty"><Sparkles :size="35" /><h3>{{ hasScanFailures ? '扫描只覆盖了部分范围' : '空间已经很轻盈' }}</h3><p>{{ hasScanFailures ? '已读取范围内没有可清理项目，请查看覆盖提示后继续扫描。' : '本次扫描没有发现可清理的文件。' }}</p></div>
          <section v-for="group in groups" :key="group.id" class="clean-group">
            <button class="group-heading" @click="toggleGroup(group.id)"><span class="group-icon" :class="group.id"><component :is="group.icon" :size="18" /></span><span class="group-title"><strong>{{ group.title }}</strong><small>{{ group.items.length }} 个清理项目</small></span><span class="group-total">{{ size(groupBytes(group.items)) }}<small>已选 {{ size(groupBytes(group.items, true)) }}</small></span><span v-if="!group.items.length" class="clean-status">很干净</span><ChevronUp v-if="expandedGroups.has(group.id)" :size="16" class="heading-chevron" /><ChevronDown v-else :size="16" class="heading-chevron" /></button>
            <div v-if="expandedGroups.has(group.id)" class="group-items">
              <div v-if="group.items.length" class="group-select"><el-checkbox :model-value="groupChecked(group.items)" :indeterminate="groupPartial(group.items)" :disabled="!group.items.some(item => item.can_delete) || cleaning" @change="toggleSelection(group.items)">选择此分类</el-checkbox></div>
              <div v-for="item in group.items" :key="item.id" class="item-block">
                <div class="clean-row" :class="{ 'row-expanded': expandedItems.has(item.id), 'row-selected': itemChecked(item) }">
                  <el-checkbox :model-value="itemChecked(item)" :indeterminate="itemPartial(item)" :disabled="!item.can_delete || cleaning" :aria-label="item.name" @change="toggleSelection([item])" />
                  <button class="item-copy" @click="toggleDetails(item)"><span class="item-name" :title="item.name">{{ item.name }}</span><span class="item-description" :title="item.reason">{{ item.reason }}</span></button>
                  <button class="item-stat" :class="{ 'clean-status': item.size_known && !item.size_bytes && !item.files.length, 'failed-status': !item.size_known }" @click="toggleDetails(item)">{{ !item.size_known ? '扫描未完成' : item.size_bytes || item.files.length ? item.size_str : '很干净' }}</button>
                  <button class="detail-toggle" :aria-label="`${expandedItems.has(item.id) ? '收起' : '展开'}${item.name}详情`" @click="toggleDetails(item)"><ChevronUp v-if="expandedItems.has(item.id)" :size="15" /><ChevronDown v-else :size="15" /></button>
                </div>
                <div v-if="expandedItems.has(item.id)" class="item-details">
                  <p v-if="item.details" class="detail-note">{{ item.details }}</p><p v-else class="detail-note">{{ item.path }}</p>
                  <p v-if="loadingDetails.has(item.id)" class="detail-note">正在读取文件详情…</p>
                  <div v-for="file in visibleFiles(item)" :key="file.path" class="file-row"><el-checkbox v-if="isDeveloperGroup(item)" :model-value="fileSelected(item, file)" :disabled="cleaning" :aria-label="file.path" @change="toggleDeveloperFile(item,file)" /><File v-if="file.path.startsWith('docker:')" :size="16" /><Folder v-else :size="16" /><span class="file-name" :title="file.name">{{ file.name }}</span><span class="file-path" :title="file.path">{{ file.path }}</span><button v-if="!file.path.startsWith('docker:')" class="reveal-button" title="查看文件位置" @click="reveal(file)"><Search :size="14" /></button><button v-if="!file.path.startsWith('docker:')" class="reveal-button" title="复制路径" @click="copy(file.path)"><Copy :size="14" /></button><button v-if="!file.path.startsWith('docker:')" class="quiet-button" :disabled="cleaning" @click="exclude(file.path)">保留</button><span class="file-size">{{ file.size_str }}</span></div>
                  <button v-if="item.files.length > visibleFiles(item).length" class="quiet-button more-details" @click="moreDetails(item)">继续显示 100 项（{{ visibleFiles(item).length }} / {{ item.files.length }}）</button>
                  <p v-if="!loadingDetails.has(item.id) && !item.files.length && item.size_known" class="detail-note">没有符合此清理规则的文件。</p>
                </div>
              </div>
            </div>
          </section>
        </div>
      </template>
    </div>
    <footer v-if="desktopAvailable && !scanning" class="clean-action-bar"><div class="selection-copy"><span class="selection-icon"><Sparkles :size="19" /></span><div><strong>{{ size(selectedBytes) }} <small>预计可回收</small></strong><p>已选择 {{ selected.length }} 项 · 扫描时逻辑大小</p></div></div><button class="quiet-button" @click="selectionDialog = true">查看全部已选</button><button class="primary-clean" :disabled="!selected.length || cleaning || authorizing || developerScanning" @click="clean"><Sparkles :size="17" />{{ cleaning ? '正在清理…' : '释放空间' }}</button></footer>
    <el-dialog v-model="developerDialog" title="开发者深度扫描" width="520px" :close-on-click-modal="!developerScanning" :close-on-press-escape="!developerScanning" :show-close="!developerScanning">
      <div class="developer-dialog-body"><p class="developer-note">扫描历史包缓存与项目构建产物，统计实际大小和路径，结果加入本页垃圾列表。所有项目默认不勾选。</p>
      <el-form label-position="top">
        <ScanScope developer :disabled="developerScanning" />
        <el-form-item label="保留最近使用或修改的文件"><el-select v-model="developerDays" :disabled="developerScanning"><el-option v-for="days in [1,7,30,90]" :key="days" :label="`${days} 天`" :value="days" /></el-select></el-form-item>
      </el-form>
      <p class="developer-note">识别 Node.js、Rust、Maven、Gradle、Flutter、.NET、Python 项目。保留源码、锁文件、SDK、虚拟环境和配置；清理后可能需要重新下载依赖或构建。</p>
      <div v-if="developerScanning" class="developer-scan-state"><el-progress :percentage="100" :indeterminate="true" :show-text="false" /><p>已检查 {{ developerTask?.current || 0 }} 个目录</p><p class="scan-path" :title="developerTask?.path">{{ developerTask?.path }}</p><el-button v-if="developerTask" @click="cancelTask(developerTask.id)">停止扫描，保留已完成结果</el-button></div>
      </div><template #footer><el-button :disabled="developerScanning" @click="developerDialog = false">取消</el-button><el-button type="primary" :loading="developerScanning" @click="scanDeveloper">扫描并统计</el-button></template>
    </el-dialog>
    <el-dialog v-model="selectionDialog" title="全部已选项目" width="70%"><div class="selection-list"><div v-for="item in selectedDetails" :key="item.path" class="selected-file"><span>{{ item.name }} · {{ item.size_str }}<small>{{ item.path }}</small></span><el-button v-if="!item.path.startsWith('system:')" text @click="reveal(item)">查看位置</el-button></div></div></el-dialog>
  </main>
</template>

<style scoped>
.selection-list, .developer-dialog-body { max-height: 55vh; overflow: auto; }
.filter-toolbar { display: flex; align-items: center; gap: 16px; margin: 15px 0; } .filter-toolbar .el-input { max-width: 320px; } .selected-file { display: flex; justify-content: space-between; gap: 12px; padding: 10px 0; overflow-wrap: anywhere; font-size: 12px; } .selected-file small { display: block; color: #998dac; margin-top: 5px; }
:deep(.el-button svg.lucide) { margin-right: 6px; flex-shrink: 0; }
.clean-page { display: flex; flex-direction: column; height: 100%; min-width: 0; color: #f1efff; background: transparent; }
.clean-content { display: flex; flex-direction: column; flex: 1; min-height: 0; padding: 16px 28px 0; }
.scan-overview { display: flex; align-items: center; gap: 20px; padding: 22px 25px; flex-shrink: 0; border: 1px solid #ffffff0d; border-radius: 18px; background: radial-gradient(ellipse at 0% 50%, #8862df21, transparent 70%), #ffffff04; }
.overview-symbol { width: 64px; height: 64px; border: 1px solid #a78bfa45; border-radius: 20px; display: grid; place-items: center; color: #c3afff; background: linear-gradient(145deg, #9070e927, #6443ab10); box-shadow: inset 0 1px #ffffff13, 0 0 25px #8c5fe811; position: relative; }
.overview-symbol span { position: absolute; width: 6px; height: 6px; background: #64d5e9; border-radius: 50%; right: 8px; bottom: 8px; box-shadow: 0 0 8px #64d5e9; }
.overview-copy { min-width: 0; flex: 1; }
.eyebrow { color: #a79bbf; font-size: 9px; letter-spacing: 2px; }
.overview-copy h2 { margin: 5px 0 3px; font-size: 34px; line-height: 1.15; font-weight: 600; letter-spacing: -1px; }
.overview-copy p { font-size: 12px; color: #9693ad; margin: 6px 0 0; }
.overview-status { display: flex; align-items: center; gap: 7px; color: #aaa1be; font-size: 11px; white-space: nowrap; align-self: flex-start; margin-top: 4px; }
.overview-status svg { color: #64d5e9; }
.scan-failure { color: #ebbf7d; }
.list-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 17px 3px; flex-shrink: 0; flex-wrap: wrap; }
.toolbar-actions { display: flex; gap: 9px; flex-wrap: wrap; }
.quiet-button { display: inline-flex; align-items: center; gap: 7px; border: 1px solid #ffffff12; border-radius: 8px; color: #b7afca; background: #ffffff04; padding: 8px 11px; font-size: 11px; font-family: inherit; cursor: pointer; transition: background .18s, border-color .18s; }
.quiet-button:hover { background: #a78bfa16; border-color: #a78bfa40; color: #d8caff; }
.quiet-button:disabled { opacity: .4; cursor: default; }
.authorization-button svg { color: #64d5e9; }
.clean-list { flex: 1; min-height: 0; overflow: auto; padding: 0 0 22px; scrollbar-gutter: stable; }
.clean-group { overflow: hidden; border: 1px solid #ffffff0c; border-radius: 14px; background: #ffffff03; margin-bottom: 12px; }
.group-heading { width: 100%; display: flex; align-items: center; gap: 12px; border: 0; background: #ffffff02; padding: 16px 17px; font-family: inherit; text-align: left; color: #f1efff; cursor: pointer; }
.group-heading:hover { background: #ffffff05; }
.group-icon { width: 36px; height: 36px; display: grid; place-items: center; border-radius: 11px; background: #a78bfa15; color: #bca1fa; flex-shrink: 0; }
.group-icon.applications { color: #64d5e9; background: #64d5e914; }
.group-icon.browser { color: #7fabfa; background: #7fabfa14; }
.group-icon.developer { color: #e8b780; background: #e8b78014; }
.group-icon.docker { color: #adaddb; background: #adaddb14; }
.group-title { display: flex; flex-direction: column; gap: 5px; flex: 1; min-width: 0; }
.group-title strong { font-size: 13px; font-weight: 550; }
.group-title small, .group-total small { color: #89829e; font-size: 10px; }
.group-total { display: flex; flex-direction: column; gap: 5px; text-align: right; font-size: 13px; font-weight: 500; }
.group-total small { font-weight: 400; }
.heading-chevron { color: #8f85a7; margin-left: 4px; flex-shrink: 0; }
.group-select { padding: 5px 18px 5px; border-top: 1px solid #ffffff08; }
.group-select :deep(.el-checkbox__label) { font-size: 11px; color: #a397b9; }
.item-block { margin: 0 9px; }
.clean-row { display: flex; align-items: center; gap: 12px; padding: 10px 9px; min-height: 34px; font-size: 12px; border-top: 1px solid #ffffff06; border-radius: 8px; }
.clean-row:hover, .row-expanded { background: #ffffff04; }
.row-selected { background: #a78bfa08; }
.item-copy, .item-stat, .detail-toggle, .reveal-button { background: none; border: 0; padding: 0; font-family: inherit; font-size: inherit; cursor: pointer; color: inherit; }
.item-copy { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 5px; text-align: left; }
.item-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; width: 100%; }
.item-description { font-size: 10px; color: #89819d; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; width: 100%; }
.item-stat { font-size: 12px; color: #c0b2dc; white-space: nowrap; font-variant-numeric: tabular-nums; }
.detail-toggle { display: grid; place-items: center; color: #8f85a7; padding: 5px; border-radius: 5px; }
.detail-toggle:hover { color: #c2aff2; background: #a78bfa14; }
.clean-status { color: #64d5e9 !important; font-size: 11px; } .failed-status { color: #ebbf7d; }
.item-details { padding: 8px 14px 14px 40px; margin-bottom: 8px; background: #0d0b171c; border-radius: 0 0 10px 10px; }
.detail-note { font-size: 11px; color: #948aa7; white-space: pre-wrap; line-height: 1.7; margin: 6px 0 10px; overflow-wrap: anywhere; user-select: text; }
.file-row { display: flex; align-items: center; gap: 9px; min-height: 33px; font-size: 11px; border-top: 1px solid #ffffff04; }
.file-row svg { color: #a995c6; flex-shrink: 0; }
.file-name { width: 170px; max-width: 32%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.file-path { flex: 1; min-width: 0; color: #857b98; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; user-select: text; }
.file-size { width: 70px; text-align: right; flex-shrink: 0; color: #b5a8cc; font-variant-numeric: tabular-nums; }
.reveal-button { display: grid; place-items: center; padding: 5px; border-radius: 5px; }
.reveal-button:hover { background: #a78bfa18; }
.more-details { margin-top: 10px; }
.clean-action-bar { display: flex; align-items: center; justify-content: space-between; gap: 18px; flex-shrink: 0; padding: 19px 28px 22px; border-top: 1px solid #ffffff0b; background: #151220c9; backdrop-filter: blur(16px); }
.selection-copy { display: flex; align-items: center; gap: 12px; min-width: 0; }
.selection-icon { display: grid; place-items: center; width: 40px; height: 40px; border-radius: 12px; background: #a78bfa10; color: #b99cf5; }
.selection-copy strong { font-size: 23px; font-weight: 550; letter-spacing: -.5px; }
.selection-copy small { font-size: 11px; color: #aca0c1; font-weight: 400; letter-spacing: 0; margin-left: 5px; }
.selection-copy p { font-size: 10px; color: #8b819e; margin: 6px 0 0; }
.primary-clean { display: flex; align-items: center; justify-content: center; gap: 8px; height: 46px; padding: 0 26px; flex-shrink: 0; border: 1px solid #c6a8ff40; border-radius: 12px; background: linear-gradient(125deg, #a182ed, #815bd5); color: #fff; font-size: 13px; font-weight: 550; font-family: inherit; cursor: pointer; box-shadow: 0 5px 24px #9465dd29, inset 0 1px #ffffff20; transition: filter .2s, transform .2s; }
.primary-clean:hover { filter: brightness(1.12); transform: translateY(-1px); }
.primary-clean:disabled { opacity: .4; cursor: default; transform: none; box-shadow: none; }
.scan-state { position: relative; min-height: 250px; }
.scan-state :deep(.scanner-overlay) { min-height: 260px; }
.scan-path { font-size: 11px; color: #9183aa; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin: 0 auto 18px; text-align: center; width: 85%; }
.clean-empty, .desktop-notice { display: flex; flex-direction: column; align-items: center; justify-content: center; flex: 1; text-align: center; padding: 40px 20px; color: #a78bfa; }
.clean-empty h3, .desktop-notice h2 { font-size: 19px; font-weight: 500; color: #e5dcf6; margin: 20px 0 9px; }
.clean-empty p, .desktop-notice p { color: #9486ac; font-size: 13px; margin: 0; }
.desktop-notice span { margin-top: 10px; color: #6e637f; font-size: 11px; }
.developer-warning { margin: 0 0 14px; padding: 12px 15px; font-size: 12px; color: #d9b57c; background: #c49c5310; border: 1px solid #c49c5320; border-radius: 10px; overflow-wrap: anywhere; }
.developer-warning summary { cursor: pointer; }
.developer-warning p { white-space: pre-line; line-height: 1.7; }
.developer-scan-state { font-size: 12px; color: #a392bb; }
.developer-note { font-size: 12px; line-height: 1.8; color: #a398b6; }
.developer-roots { display: flex; gap: 6px; flex-wrap: wrap; margin: -8px 0 16px; }
@media (max-width: 900px) { .clean-content { padding: 16px 20px 0; } .scan-overview { padding: 18px; gap: 15px; } .overview-status { display: none; } .overview-copy h2 { font-size: 29px; } .clean-action-bar { padding: 16px 20px; } .selection-copy p { max-width: 250px; line-height: 1.6; } .primary-clean { padding: 16px 20px 0; } }
@media (prefers-reduced-motion: reduce) { .primary-clean, .quiet-button { transition: none; } }
</style>
