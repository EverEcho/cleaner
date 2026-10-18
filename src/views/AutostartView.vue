<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import { Info, Monitor, Power, RefreshCw, Rocket, Search, ShieldCheck } from 'lucide-vue-next';
import { platformInfo, loadPlatform } from '../platform';
import PageHeader from '../layout/PageHeader.vue';

interface AutostartItem { path: string; name: string; exec: string; comment: string; enabled: boolean; is_user: boolean }
const desktopConnected = isTauri();
const scanning = ref(false);
const loaded = ref(false);
const error = ref('');
const search = ref('');
const toggling = ref(false);
const autostartList = ref<AutostartItem[]>([]);
const enabledCount = computed(() => autostartList.value.filter(item => item.enabled).length);
const filtered = computed(() => autostartList.value.filter(item => item.name.toLowerCase().includes(search.value.toLowerCase())));

async function loadAutostart() {
  if (!desktopConnected || scanning.value) return;
  scanning.value = true;
  loaded.value = false;
  autostartList.value = [];
  error.value = '';
  try {
    await loadPlatform();
    autostartList.value = await invoke<AutostartItem[]>('get_autostart_items');
    loaded.value = true;
  } catch (cause) { error.value = String(cause); }
  finally { scanning.value = false; }
}
async function handleToggleAutostart(item: AutostartItem) {
  const enabled = item.enabled;
  toggling.value = true;
  try {
    await invoke('toggle_autostart_item', { path: item.path, enable: enabled });
    ElMessage.success(`已${enabled ? '启用' : '禁用'}启动项：${item.name}`);
  } catch (cause) {
    item.enabled = !enabled;
    ElMessage.error(`修改失败：${String(cause)}`);
    toggling.value = false;
    return;
  }
  await loadAutostart();
  toggling.value = false;
}
onMounted(() => { void loadAutostart(); });
</script>

<template>
  <div class="tool-page">
    <PageHeader title="启动管理" subtitle="让需要的应用随你启动，其他的留待稍后。">
      <template #actions><el-button :loading="scanning" :disabled="!desktopConnected || toggling" @click="loadAutostart"><RefreshCw :size="14" />刷新列表</el-button></template>
    </PageHeader>
    <div class="tool-content">
      <div v-if="!desktopConnected" class="empty-state"><span class="empty-icon"><Monitor :size="30" /></span><h2>让开机更从容</h2><p>请在桌面应用中打开，查看并管理本机的登录启动项。</p><span class="state-label">浏览器预览 · 尚未连接桌面端</span></div>
      <template v-else>
        <div class="summary-grid">
          <div class="summary-card featured"><Rocket :size="20" /><span>已启用</span><strong>{{ loaded ? enabledCount : '—' }}<small>项</small></strong></div>
          <div class="summary-card"><Power :size="20" /><span>已禁用</span><strong>{{ loaded ? autostartList.length - enabledCount : '—' }}<small>项</small></strong></div>
          <div class="summary-card"><ShieldCheck :size="20" /><span>启动项总数</span><strong>{{ loaded ? autostartList.length : '—' }}<small>项</small></strong></div>
        </div>
        <el-alert v-if="error" title="启动项读取未完成" :description="error" type="error" :closable="false" show-icon />
        <div v-if="platformInfo?.autostart_note" class="platform-note"><Info :size="15" /><span>{{ platformInfo.autostart_note }}</span></div>
        <div class="result-heading"><div><h2>登录时启动的应用</h2><p>关闭开关后，该应用将不再随登录自动启动。</p></div><el-input v-model="search" placeholder="搜索启动项" clearable class="search-input"><template #prefix><Search :size="14" /></template></el-input></div>
        <div v-loading="scanning" class="table-panel">
          <el-table :data="filtered" :empty-text="scanning ? '正在读取启动项…' : loaded ? (search ? '没有匹配的启动项' : '未发现登录启动项') : '尚未读取启动项'">
            <el-table-column label="应用" min-width="185"><template #default="{ row }"><div class="app-cell"><span class="app-icon"><Power :size="16" /></span><div><strong>{{ row.name }}</strong><span v-if="row.comment" :title="row.comment">{{ row.comment }}</span></div></div></template></el-table-column>
            <el-table-column prop="exec" label="启动命令" min-width="170" show-overflow-tooltip />
            <el-table-column label="范围" width="86"><template #default="{ row }"><span class="scope-tag" :class="{ system: !row.is_user }">{{ row.is_user ? '当前用户' : '系统级' }}</span></template></el-table-column>
            <el-table-column label="状态" width="96" align="center"><template #default="{ row }"><el-tooltip :content="platformInfo?.os === 'macos' && !row.is_user ? '系统级启动项由 macOS 管理' : row.enabled ? '点击禁用' : '点击启用'"><el-switch v-model="row.enabled" :disabled="scanning || toggling || (platformInfo?.os === 'macos' && !row.is_user)" style="--el-switch-on-color: #a78bfa; --el-switch-off-color: #373348" :aria-label="`${row.name}登录启动`" @change="handleToggleAutostart(row)" /></el-tooltip></template></el-table-column>
          </el-table>
        </div>
        <p class="footnote">启动项开关仅调整登录启动状态，不会卸载应用。</p>
      </template>
    </div>
  </div>
</template>

<style scoped>
.tool-page { height: 100%; display: flex; flex-direction: column; min-width: 0; background: transparent; }
.tool-content { flex: 1; min-height: 0; overflow: auto; padding: 8px 28px 24px; }
.summary-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; margin-bottom: 22px; }
.summary-card { display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 10px; padding: 20px; background: #ffffff04; border: 1px solid #ffffff0d; border-radius: 16px; }
.summary-card > svg { grid-row: 1; grid-column: 2; color: #79728f; }
.summary-card > span { font-size: 11px; color: #9693ad; }
.summary-card strong { grid-column: 1 / -1; font-size: 28px; color: #f1efff; letter-spacing: -.7px; font-weight: 500; }
.summary-card small { font-size: 11px; color: #817b98; letter-spacing: 0; margin-left: 8px; }
.summary-card.featured { background: radial-gradient(ellipse at top right, #a78bfa22, transparent 100%), #ffffff04; border-color: #a78bfa25; }
.featured > svg { color: #b5a0ef; }
.platform-note { display: flex; gap: 9px; color: #a59dbc; font-size: 11px; line-height: 1.8; padding: 12px 14px; background: #a78bfa08; border-radius: 10px; margin-bottom: 22px; }
.platform-note svg { flex-shrink: 0; margin-top: 2px; color: #a78bfa; }
.result-heading { display: flex; justify-content: space-between; align-items: center; gap: 16px; margin: 0 0 15px; }
.result-heading h2 { margin: 0 0 6px; font-size: 14px; font-weight: 500; color: #f1efff; }
.result-heading p { color: #9693ad; font-size: 11px; line-height: 1.6; margin: 0; }
.search-input { width: 185px; flex-shrink: 0; }
.table-panel { border: 1px solid #ffffff0d; border-radius: 14px; overflow: hidden; }
.app-cell { display: flex; gap: 11px; align-items: center; min-width: 0; }
.app-icon { width: 32px; height: 32px; display: grid; place-items: center; flex-shrink: 0; background: #a78bfa0d; border: 1px solid #a78bfa12; color: #aa9ccf; border-radius: 10px; }
.app-cell > div { min-width: 0; }
.app-cell strong { display: block; font-size: 12px; font-weight: 500; color: #e5dff7; }
.app-cell div span { display: block; font-size: 10px; color: #817b97; max-width: 185px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 3px; }
.scope-tag { font-size: 10px; color: #aba3c7; border: 1px solid #a78bfa1a; background: #a78bfa08; border-radius: 5px; padding: 3px 7px; white-space: nowrap; }
.scope-tag.system { color: #b4ad9f; border-color: #e8c08416; background: #e8c08408; }
.footnote { color: #817c99; font-size: 11px; line-height: 1.8; margin: 15px 0 0; }
.empty-state { min-height: 380px; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; }
.empty-icon { display: grid; place-items: center; width: 78px; height: 78px; border-radius: 24px; background: linear-gradient(135deg, #a78bfa22, #64d5e90b); border: 1px solid #a78bfa22; color: #b5a0ef; margin-bottom: 24px; }
.empty-state h2 { color: #ede9fb; font-size: 20px; font-weight: 500; margin: 0 0 12px; }
.empty-state p { color: #9693ad; font-size: 12px; line-height: 1.8; margin: 0; }
.state-label { margin-top: 22px; padding: 6px 10px; border: 1px solid #ffffff0e; border-radius: 20px; font-size: 10px; color: #807b95; }
:deep(.el-alert) { margin-bottom: 15px; }
:deep(.el-button svg) { margin-right: 7px; }
@media (max-width: 850px) { .tool-content { padding: 8px 20px 20px; } .summary-card { padding: 16px; } }
</style>
