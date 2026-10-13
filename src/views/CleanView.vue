<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useRouter } from 'vue-router';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { ElMessage, ElMessageBox } from 'element-plus';
import { ChevronDown, ChevronUp, Folder, File, Search, Settings, Blocks, Globe, Container, Braces } from 'lucide-vue-next';
import { platformInfo, loadPlatform } from '../platform';
import LemonMark from '../layout/LemonMark.vue';

interface FileDetail { name: string; path: string; size_bytes: number; size_str: string }
interface CleanItem { id: string; path: string; name: string; size_bytes: number; size_str: string; reason: string; can_delete: boolean; selected: boolean; size_known: boolean; action?: string; details?: string; files: FileDetail[] }
interface CleanCategory { warning?: string; scan_summary?: string; id: string; title: string; desc: string; total_size_bytes: number; total_size_str: string; count: number; items: CleanItem[] }
const router = useRouter();
const scanning = ref(true);
const cleaning = ref(false);
const authorizing = ref(false);
const categories = ref<CleanCategory[]>([]);
const developerDialog = ref(false);
const developerScanning = ref(false);
const developerRoot = ref('');
const developerRoots = ref<string[]>([]);
const developerDays = ref(7);
const developerProgress = ref({ path: '', directories_scanned: 0, projects_found: 0 });
const developerWarnings = computed(() => categories.value.filter(category => category.id === 'developer' && category.warning));
let unlistenDeveloper: UnlistenFn | undefined;
async function openDeveloper() {
  developerDialog.value = true;
  try { developerRoots.value = await invoke<string[]>('get_developer_roots'); } catch {}
}
async function scanDeveloper() {
  developerScanning.value = true;
  developerProgress.value = { path: '准备扫描工具缓存与项目目录…', directories_scanned: 0, projects_found: 0 };
  try {
    const category = await invoke<CleanCategory>('scan_developer_junk', { root: developerRoot.value.trim() || null, minAgeDays: developerDays.value });
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
const scanPath = ref('准备扫描…');
const discoveredBytes = ref(0);
const scanError = ref('');
let unlisten: UnlistenFn | undefined;
let disposed = false;
const selectedSet = computed(() => new Set(selected.value));
const groups = computed(() => [
  { id: 'system', title: '系统垃圾', icon: Settings, cats: ['apt','journal','tmp','trash','system'] },
  { id: 'applications', title: '应用垃圾', icon: Blocks, cats: ['app_cache','logs'] },
  { id: 'developer', title: '开发者垃圾', icon: Braces, cats: ['developer'] },
  { id: 'browser', title: '上网垃圾', icon: Globe, cats: ['browser_cache'] },
  { id: 'docker', title: '容器垃圾', icon: Container, cats: ['docker'] },
].map(group => ({ ...group, items: categories.value.filter(cat => group.cats.includes(cat.id)).flatMap(cat => cat.items) })).filter(group => group.items.length > 0 || (!scanning.value && !scanError.value && group.id === 'applications')));
const totalBytes = computed(() => categories.value.reduce((sum, cat) => sum + cat.total_size_bytes, 0));
const selectedBytes = computed(() => categories.value.flatMap(cat => cat.items).reduce((sum, item) => sum + itemSelectedBytes(item), 0));
const hasScanFailures = computed(() => categories.value.some(cat => cat.warning || cat.items.some(item => !item.size_known)));
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
async function reveal(file: FileDetail) {
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
  scanning.value = true; scanError.value = ''; scanPath.value = '准备扫描…'; discoveredBytes.value = 0;
  categories.value = []; selected.value = []; expandedItems.value = new Set(); loadedDetails.value = new Set(); detailLimits.value = {};
  try {
    categories.value = await invoke<CleanCategory[]>('scan_junk');
    selected.value = categories.value.flatMap(cat => cat.items).filter(item => item.can_delete && item.selected).map(item => item.path);
  } catch (error) { scanError.value = String(error); }
  finally { scanning.value = false; }
}
async function clean() {
  if (!selected.value.length || cleaning.value || authorizing.value) return;
  try {
    const scope = allItems.value.filter(item => item.action && selectedSet.value.has(item.path)).map(item => `${item.name}：${item.reason}`).join('\n');
    await ElMessageBox.confirm(`清理所选 ${selected.value.length} 项？缓存和系统清理无法撤销。${scope ? '\n' + scope : ''}`, '确认清理', { type: 'warning', confirmButtonText: '立即清理', cancelButtonText: '取消' });
    cleaning.value = true;
    const count = await invoke<number>('clean_items', { paths: selected.value });
    ElMessage.success(`已完成 ${count} 项清理`);
    cleaning.value = false; await scan();
  } catch (error) { if (error !== 'cancel' && error !== 'close') ElMessage.error('清理失败: ' + error); }
  finally { cleaning.value = false; }
}
onMounted(async () => {
  await loadPlatform().catch(() => {});
  if (isTauri()) { try {
    const stop = await listen<{path: string; total_size_bytes: number}>('scan-progress', event => { scanPath.value = event.payload.path; discoveredBytes.value = event.payload.total_size_bytes; });
    if (disposed) { stop(); return; } unlisten = stop;
    const developerStop = await listen<{path: string; directories_scanned: number; projects_found: number}>('developer-scan-progress', event => { developerProgress.value = event.payload; });
    if (disposed) { developerStop(); return; } unlistenDeveloper = developerStop;
  } catch {} }
  await scan();
});
onUnmounted(() => { disposed = true; unlisten?.(); unlistenDeveloper?.(); });
</script>

<template>
  <main class="clean-page">
    <header class="clean-hero">
      <div class="hero-mark"><LemonMark /></div>
      <div class="hero-copy">
        <div class="hero-title-row"><h1>{{ scanError ? '扫描未完成' : scanning ? '已发现可清理文件' : '共发现可清理文件' }} <template v-if="!scanError">{{ size(scanning ? discoveredBytes : totalBytes) }}</template></h1><button class="quiet-button" @click="router.push('/')">返回</button></div>
        <template v-if="scanning"><div class="scan-track"><span /></div><p class="scan-path">正在扫描 {{ scanPath }}</p></template>
        <template v-else><p class="selection-summary">已选择垃圾 <strong>{{ size(selectedBytes) }}</strong><span v-if="hasScanFailures" class="scan-failure"> · 部分项目扫描未完成</span></p><p class="hero-note">先查看清理详情，系统垃圾与容器数据按各自规则处理。</p></template>
      </div>
      <button v-if="!scanning" class="primary-clean" :disabled="!selected.length || cleaning || authorizing || developerScanning" @click="clean">{{ cleaning ? '正在清理…' : '立即清理' }}</button>
    </header>
    <div class="list-toolbar"><el-checkbox :model-value="allSelected" :disabled="scanning || cleaning || !allItems.length" @change="toggleAll">全部选择</el-checkbox><button class="quiet-button" :disabled="scanning || cleaning || authorizing || developerScanning" @click="scan">重新扫描</button><button class="quiet-button" :disabled="scanning || cleaning || authorizing || developerScanning" @click="openDeveloper">开发者深度扫描</button><button class="authorization-button" :disabled="scanning || cleaning || authorizing || developerScanning" @click="authorize">{{ authorizing ? '正在授权…' : '管理员授权' }}</button></div>
    <div class="clean-list" :aria-busy="scanning">
      <el-alert v-if="scanError" title="扫描未完成" :description="scanError" type="error" :closable="false" />
      <details v-for="category in developerWarnings" :key="category.id" class="developer-warning"><summary>开发者扫描已保留部分结果 · 查看未覆盖目录</summary><p>{{ category.scan_summary }}</p><p>{{ category.warning }}</p></details>
      <div v-if="scanning && !groups.length" class="scan-placeholder">正在逐项扫描文件与清理规则，请稍候…</div>
      <section v-for="group in groups" :key="group.id" class="clean-group">
        <button class="group-heading" @click="toggleGroup(group.id)"><component :is="group.icon" :size="18" /><strong>{{ group.title }}</strong><span>共 {{ size(groupBytes(group.items)) }}，已选择 <b>{{ size(groupBytes(group.items, true)) }}</b></span><span v-if="!group.items.length" class="clean-status">很干净</span><ChevronUp v-if="expandedGroups.has(group.id)" :size="16" class="heading-chevron" /><ChevronDown v-else :size="16" class="heading-chevron" /></button>
        <div v-if="expandedGroups.has(group.id)" class="group-items">
          <div v-if="group.items.length" class="group-select"><el-checkbox :model-value="groupChecked(group.items)" :indeterminate="groupPartial(group.items)" :disabled="!group.items.some(item => item.can_delete) || cleaning" @change="toggleSelection(group.items)">选择此分类</el-checkbox></div>
          <div v-for="item in group.items" :key="item.id" class="item-block">
            <div class="clean-row" :class="{ 'row-expanded': expandedItems.has(item.id) }">
              <el-checkbox :model-value="itemChecked(item)" :indeterminate="itemPartial(item)" :disabled="!item.can_delete || cleaning" :aria-label="item.name" @change="toggleSelection([item])" />
              <button class="item-name" :title="item.name" @click="toggleDetails(item)">{{ item.name }}</button>
              <span class="item-description" :title="item.reason">{{ item.reason }}</span>
              <button class="item-stat" :class="{ 'clean-status': item.size_known && !item.size_bytes && !item.files.length, 'failed-status': !item.size_known }" @click="toggleDetails(item)">{{ !item.size_known ? '扫描未完成' : item.size_bytes || item.files.length ? item.size_str : '很干净' }}</button>
              <button class="detail-toggle" :aria-label="`${expandedItems.has(item.id) ? '收起' : '展开'}${item.name}详情`" @click="toggleDetails(item)"><ChevronUp v-if="expandedItems.has(item.id)" :size="15" /><ChevronDown v-else :size="15" /></button>
            </div>
            <div v-if="expandedItems.has(item.id)" class="item-details">
              <p v-if="item.details" class="detail-note">{{ item.details }}</p><p v-else class="detail-note">{{ item.path }}</p>
              <p v-if="loadingDetails.has(item.id)" class="detail-note">正在读取文件详情…</p>
              <div v-for="file in visibleFiles(item)" :key="file.path" class="file-row"><el-checkbox v-if="isDeveloperGroup(item)" :model-value="fileSelected(item, file)" :disabled="cleaning" :aria-label="file.path" @change="toggleDeveloperFile(item,file)" /><File v-if="file.path.startsWith('docker:')" :size="16" /><Folder v-else :size="16" /><span class="file-name" :title="file.name">{{ file.name }}</span><span class="file-path" :title="file.path">{{ file.path }}</span><button v-if="!file.path.startsWith('docker:')" class="reveal-button" title="查看文件位置" @click="reveal(file)"><Search :size="14" /></button><span class="file-size">{{ file.size_str }}</span></div>
              <button v-if="item.files.length > visibleFiles(item).length" class="quiet-button more-details" @click="moreDetails(item)">继续显示 100 项（{{ visibleFiles(item).length }} / {{ item.files.length }}）</button>
              <p v-if="!loadingDetails.has(item.id) && !item.files.length && item.size_known" class="detail-note">没有符合此清理规则的文件。</p>
            </div>
          </div>
        </div>
      </section>
    </div>
    <el-dialog v-model="developerDialog" title="开发者深度扫描" width="520px" :close-on-click-modal="!developerScanning" :close-on-press-escape="!developerScanning" :show-close="!developerScanning">
      <p class="developer-note">扫描历史包缓存与项目构建产物，统计实际大小和路径，结果加入本页垃圾列表。所有项目默认不勾选。</p>
      <el-form label-position="top">
        <el-form-item label="项目目录（留空只扫描工具缓存）"><el-input v-model="developerRoot" placeholder="如 /home/me/Projects 或 /Users/me/Developer" :disabled="developerScanning" /></el-form-item>
        <div v-if="developerRoots.length" class="developer-roots"><button v-for="root in developerRoots" :key="root" class="quiet-button" :disabled="developerScanning" @click="developerRoot = root">{{ root }}</button></div>
        <el-form-item label="保留最近使用或修改的文件"><el-select v-model="developerDays" :disabled="developerScanning"><el-option v-for="days in [1,7,30,90]" :key="days" :label="`${days} 天`" :value="days" /></el-select></el-form-item>
      </el-form>
      <p class="developer-note">识别 Node.js、Rust、Maven、Gradle、Flutter、.NET、Python 项目。保留源码、锁文件、SDK、虚拟环境和配置；清理后可能需要重新下载依赖或构建。</p>
      <div v-if="developerScanning" class="developer-scan-state"><el-progress :percentage="100" :indeterminate="true" :show-text="false" /><p>已检查 {{ developerProgress.directories_scanned }} 个目录，识别 {{ developerProgress.projects_found }} 个候选项目目录</p><p class="scan-path" :title="developerProgress.path">{{ developerProgress.path }}</p></div>
      <template #footer><el-button :disabled="developerScanning" @click="developerDialog = false">取消</el-button><el-button type="primary" :loading="developerScanning" @click="scanDeveloper">扫描并统计</el-button></template>
    </el-dialog>
  </main>
</template>

<style scoped>
.more-details { margin-top: 10px; }
.developer-warning { margin: 8px 60px; padding: 10px 14px; font-size: 12px; color: #ad7f2d; background: #fffbf1; border-radius: 3px; overflow-wrap: anywhere; }
.developer-warning summary { cursor: pointer; }
.developer-warning p { white-space: pre-line; line-height: 1.7; }
.developer-scan-state { font-size: 12px; color: #92989c; }
.developer-note { font-size: 12px; line-height: 1.8; color: #8b9297; }
.developer-roots { display: flex; gap: 6px; flex-wrap: wrap; margin: -8px 0 16px; }
.clean-page { display: flex; flex-direction: column; height: 100%; background: #fff; color: #444; }
.clean-hero { display: flex; align-items: center; gap: 20px; padding: 10px 54px 18px; min-height: 145px; flex-shrink: 0; }
.hero-mark { width: 120px; height: 120px; flex-shrink: 0; }
.hero-copy { flex: 1; min-width: 0; }
.hero-title-row { display: flex; align-items: center; gap: 16px; }
h1 { font-size: 27px; font-weight: 500; line-height: 1.4; margin: 0 0 8px; color: #2d2d2d; white-space: nowrap; }
.quiet-button { background: #fff; border: 1px solid #e2e2e2; border-radius: 1px; color: #9a9da1; font-size: 11px; padding: 4px 14px; cursor: pointer; white-space: nowrap; }
.quiet-button:disabled { opacity: .45; cursor: default; }
.selection-summary { margin: 0 0 7px; font-size: 13px; color: #92959b; }
.selection-summary strong { color: #ffad19; font-weight: 400; }
.hero-note { color: #a0a3a8; font-size: 11px; margin: 0; }
.primary-clean { width: 190px; height: 58px; flex-shrink: 0; border: 0; border-radius: 2px; background: linear-gradient(110deg,#51dfad,#00ce95); color: #fff; font-size: 25px; cursor: pointer; }
.primary-clean:disabled { opacity: .45; cursor: default; }
.scan-track { height: 5px; border-radius: 4px; background: #efefef; overflow: hidden; margin-top: 5px; }
.scan-track span { display: block; width: 40%; height: 100%; background: #37dbaa; animation: scan-flow 1.8s infinite ease-in-out; }
.scan-path { font-size: 12px; color: #999ca1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin: 10px 0 0; }
.scan-failure { color: #d89835; font-size: 12px; }
.list-toolbar { margin: 0 60px; display: flex; align-items: center; gap: 14px; padding: 9px 0; border-top: 1px solid #ededed; }
.authorization-button { margin-left: auto; border: 1px solid #d5eee4; border-radius: 2px; padding: 5px 13px; background: #f4fcf8; color: #00ac7b; font-size: 12px; cursor: pointer; }
.authorization-button:disabled { opacity: .5; cursor: default; }
.clean-list { flex: 1; min-height: 0; overflow: auto; padding-bottom: 22px; }
.group-heading { width: 100%; display: flex; align-items: center; gap: 12px; border: 0; background: #fff; padding: 15px 70px; font-family: inherit; text-align: left; color: #ffaa00; cursor: pointer; }
.group-heading strong { font-size: 14px; font-weight: 500; }
.group-heading span { color: #a1a3a8; font-size: 12px; }
.group-heading b { color: #ffaa00; font-weight: 400; }
.heading-chevron { margin-left: auto; color: #999da3; }
.group-select { padding: 0 100px 5px; }
.group-select :deep(.el-checkbox__label) { color: #acafb4; font-size: 11px; }
.clean-row { display: flex; align-items: center; gap: 10px; min-height: 34px; padding: 0 66px 0 100px; font-size: 12px; }
.clean-row:hover, .row-expanded { background: #f3f3f3; }
.item-name, .item-stat, .detail-toggle, .reveal-button { background: none; border: 0; padding: 0; font-family: inherit; font-size: inherit; cursor: pointer; color: inherit; }
.item-name { max-width: 210px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; }
.item-description { flex: 1; min-width: 0; font-size: 11px; color: #a1a4aa; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.item-stat { margin-left: auto; white-space: nowrap; font-size: 11px; color: #62666b; }
.detail-toggle { display: grid; place-items: center; color: #93979d; }
.clean-status { color: #00c28a !important; } .failed-status { color: #e6a03d; }
.item-details { padding: 5px 66px 12px 128px; background: #fdfdfd; }
.detail-note { font-size: 11px; color: #9da1a7; white-space: pre-wrap; line-height: 1.7; margin: 6px 0 10px; overflow-wrap: anywhere; user-select: text; }
.file-row { display: flex; align-items: center; gap: 9px; min-height: 32px; font-size: 12px; }
.file-row svg { color: #9aa4a8; flex-shrink: 0; }
.file-name { width: 220px; max-width: 35%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.file-path { flex: 1; min-width: 0; color: #9a9da3; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; user-select: text; }
.file-size { width: 74px; text-align: right; flex-shrink: 0; color: #696d72; }
.reveal-button { display: grid; place-items: center; padding: 4px; }
.scan-placeholder { text-align: center; color: #a1a5ab; font-size: 13px; padding: 65px; }
@keyframes scan-flow { from { transform: translateX(-110%); } to { transform: translateX(350%); } }
@media(max-width: 950px) { .clean-hero { padding-left: 28px; padding-right: 28px; gap: 12px; } h1 { font-size: 23px; } .hero-mark { width: 100px; height: 100px; } .primary-clean { width: 154px; font-size: 22px; } .clean-row { padding-right: 36px; } }
@media(prefers-reduced-motion: reduce) { .scan-track span { animation: none; } }
</style>
