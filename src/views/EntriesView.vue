<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import { Eye, EyeOff, Info, LayoutGrid, Monitor, RefreshCw, Search } from 'lucide-vue-next';
import { platformInfo, loadPlatform } from '../platform';
import PageHeader from '../layout/PageHeader.vue';

interface DesktopEntry { path: string; basename: string; name: string; exec: string; scope: string; visible: boolean }
const desktopConnected = isTauri();
const scanning = ref(false);
const loaded = ref(false);
const error = ref('');
const toggling = ref(false);
const desktopEntries = ref<DesktopEntry[]>([]);
const entrySearch = ref('');
const visibleCount = computed(() => desktopEntries.value.filter(entry => entry.visible).length);
const filteredEntries = computed(() => {
  const query = entrySearch.value.toLowerCase();
  return desktopEntries.value.filter(entry => entry.name.toLowerCase().includes(query) || entry.basename.toLowerCase().includes(query));
});
async function loadDesktopEntries() {
  if (!desktopConnected || scanning.value) return;
  scanning.value = true;
  loaded.value = false;
  desktopEntries.value = [];
  error.value = '';
  try {
    await loadPlatform();
    desktopEntries.value = await invoke<DesktopEntry[]>('get_desktop_entries');
    loaded.value = true;
  } catch (cause) { error.value = String(cause); }
  finally { scanning.value = false; }
}
async function handleToggleEntry(entry: DesktopEntry) {
  const visible = entry.visible;
  toggling.value = true;
  try {
    await invoke('toggle_desktop_visibility', { path: entry.path, visible });
    ElMessage.success(`已在菜单中${visible ? '显示' : '隐藏'}：${entry.name}`);
  } catch (cause) {
    entry.visible = !visible;
    ElMessage.error(`修改失败：${String(cause)}`);
    toggling.value = false;
    return;
  }
  await loadDesktopEntries();
  toggling.value = false;
}
onMounted(() => { void loadDesktopEntries(); });
</script>

<template>
  <div class="tool-page">
    <PageHeader title="应用入口" subtitle="整理应用菜单，把常用的留在眼前。">
      <template #actions><el-button :loading="scanning" :disabled="!desktopConnected || toggling" @click="loadDesktopEntries"><RefreshCw :size="14" />刷新列表</el-button></template>
    </PageHeader>
    <div class="tool-content">
      <div v-if="!desktopConnected" class="empty-state"><span class="empty-icon"><Monitor :size="30" /></span><h2>为桌面留一点秩序</h2><p>请在桌面应用中打开，查看本机的应用入口和菜单可见性。</p><span class="state-label">浏览器预览 · 尚未连接桌面端</span></div>
      <template v-else>
        <div class="summary-grid">
          <div class="summary-card featured"><LayoutGrid :size="20" /><span>应用入口</span><strong>{{ loaded ? desktopEntries.length : '—' }}<small>项</small></strong></div>
          <div class="summary-card"><Eye :size="20" /><span>菜单中显示</span><strong>{{ loaded ? visibleCount : '—' }}<small>项</small></strong></div>
          <div class="summary-card"><EyeOff :size="20" /><span>已隐藏</span><strong>{{ loaded ? desktopEntries.length - visibleCount : '—' }}<small>项</small></strong></div>
        </div>
        <el-alert v-if="error" title="应用入口读取未完成" :description="error" type="error" :closable="false" show-icon />
        <div v-if="platformInfo && !platformInfo.desktop_visibility" class="platform-note"><Info :size="15" /><span>此系统的应用入口仅供查看，可见性由系统管理。</span></div>
        <div class="result-heading"><div><h2>应用菜单</h2><p>{{ platformInfo?.desktop_visibility ? '通过开关调整菜单显示，应用本身会保留。' : '查看应用名称、启动命令和入口信息。' }}</p></div><el-input v-model="entrySearch" placeholder="搜索名称或文件名" clearable class="search-input"><template #prefix><Search :size="14" /></template></el-input></div>
        <div v-loading="scanning" class="table-panel">
          <el-table :data="filteredEntries" :empty-text="scanning ? '正在读取应用入口…' : loaded ? (entrySearch ? '没有匹配的应用入口' : '未发现应用入口') : '尚未读取应用入口'">
            <el-table-column label="应用" min-width="190"><template #default="{ row }"><div class="app-cell"><span class="app-icon"><LayoutGrid :size="16" /></span><div><strong>{{ row.name }}</strong><span :title="row.basename">{{ row.basename }}</span></div></div></template></el-table-column>
            <el-table-column label="范围" width="88"><template #default="{ row }"><span class="scope-tag">{{ row.scope }}</span></template></el-table-column>
            <el-table-column prop="exec" label="启动命令" min-width="170" show-overflow-tooltip />
            <el-table-column label="菜单显示" width="96" align="center"><template #default="{ row }"><el-switch v-model="row.visible" :disabled="scanning || toggling || !platformInfo?.desktop_visibility" style="--el-switch-on-color: #64d5e9; --el-switch-off-color: #373348" :aria-label="`${row.name}在菜单中显示`" @change="handleToggleEntry(row)" /></template></el-table-column>
          </el-table>
        </div>
        <p class="footnote">{{ filteredEntries.length }} 个入口{{ entrySearch ? '匹配当前搜索' : '已列出' }} · 隐藏入口不会卸载应用或删除文件。</p>
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
.summary-card.featured { background: radial-gradient(ellipse at top right, #64d5e918, transparent 100%), #ffffff04; border-color: #64d5e922; }
.featured > svg { color: #64d5e9; }
.platform-note { display: flex; gap: 9px; color: #a59dbc; font-size: 11px; line-height: 1.8; padding: 12px 14px; background: #a78bfa08; border-radius: 10px; margin-bottom: 22px; }
.platform-note svg { flex-shrink: 0; margin-top: 2px; color: #a78bfa; }
.result-heading { display: flex; justify-content: space-between; align-items: center; gap: 16px; margin: 0 0 15px; }
.result-heading h2 { margin: 0 0 6px; font-size: 14px; font-weight: 500; color: #f1efff; }
.result-heading p { color: #9693ad; font-size: 11px; line-height: 1.6; margin: 0; }
.search-input { width: 190px; flex-shrink: 0; }
.table-panel { border: 1px solid #ffffff0d; border-radius: 14px; overflow: hidden; }
.app-cell { display: flex; gap: 11px; align-items: center; min-width: 0; }
.app-icon { width: 32px; height: 32px; display: grid; place-items: center; flex-shrink: 0; background: #64d5e909; border: 1px solid #64d5e914; color: #80b9c3; border-radius: 10px; }
.app-cell > div { min-width: 0; }
.app-cell strong { display: block; font-size: 12px; font-weight: 500; color: #e5dff7; }
.app-cell div span { display: block; font-size: 10px; color: #817b97; max-width: 170px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 3px; }
.scope-tag { font-size: 10px; color: #aba3c7; border: 1px solid #a78bfa1a; background: #a78bfa08; border-radius: 5px; padding: 3px 7px; white-space: nowrap; }
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
