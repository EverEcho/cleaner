<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import { Download, RefreshCw, Search, Store } from 'lucide-vue-next';
import catalog from '../../market/catalog.json';
import PageHeader from '../layout/PageHeader.vue';
import { marketCategories, marketPlatforms, parseMarketIndex, type MarketApp, type MarketCategory, type MarketPlatform } from '../market-index';
import { platformInfo } from '../platform';
import { activeTask, runTask } from '../tasks';

interface IndexReport { index: string; source: 'bundled' | 'cached' | 'remote'; warning: string | null }
interface MarketEnvironment { installed: string[]; issue: string | null }
interface InstallSelection { id: string; name: string; package: string; platform: MarketPlatform }

const native = isTauri();
const apps = ref(parseMarketIndex(JSON.stringify(catalog)));
const source = ref<IndexReport['source']>('bundled');
const sourceLabels = { bundled: '内置索引', cached: '上次同步结果', remote: 'GitHub 索引' };
const channelLabels = { linux: 'Flathub', macos: 'Homebrew Cask' };
const refreshing = ref(false);
const refreshError = ref('');
const environment = ref<MarketEnvironment>();
const environmentLoading = ref(false);
const installedPackages = computed(() => new Set(environment.value?.installed ?? []));
const pendingInstall = ref<InstallSelection>();
const dialogReady = ref(false);
const installDialog = computed({
  get: () => !!pendingInstall.value,
  set: (open: boolean) => {
    if (!open) {
      dialogReady.value = false;
      pendingInstall.value = undefined;
    }
  },
});
const installing = ref('');
const installTask = computed(() => activeTask('install_market_app'));
const lastFailure = ref<{ id: string; message: string }>();
const search = ref('');
const category = ref<MarketCategory | 'all'>('all');
const chosenPlatform = ref<MarketPlatform | 'all'>();
const localPlatform = computed<MarketPlatform | undefined>(() => {
  const os = platformInfo.value?.os;
  return os === 'linux' || os === 'macos' ? os : undefined;
});
const platform = computed({
  get: () => chosenPlatform.value ?? localPlatform.value ?? 'all',
  set: value => { chosenPlatform.value = value; },
});
const platformKeys = Object.keys(marketPlatforms) as MarketPlatform[];

const filteredApps = computed(() => {
  const terms = search.value.toLowerCase().trim().split(/\s+/).filter(Boolean);
  return apps.value.filter(app => {
    if (platform.value !== 'all' && !app.install[platform.value]) return false;
    if (category.value !== 'all' && category.value !== app.category) return false;
    const text = `${app.name} ${app.id} ${app.description} ${marketCategories[app.category]} ${Object.values(app.install).join(' ')}`.toLowerCase();
    return terms.every(term => text.includes(term));
  });
});

function localTarget(app: MarketApp) {
  const os = localPlatform.value;
  return os && (platform.value === 'all' || platform.value === os) ? app.install[os] : undefined;
}

async function loadEnvironment() {
  if (!native || environmentLoading.value) return;
  environmentLoading.value = true;
  try {
    environment.value = await invoke<MarketEnvironment>('get_market_environment');
  } catch (error) {
    environment.value = { installed: [], issue: String(error) };
  } finally {
    environmentLoading.value = false;
  }
}

async function refreshIndex() {
  if (!native || refreshing.value) return;
  refreshing.value = true;
  refreshError.value = '';
  try {
    const report = await invoke<IndexReport>('fetch_market_index');
    apps.value = parseMarketIndex(report.index);
    source.value = report.source;
    refreshError.value = report.warning ?? '';
  } catch (error) {
    refreshError.value = error instanceof Error ? error.message : String(error);
  } finally {
    refreshing.value = false;
  }
}

function selectInstall(app: MarketApp) {
  const os = localPlatform.value;
  const target = localTarget(app);
  if (!native || !os || !target || !environment.value || environment.value.issue || installing.value || installTask.value) return;
  pendingInstall.value = { id: app.id, name: app.name, package: target, platform: os };
}

async function confirmInstall() {
  const selection = pendingInstall.value;
  if (!selection || !dialogReady.value || installing.value || installTask.value) return;
  installDialog.value = false;
  installing.value = selection.id;
  lastFailure.value = undefined;
  try {
    await runTask<string>('install_market_app', { id: selection.id, package: selection.package });
    ElMessage.success(`${selection.name} 已安装`);
  } catch (error) {
    lastFailure.value = { id: selection.id, message: String(error) };
    ElMessage.error(`${selection.name} 安装未完成，请查看详情`);
  } finally {
    await loadEnvironment();
    installing.value = '';
  }
}

function resetFilters() {
  search.value = '';
  category.value = 'all';
  platform.value = 'all';
}

onMounted(() => { void refreshIndex(); });
onActivated(() => { if (!installing.value) void loadEnvironment(); });
onDeactivated(() => { installDialog.value = false; });
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="应用市场">
      <template v-if="native" #actions>
        <el-button :icon="RefreshCw" :loading="refreshing" :disabled="!!installing" @click="refreshIndex(); loadEnvironment()">刷新索引</el-button>
      </template>
    </PageHeader>
    <main class="subpage-content">
      <section class="market-intro" aria-labelledby="market-title">
        <div class="market-symbol"><Store :size="25" aria-hidden="true" /></div>
        <div>
          <h2 id="market-title">发现适合你的桌面应用</h2>
          <p>查找 Linux 和 macOS 应用，并通过本机软件渠道直接下载安装。</p>
        </div>
      </section>
      <el-alert
        v-if="environment?.issue" class="market-environment" type="warning" :closable="false"
        :title="environment.issue" show-icon
      />
      <p v-else-if="!native" class="platform-note">请在 AppClear 桌面端下载安装。</p>

      <div class="market-filters" aria-label="应用筛选">
        <el-input v-model="search" :prefix-icon="Search" placeholder="搜索应用名称或关键词" aria-label="搜索应用" clearable />
        <el-select v-model="platform" class="market-select" aria-label="筛选平台">
          <el-option label="全部平台" value="all" />
          <el-option v-for="(label, key) in marketPlatforms" :key="key" :label="label" :value="key" />
        </el-select>
        <el-select v-model="category" class="market-select" aria-label="筛选分类">
          <el-option label="全部分类" value="all" />
          <el-option v-for="(label, key) in marketCategories" :key="key" :label="label" :value="key" />
        </el-select>
      </div>
      <div class="market-summary" aria-live="polite">
        <span>{{ filteredApps.length }} 款应用</span>
        <span v-if="refreshing">正在同步 GitHub 索引 · {{ sourceLabels[source] }}</span>
        <span v-else-if="refreshError" class="market-warning" :title="refreshError">
          GitHub 同步失败 · 正在显示{{ sourceLabels[source] }}
        </span>
        <span v-else>{{ sourceLabels[source] }}</span>
      </div>

      <ul v-if="filteredApps.length" class="market-grid" aria-label="应用列表">
        <li v-for="app in filteredApps" :key="app.id" class="market-card">
          <div class="market-card-title">
            <span class="market-app-icon" :class="app.category" aria-hidden="true">{{ app.name.charAt(0).toUpperCase() }}</span>
            <div>
              <h3>{{ app.name }}</h3>
              <span>{{ marketCategories[app.category] }}</span>
            </div>
          </div>
          <p class="market-description">{{ app.description }}</p>
          <div class="market-meta">
            <span v-for="target in platformKeys.filter(key => app.install[key])" :key="target">{{ marketPlatforms[target] }}</span>
            <span v-if="app.upstream_version" class="market-version">上游 {{ app.upstream_version }}</span>
          </div>
          <div class="market-install">
            <template v-if="localPlatform && localTarget(app)">
              <span>{{ channelLabels[localPlatform] }}</span>
              <span v-if="installedPackages.has(localTarget(app)!)" class="market-installed">已安装</span>
              <el-button
                v-else type="primary" plain size="small" :icon="Download"
                :aria-label="`安装 ${app.name}`" :loading="installing === app.id"
                :disabled="!environment || !!environment.issue || !!installTask || !!installing || refreshing"
                @click="selectInstall(app)"
              >安装</el-button>
            </template>
            <span v-else-if="native">暂无当前平台安装渠道</span>
            <span v-else>桌面端可安装</span>
          </div>
          <p v-if="installing === app.id" class="market-phase" role="status">{{ installTask?.phase ?? '正在核对安装状态' }}</p>
          <details v-if="lastFailure?.id === app.id" class="market-warning market-failure">
            <summary>安装未完成</summary>
            <p>{{ lastFailure.message }}</p>
          </details>
        </li>
      </ul>
      <el-empty v-else description="没有符合条件的应用">
        <el-button @click="resetFilters">清除筛选</el-button>
      </el-empty>
    </main>
    <el-dialog
      v-model="installDialog" :title="`安装 ${pendingInstall?.name ?? '应用'}`" width="440"
      destroy-on-close @opened="dialogReady = !!pendingInstall"
    >
      <div v-if="pendingInstall" class="market-confirm">
        <p>通过 {{ channelLabels[pendingInstall.platform] }} 下载并安装 {{ pendingInstall.name }}。</p>
        <code>{{ pendingInstall.package }}</code>
        <p v-if="pendingInstall.platform === 'linux'">安装到当前用户。首次使用时会添加用户级 Flathub 软件源，所需运行时会随应用安装。</p>
        <p v-else>Homebrew 会将应用安装到其配置的应用目录，并处理所需依赖。</p>
      </div>
      <template #footer>
        <el-button @click="installDialog = false">取消</el-button>
        <el-button type="primary" :disabled="!dialogReady" @click="confirmInstall">确认安装</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.market-intro { display: flex; align-items: center; gap: 16px; padding: 19px 20px; margin-bottom: 20px; border: 1px solid var(--accent-border); border-radius: 14px; background: linear-gradient(110deg, var(--accent-soft), var(--surface)); }
.market-symbol { display: grid; place-items: center; flex: 0 0 49px; height: 49px; border-radius: 12px; background: var(--surface); color: var(--accent); }
.market-intro h2 { margin: 0 0 6px; font-size: 20px; font-weight: 600; }
.market-intro p { margin: 0; color: var(--text-muted); font-size: 13px; line-height: 1.5; }
.market-environment { margin-bottom: 16px; overflow-wrap: anywhere; }
.market-filters { display: flex; flex-wrap: wrap; gap: 8px; }
.market-filters .el-input { flex: 1 1 220px; }
.market-select { width: 132px; }
.market-summary { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 6px 12px; margin: 13px 0; color: var(--text-muted); font-size: 12px; }
.market-warning { color: var(--warning); }
.market-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 215px), 1fr)); gap: 12px; padding: 0; margin: 0; list-style: none; }
.market-card { display: flex; flex-direction: column; min-width: 0; padding: 14px; border: 1px solid var(--line); border-radius: 12px; background: var(--surface); }
.market-card-title { display: flex; align-items: center; gap: 10px; }
.market-card-title h3 { margin: 0 0 3px; font-size: 14px; font-weight: 600; overflow-wrap: anywhere; }
.market-card-title div > span { font-size: 11px; color: var(--text-muted); }
.market-app-icon { display: grid; place-items: center; flex: 0 0 36px; height: 36px; border-radius: 9px; background: var(--accent-soft); color: var(--accent); font-size: 16px; font-weight: 650; }
.market-app-icon.browsers { background: var(--blue-soft); color: var(--blue); }
.market-app-icon.communication { background: var(--cyan-soft); color: var(--cyan); }
.market-app-icon.media { background: var(--pink-soft); color: var(--pink); }
.market-app-icon.security { background: var(--success-soft); color: var(--success); }
.market-description { margin: 11px 0 12px; flex: 1; color: var(--text-secondary); font-size: 12px; line-height: 1.6; overflow-wrap: anywhere; }
.market-meta, .market-install { display: flex; flex-wrap: wrap; gap: 6px; }
.market-meta span { padding: 2px 6px; border-radius: 4px; background: var(--surface-raised); color: var(--text-muted); font-size: 10px; }
.market-version { max-width: 100%; overflow-wrap: anywhere; }
.market-install { align-items: center; justify-content: space-between; min-height: 38px; margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--line); color: var(--text-muted); font-size: 12px; }
.market-installed { color: var(--success); }
.market-phase, .market-failure { margin: 6px 0 0; font-size: 11px; line-height: 1.6; }
.market-failure summary { cursor: pointer; }
.market-failure p { margin: 4px 0; white-space: pre-wrap; overflow-wrap: anywhere; }
.market-confirm p { margin: 0 0 12px; line-height: 1.65; }
.market-confirm code { display: block; margin-bottom: 12px; color: var(--accent); overflow-wrap: anywhere; }
</style>
