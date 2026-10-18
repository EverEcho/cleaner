<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, onActivated, onDeactivated } from 'vue';
import { isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ElMessage, ElMessageBox } from 'element-plus';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Search, RotateCw, Download, Monitor, ArrowUpRight, Layers, Boxes, CircleCheck, TriangleAlert, ExternalLink, Square, ListOrdered } from 'lucide-vue-next';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';
import { runTask, activeTask, cancelTask } from '../tasks';

interface UpgradablePkg { name: string; manager: string; current_version: string; new_version: string; arch: string; repository: string }
interface ToolUpdate { id: string; name: string; provider: string; current_version: string; latest_version: string; status: string; install_path: string; source_url: string; update_hint: string; note: string }
interface UpdateChannel { id: string; provider: string; state: string; message: string; checked_at: string; source_path: string; cache_seconds: number }
interface UpdateReport { packages: UpgradablePkg[]; tools: ToolUpdate[]; warnings: string[]; channels: UpdateChannel[]; checked_at: string; expires_at: string; executable_tools: string[]; automatic_managers: string[] }
interface QueueItem { id: string; name: string; provider: string; target: string; state: 'pending'|'running'|'succeeded'|'failed'|'skipped'; message: string; package?: UpgradablePkg; tool?: ToolUpdate }
const desktopAvailable = isTauri();
const checking = ref(false), refreshing = ref(false), queueRunning = ref(false), stopRequested = ref(false);
const updates = ref<UpgradablePkg[]>([]), tools = ref<ToolUpdate[]>([]), warnings = ref<string[]>([]), channels = ref<UpdateChannel[]>([]);
const checkedAt = ref(''), expiresAt = ref(''), executableTools = ref<string[]>([]), automaticManagers = ref<string[]>([]);
const selectedPackages = ref<UpgradablePkg[]>([]), selectedTools = ref<ToolUpdate[]>([]), queue = ref<QueueItem[]>([]);
const search = ref(''), filterManager = ref('all'), onlyAvailable = ref(false), errorMessage = ref('');
const currentLog = ref('准备检查安装渠道'), logs = ref<string[]>([]);
const busy = computed(() => checking.value || refreshing.value || queueRunning.value);
const checkTask = computed(() => activeTask('check_software_updates'));
const providerLabels: Record<string, string> = { 'github-release': '独立安装', 'github-branch': '源码安装', npm: 'npm', uv: 'uv', pipx: 'pipx', 'brew-cask':'Homebrew 应用', brew:'Homebrew', pacman:'Arch' };
const channelLabels: Record<string,string> = { completed:'检测完成 · 有更新', 'no-update':'检测完成 · 无更新', failed:'检测失败', unsupported:'无法自动检测', cancelled:'已停止' };
const queueLabels: Record<string,string> = { pending:'等待', running:'执行中', succeeded:'完成', failed:'失败', skipped:'已停止后续' };
const toolStatuses: Record<string, { label: string; type: 'success'|'warning'|'info'|'danger' }> = {
  'no-update':{label:'未发现更新',type:'success'}, available:{label:'有更新',type:'warning'}, current:{label:'已是最新',type:'success'}, ahead:{label:'本地版本较新',type:'info'}, unknown:{label:'按原渠道核对',type:'info'}, failed:{label:'检测未完成',type:'danger'},
};
const completedChannels = computed(() => channels.value.filter(c => ['completed','no-update'].includes(c.state)).length);
const availableManagers = computed(() => Array.from(new Set(channels.value.map(c => c.provider))));
const availableCount = computed(() => updates.value.length + tools.value.filter(t => t.status === 'available').length);
const selectedCount = computed(() => selectedPackages.value.length + selectedTools.value.length);
const filteredUpdates = computed(() => updates.value.filter(p => (filterManager.value === 'all' || p.manager === filterManager.value) && `${p.name} ${p.arch} ${p.repository}`.toLowerCase().includes(search.value.toLowerCase())));
const filteredTools = computed(() => tools.value.filter(t => (!onlyAvailable.value || t.status === 'available') && (filterManager.value === 'all' || t.provider === filterManager.value) && `${t.name} ${t.install_path}`.toLowerCase().includes(search.value.toLowerCase())));
const failedQueue = computed(() => queue.value.filter(item => item.state === 'failed'));
function packageKey(pkg: UpgradablePkg) { return `${pkg.manager}:${pkg.name}:${pkg.arch}:${pkg.repository}`; }
function timeLabel(value: string) { return value ? new Date(value).toLocaleString('zh-CN', { hour12:false }) : '尚未检测'; }
function canUpgradePackage(pkg: UpgradablePkg) { return automaticManagers.value.includes(pkg.manager); }
function canUpgradeTool(tool: ToolUpdate) { return tool.status === 'available' && executableTools.value.includes(tool.id); }
function log(message: string) { currentLog.value = message; logs.value = [...logs.value, `${new Date().toLocaleTimeString('zh-CN', {hour12:false})} ${message}`].slice(-100); }
let pageActive = true, disposed = false;
let unlisten: UnlistenFn|undefined, cancelMessageBox:(() => void)|undefined;
async function pageMessageBox(open: () => Promise<unknown>) {
  if (!pageActive || cancelMessageBox) throw 'cancel';
  try { await Promise.race([open(), new Promise<never>((_,reject) => { cancelMessageBox = () => reject('cancel'); })]); }
  finally { cancelMessageBox = undefined; }
}
async function copyHint(tool: ToolUpdate) {
  try { await navigator.clipboard.writeText(tool.update_hint); ElMessage.success('已复制原渠道更新方式'); }
  catch { try { await pageMessageBox(() => ElMessageBox.alert(tool.update_hint, `${tool.name} 更新方式`, {confirmButtonText:'关闭'})); } catch {} }
}
async function showSource(tool: ToolUpdate) { try { await openUrl(tool.source_url); } catch (error) { ElMessage.error('打开软件源失败: '+error); } }
async function checkUpdates() {
  if (checking.value || queueRunning.value) return;
  checking.value = true; errorMessage.value = ''; log('读取系统软件源与每个工具安装实例…');
  try {
    const report = await runTask<UpdateReport>('check_software_updates');
    updates.value = report.packages; tools.value = report.tools; warnings.value = report.warnings; channels.value = report.channels;
    checkedAt.value = report.checked_at; expiresAt.value = report.expires_at; executableTools.value = report.executable_tools; automaticManagers.value = report.automatic_managers;
    selectedPackages.value = []; selectedTools.value = [];
    if (!availableManagers.value.includes(filterManager.value)) filterManager.value = 'all';
    const stopped = channels.value.some(c => c.state === 'cancelled');
    log(stopped ? '检测已停止，保留已完成渠道；未检测渠道不代表没有更新' : `已完成 ${completedChannels.value} 个渠道，发现 ${availableCount.value} 项更新`);
  } catch (error) { errorMessage.value = String(error); log('检测未完成：'+error); ElMessage.error(errorMessage.value); }
  finally { checking.value = false; }
}
async function stopCheck() { const task = checkTask.value; if (task) try { await cancelTask(task.id); } catch (error) { ElMessage.error('停止失败: '+error); } }
async function refreshSources() {
  if (busy.value) return;
  refreshing.value = true; log('请求管理员授权并刷新软件源索引…');
  try { const message = await runTask<string>('refresh_software_sources'); log(message); await checkUpdates(); }
  catch (error) { errorMessage.value = String(error); log('软件源刷新未完成：'+error); ElMessage.error(errorMessage.value); }
  finally { refreshing.value = false; }
}
async function nativeHint(pkg: UpgradablePkg) {
  const instructions:Record<string,string> = { pacman:'Arch 请在终端执行完整系统升级 pacman -Syu', 'brew-cask':'请在 Homebrew 原渠道核对应用版本与更新说明后升级', brew:'请在 Homebrew 原渠道核对公式版本与更新说明后升级', zypper:'请在 Zypper 原渠道核对版本与依赖后更新', yum:'请在 YUM 原渠道核对版本与依赖后更新', dnf:'此 DNF 版本无法保证本工具的精确版本更新，请在 DNF 原渠道核对后更新' };
  try { await pageMessageBox(() => ElMessageBox.alert(instructions[pkg.manager] ?? '请在原安装渠道核对此安装实例后更新', `${pkg.name} 更新建议`, {confirmButtonText:'关闭'})); } catch {}
}
function packageQueue(pkg: UpgradablePkg): QueueItem { return { id: packageKey(pkg), name: pkg.name, provider: pkg.manager, target: pkg.new_version, state:'pending', message:'', package:pkg }; }
function toolQueue(tool: ToolUpdate): QueueItem { return { id:tool.id, name:tool.name, provider:tool.provider, target:tool.latest_version, state:'pending', message:'', tool }; }
async function runQueue(items: QueueItem[]) {
  if (busy.value || !items.length) return;
  if (items.some(item => item.package && !canUpgradePackage(item.package))) { ElMessage.warning('此渠道请按原渠道更新，无法自动锁定确认版本'); return; }
  if (items.some(item => item.tool) && new Date(expiresAt.value).getTime() <= Date.now()) { ElMessage.warning('工具检测结果已超过 10 分钟，请重新检查后升级'); return; }
  try {
    await pageMessageBox(() => ElMessageBox.confirm(
      `将依次更新 ${items.length} 个安装实例：\n${items.map(item => `${item.name} · ${providerLabels[item.provider] ?? item.provider}${item.package?.arch ? ` · ${item.package.arch}` : ''} → ${item.target}`).join('\n')}\n\n系统更新可能请求管理员授权。${items.some(item => item.provider === 'npm') ? 'npm 将通过已确认的公开原软件源执行标准安装流程，包括软件包的安装脚本。' : ''}失败会保留原因并继续后续项目；停止按钮在当前项目结束后生效。`,
      '确认更新队列', {confirmButtonText:'开始更新',cancelButtonText:'取消',type:'info'}));
  } catch { return; }
  queue.value = items.map(item => ({...item,state:'pending',message:''}));
  queueRunning.value = true; stopRequested.value = false; log(`开始串行更新 ${items.length} 个安装实例`);
  try {
    for (const item of queue.value) {
      if (stopRequested.value) { item.state = 'skipped'; item.message = '已停止后续任务，当前安装未修改'; continue; }
      item.state = 'running'; log(`开始更新 ${item.name} · ${item.provider}`);
      try {
        const message = item.tool
          ? await runTask<string>('upgrade_tool',{id:item.tool.id})
          : await runTask<string>('upgrade_software',{manager:item.package!.manager,name:item.package!.name,expectedVersion:item.package!.new_version,arch:item.package!.arch});
        item.state = 'succeeded'; item.message = message; log(message);
        if (item.package) updates.value = updates.value.filter(pkg => packageKey(pkg) !== item.id);
        if (item.tool) { const row = tools.value.find(tool => tool.id === item.id); if (row) { row.current_version = item.target; row.status = 'current'; } executableTools.value = executableTools.value.filter(id => id !== item.id); }
      } catch (error) { item.state = 'failed'; item.message = String(error); log(`${item.name} 更新未完成：${error}`); }
    }
  } finally {
    queueRunning.value = false; selectedPackages.value = []; selectedTools.value = [];
    log(`队列结束：${queue.value.filter(item => item.state === 'succeeded').length} 项完成，${failedQueue.value.length} 项失败，${queue.value.filter(item => item.state === 'skipped').length} 项未执行`);
  }
}
function upgradeSelected() { return runQueue([...selectedPackages.value.map(packageQueue),...selectedTools.value.map(toolQueue)]); }
onMounted(() => { if (!desktopAvailable) return; listen<string>('update-log', event => log(event.payload)).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(() => {}); checkUpdates(); });
onUnmounted(() => { disposed = true; unlisten?.(); });
onActivated(() => { pageActive = true; });
onDeactivated(() => { pageActive = false; if (cancelMessageBox) { ElMessageBox.close(); cancelMessageBox(); cancelMessageBox = undefined; } });
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="软件更新" subtitle="每个安装实例独立检测，按原渠道有序更新。">
      <template #actions>
        <el-button :loading="refreshing" :disabled="!desktopAvailable || checking || queueRunning" @click="refreshSources"><RotateCw :size="15" />刷新软件源</el-button>
        <el-button type="primary" :loading="checking" :disabled="!desktopAvailable || refreshing || queueRunning" @click="checkUpdates"><Download :size="15" />检查更新</el-button>
      </template>
    </PageHeader>
    <div class="subpage-content">
      <div v-if="!desktopAvailable" class="desktop-notice"><Monitor :size="38" /><h2>让你的软件保持常新</h2><p>请在 Cleaner 桌面应用中检测本机软件更新。</p><span>浏览器预览不会访问安装渠道</span></div>
      <template v-else>
        <section class="updates-summary">
          <div class="summary-main"><span class="summary-icon"><ArrowUpRight :size="23" /></span><div><strong>{{ availableCount }} <small>项可用更新</small></strong><p>最近检测 {{ timeLabel(checkedAt) }}</p></div></div>
          <div class="summary-stat"><Boxes :size="17" /><strong>{{ tools.length }}</strong><span>工具安装实例</span></div>
          <div class="summary-stat"><Layers :size="17" /><strong>{{ completedChannels }}</strong><span>完成检测的渠道</span></div>
        </section>
        <div v-if="checking || refreshing" class="update-scan-state"><ScanningRadar :title="refreshing ? '正在刷新软件源…' : '正在检查各个安装渠道…'" :sub-title="checkTask?.phase || currentLog" :tips="['同名软件按安装位置分别核对…','私有来源和固定版本保留原渠道建议…','停止后保留已完成渠道结果…']" /><el-button v-if="checking" :disabled="!checkTask || checkTask.state === 'cancelling'" @click="stopCheck"><Square :size="13" />{{ checkTask?.state === 'cancelling' ? '等待当前查询结束…' : '停止检测' }}</el-button></div>
        <el-alert v-if="errorMessage" :title="errorMessage" type="error" :closable="false" />
        <details v-if="channels.length" class="channel-panel"><summary>安装渠道覆盖 · {{ completedChannels }} 个完成 / {{ channels.length }} 个候选渠道</summary><div class="channel-grid"><article v-for="channel in channels" :key="channel.id" class="channel-card"><div><b>{{ providerLabels[channel.provider] ?? channel.provider.toUpperCase() }}</b><span :class="channel.state">{{ channelLabels[channel.state] ?? channel.state }}</span></div><p>{{ channel.message }}</p><small>{{ timeLabel(channel.checked_at) }}<template v-if="channel.cache_seconds"> · 上游缓存最多 {{ channel.cache_seconds / 60 }} 分钟</template></small><code v-if="channel.source_path">{{ channel.source_path }}</code></article></div></details>
        <details v-if="warnings.length" class="update-warnings"><summary>{{ warnings.length }} 条检测提示</summary><p v-for="warning in warnings" :key="warning">{{ warning }}</p></details>
        <section v-if="queue.length" class="queue-panel"><div class="queue-heading"><h3><ListOrdered :size="16" />更新队列</h3><el-button v-if="queueRunning" :disabled="stopRequested" size="small" @click="stopRequested = true">{{ stopRequested ? '当前项目结束后停止' : '停止后续' }}</el-button><el-button v-else-if="failedQueue.length" :disabled="checking || refreshing" size="small" @click="runQueue(failedQueue)">仅重试失败项（{{ failedQueue.length }}）</el-button></div><article v-for="item in queue" :key="item.id" class="queue-row"><span :class="item.state">{{ queueLabels[item.state] }}</span><strong>{{ item.name }}</strong><small>{{ providerLabels[item.provider] ?? item.provider }}<template v-if="item.package?.arch"> · {{ item.package.arch }}</template> → {{ item.target }}</small><p v-if="item.message">{{ item.message }}</p></article></section>
        <details v-if="logs.length" class="execution-logs" :open="queueRunning || refreshing"><summary>{{ queueRunning ? '正在执行' : '任务日志' }} · {{ currentLog }}</summary><pre>{{ logs.join('\n') }}</pre></details>
        <div class="updates-toolbar"><el-input v-model="search" placeholder="搜索软件、来源或安装路径" clearable class="update-search"><template #prefix><Search :size="15" /></template></el-input><el-select v-model="filterManager" class="manager-filter"><el-option label="所有安装渠道" value="all" /><el-option v-for="manager in availableManagers" :key="manager" :label="providerLabels[manager] ?? manager.toUpperCase()" :value="manager" /></el-select><el-button type="primary" :disabled="busy || !selectedCount" @click="upgradeSelected">更新选中（{{ selectedCount }}）</el-button></div>
        <div class="section-heading"><h3>系统软件包</h3><span>{{ updates.length }} 项候选更新</span></div>
        <p class="update-note">候选版本来自当前软件源索引。APT / DNF4 更新锁定确认版本，Flatpak 锁定范围与提交；其他渠道保留原渠道建议。未知或未完成渠道不计作无更新。</p>
        <div class="data-table"><el-table :data="filteredUpdates" :row-key="packageKey" empty-text="此范围内没有系统更新候选项" @selection-change="(rows: UpgradablePkg[]) => selectedPackages = rows">
          <el-table-column type="selection" width="44" :selectable="(row: UpgradablePkg) => canUpgradePackage(row) && !busy" />
          <el-table-column prop="name" label="软件包" min-width="150" show-overflow-tooltip />
          <el-table-column label="渠道" width="105"><template #default="{row}"><el-tag size="small">{{ row.manager.toUpperCase() }}</el-tag></template></el-table-column>
          <el-table-column prop="arch" label="架构" width="95" show-overflow-tooltip><template #default="{row}">{{ row.arch || '—' }}</template></el-table-column>
          <el-table-column prop="current_version" label="当前版本 / 提交" min-width="115" show-overflow-tooltip><template #default="{row}"><span class="old-ver">{{ row.current_version || '已安装' }}</span></template></el-table-column>
          <el-table-column prop="new_version" label="候选版本 / 提交" min-width="115" show-overflow-tooltip><template #default="{row}"><span class="new-ver">{{ row.new_version }}</span></template></el-table-column>
          <el-table-column prop="repository" label="来源 / 分支" min-width="110" show-overflow-tooltip />
          <el-table-column label="操作" width="105"><template #default="{row}"><el-button v-if="canUpgradePackage(row)" size="small" :disabled="busy" @click="runQueue([packageQueue(row)])">更新此项</el-button><el-button v-else size="small" @click="nativeHint(row)">原渠道建议</el-button></template></el-table-column>
        </el-table></div>
        <div class="tool-heading"><div class="section-heading"><h3>工具与独立软件</h3><span>{{ tools.length }} 个安装实例</span></div><el-checkbox v-model="onlyAvailable">只看有更新</el-checkbox></div>
        <p class="update-note">直接升级仅用于已确认来源和版本约束的 npm / uv / pipx 实例；检测许可有效 10 分钟。其余实例保留原安装渠道建议。</p>
        <div class="data-table"><el-table :data="filteredTools" row-key="id" empty-text="此范围内没有工具安装实例" @selection-change="(rows: ToolUpdate[]) => selectedTools = rows">
          <el-table-column type="selection" width="44" :selectable="(row: ToolUpdate) => canUpgradeTool(row) && !busy" />
          <el-table-column type="expand"><template #default="{row}"><div class="tool-details"><p><b>安装实例：</b>{{ row.install_path || '安装位置未可靠确认' }}</p><p>{{ row.note }}</p><p><b>原渠道建议：</b><code>{{ row.update_hint }}</code></p><el-button size="small" @click="copyHint(row)">复制更新方式</el-button><el-button v-if="row.source_url" size="small" @click="showSource(row)"><ExternalLink :size="13" />查看官方来源</el-button></div></template></el-table-column>
          <el-table-column prop="name" label="软件" min-width="130" show-overflow-tooltip />
          <el-table-column label="安装渠道" min-width="105"><template #default="{row}">{{ providerLabels[row.provider] ?? row.provider }}</template></el-table-column>
          <el-table-column label="当前版本" min-width="120"><template #default="{row}">{{ row.current_version || '无法确认' }}</template></el-table-column>
          <el-table-column label="上游版本" min-width="120"><template #default="{row}">{{ row.latest_version || '未检测' }}</template></el-table-column>
          <el-table-column label="状态" min-width="115"><template #default="{row}"><el-tag :type="toolStatuses[row.status]?.type ?? 'info'" size="small">{{ toolStatuses[row.status]?.label ?? '需核对' }}</el-tag></template></el-table-column>
          <el-table-column label="操作" width="110"><template #default="{row}"><el-button v-if="canUpgradeTool(row)" size="small" :disabled="busy" @click="runQueue([toolQueue(row)])">更新此项</el-button><el-button v-else size="small" @click="copyHint(row)">复制建议</el-button></template></el-table-column>
        </el-table></div>
        <div v-if="checkedAt && !checking && !availableCount && completedChannels" class="empty-status"><span class="empty-symbol"><CircleCheck :size="34" /></span><div class="empty-title">已完成检测的渠道未发现更新</div><div class="empty-sub">{{ completedChannels }} 个渠道完成检测；其余渠道的状态见上方覆盖信息。</div></div>
        <div v-if="checkedAt && !completedChannels && !checking" class="empty-status"><span class="empty-symbol is-error"><TriangleAlert :size="34" /></span><div class="empty-title">尚无渠道完成检测</div><div class="empty-sub">检查上方每个渠道的原因后重试。</div></div>
      </template>
    </div>
  </div>
</template>
<style scoped>
:deep(.el-button svg.lucide) { margin-right: 6px; flex-shrink: 0; }
.subpage-container { display: flex; flex-direction: column; height: 100%; min-width: 0; background: transparent; color: #f1efff; }
.subpage-content { flex: 1; min-height: 0; padding: 16px 28px 28px; overflow: auto; }
.updates-summary { display: flex; align-items: center; gap: 26px; padding: 22px 24px; margin-bottom: 22px; border: 1px solid #ffffff0c; border-radius: 16px; background: radial-gradient(ellipse at 0 50%, #a78bfa14, transparent 70%), #ffffff03; }
.summary-main { display: flex; align-items: center; gap: 14px; flex: 1; min-width: 0; }
.summary-icon { width: 48px; height: 48px; display: grid; place-items: center; border-radius: 15px; color: #c2a5fc; background: #a78bfa18; border: 1px solid #a78bfa24; flex-shrink: 0; }
.summary-main strong { font-size: 29px; font-weight: 550; letter-spacing: -.5px; white-space: nowrap; }
.summary-main small { font-size: 12px; color: #b5a6c8; font-weight: 400; margin-left: 6px; letter-spacing: 0; }
.summary-main p { font-size: 10px; color: #91849f; margin: 5px 0 0; }
.summary-stat { display: grid; grid-template-columns: auto auto; align-items: center; gap: 7px 9px; min-width: 70px; color: #64d5e9; }
.summary-stat strong { color: #e4dcf3; font-weight: 500; font-size: 18px; }
.summary-stat span { grid-column: 1 / -1; color: #91849f; font-size: 10px; }
.updates-toolbar { display: flex; align-items: center; gap: 12px; padding-bottom: 5px; }
.update-search { width: 270px; }
.update-search :deep(.el-input__prefix) { color: #8e80a6; }
.manager-filter { width: 160px; }
.section-heading { display: flex; align-items: center; gap: 10px; margin: 25px 2px 12px; }
h3 { font-size: 14px; color: #e1d6f0; font-weight: 500; margin: 0; }
.section-heading > span { font-size: 10px; color: #9a88b0; border: 1px solid #ffffff0c; border-radius: 5px; padding: 3px 7px; background: #ffffff03; }
.update-note { color: #91829f; font-size: 11px; line-height: 1.7; margin: 8px 2px 13px; }
.update-warnings { font-size: 12px; color: #dbb677; padding: 13px 16px; border: 1px solid #c49c5320; border-radius: 10px; background: #c49c5310; margin-top: 18px; }
.update-warnings summary { cursor: pointer; }
.update-warnings p { overflow-wrap: anywhere; line-height: 1.7; }
.tool-heading { display: flex; align-items: center; justify-content: space-between; gap: 15px; margin-top: 8px; }
.tool-heading > .el-checkbox { margin-top: 14px; }
.tool-heading :deep(.el-checkbox__label) { font-size: 11px; }
.data-table { border: 1px solid #ffffff0c; border-radius: 12px; overflow: hidden; background: #ffffff02; }
.tool-details { padding: 13px 24px; font-size: 12px; color: #a694ba; overflow-wrap: anywhere; }
.tool-details p { line-height: 1.8; margin: 6px 0 12px; }
.tool-details b { font-weight: 500; color: #c2aed8; }
.tool-details code { display: inline; color: #c8b2ed; user-select: text; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; background: #a78bfa0c; border-radius: 5px; padding: 3px 5px; }
.update-log { font-size: 11px; color: #9183aa; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; text-align: center; margin: 0 auto 20px; max-width: 90%; }
.old-ver { color: #9184a1; font-variant-numeric: tabular-nums; }
.new-ver { color: #64d5e9; font-weight: 500; font-variant-numeric: tabular-nums; }
.empty-status { min-height: 300px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; padding: 35px 15px; text-align: center; }
.empty-symbol { display: grid; place-items: center; width: 74px; height: 74px; border: 1px solid #64d5e926; border-radius: 24px; color: #64d5e9; background: #64d5e90c; box-shadow: 0 0 35px #64d5e908; margin-bottom: 8px; }
.empty-symbol.is-error { color: #dfb77b; border-color: #dfb77b20; background: #dfb77b0b; }
.empty-title { font-size: 18px; font-weight: 500; color: #e7ddf4; }
.empty-sub { font-size: 12px; color: #9583aa; line-height: 1.8; max-width: 500px; overflow-wrap: anywhere; user-select: text; }
.desktop-notice { min-height: 400px; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; padding: 40px 20px; color: #a78bfa; }
.desktop-notice h2 { font-size: 19px; font-weight: 500; color: #e5dcf6; margin: 20px 0 9px; }
.desktop-notice p { color: #9486ac; font-size: 13px; margin: 0; }
.desktop-notice span { margin-top: 10px; color: #6e637f; font-size: 11px; }
@media (max-width: 900px) { .subpage-content { padding: 16px 20px 22px; } .updates-summary { padding: 20px 18px; gap: 18px; } .summary-main { gap: 12px; } .summary-main strong { font-size: 25px; } .summary-main small { font-size: 11px; } .summary-stat { min-width: 57px; } }

.channel-panel,.queue-panel,.execution-logs { margin:18px 0; padding:16px; border:1px solid #ffffff10; border-radius:14px; background:#ffffff03; }
.channel-panel>summary,.execution-logs>summary { cursor:pointer; font-size:12px; color:#c6b7e1; overflow-wrap:anywhere; }
.channel-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(250px,1fr)); gap:10px; margin-top:14px; }
.channel-card { border:1px solid #ffffff09; border-radius:10px; padding:12px; min-width:0; }
.channel-card>div { display:flex; align-items:center; justify-content:space-between; gap:10px; font-size:11px; }
.channel-card b { font-weight:500; color:#e1d7f2; }.channel-card span { color:#b7a6ce; }.channel-card .completed,.channel-card .no-update { color:#72d6c0; }.channel-card .failed { color:#f49da7; }.channel-card .cancelled { color:#dab985; }
.channel-card p { color:#a493ba;font-size:11px;line-height:1.7;margin:8px 0; }.channel-card small { font-size:10px;color:#827391; }.channel-card code { display:block; font-size:10px; margin-top:8px;color:#8b7b9f;overflow-wrap:anywhere;user-select:text; }
.queue-heading { display:flex;align-items:center;justify-content:space-between;gap:12px;margin-bottom:12px; }.queue-heading h3 { display:flex;gap:8px;align-items:center; }
.queue-row { display:flex;flex-wrap:wrap;gap:10px;align-items:center;padding:11px 0;border-top:1px solid #ffffff09;font-size:11px; }.queue-row>span { border-radius:5px;padding:3px 7px;background:#ffffff06;color:#a393b6; }.queue-row .running { color:#c3a3fa; }.queue-row .succeeded { color:#72d6c0; }.queue-row .failed { color:#f49da7; }.queue-row strong { font-weight:500;color:#dccfea; }.queue-row small { color:#8f7eaa; }.queue-row p { flex-basis:100%;margin:2px 0 0;color:#ae9cc4;overflow-wrap:anywhere;line-height:1.6;user-select:text; }
.execution-logs pre { font-size:11px;line-height:1.8;color:#a89abd;max-height:260px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere;user-select:text; }.update-scan-state { text-align:center; }.updates-toolbar { flex-wrap:wrap;margin-top:24px; }
</style>
