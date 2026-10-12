<script setup lang="ts">
import LemonMark from '../layout/LemonMark.vue';
import { useRouter } from 'vue-router';
import { FolderSearch, ScanLine, Trash2, PackageMinus, ShieldCheck, Power, HardDrive, PanelsTopLeft } from 'lucide-vue-next';
const router = useRouter();
const features = [
  { title: '大文件清理', subtitle: '查找50MB以上的文件', icon: FolderSearch, path: '/large-files' },
  { title: '重复文件清理', subtitle: '查找内容一样的文件', icon: ScanLine, path: '/duplicates' },
  { title: '系统垃圾清理', subtitle: '清理应用缓存与历史日志', icon: Trash2, path: '/clean' },
  { title: '应用卸载', subtitle: '管理已安装的应用', icon: PackageMinus, path: '/apps' },
  { title: '软件更新检测', subtitle: '检查可用的软件更新', icon: ShieldCheck, path: '/updates' },
  { title: '开机启动项管理', subtitle: '快速禁用开机自启应用', icon: Power, path: '/autostart' },
  { title: '磁盘空间分析', subtitle: '深度分析存储情况', icon: HardDrive, path: '/storage' },
  { title: '桌面与菜单图标', subtitle: '查看和管理应用入口', icon: PanelsTopLeft, path: '/entries' },
];
</script>

<template>
  <main class="lemon-home">
    <section class="scan-panel">
      <div class="lemon-art" aria-hidden="true">
        <LemonMark />
      </div>
      <h1>畅快清理，清爽一下</h1>
      <p>欢迎使用 Lemon Cleaner</p>
      <button class="scan-button" @click="router.push('/clean')">开始扫描</button>
      <div class="corner-mark" aria-hidden="true">✦</div>
    </section>
    <section class="tools-panel" aria-label="清理与管理工具">
      <div class="features-grid">
        <button v-for="feature in features" :key="feature.path" class="feature" @click="router.push(feature.path)">
          <component :is="feature.icon" :size="38" :stroke-width="1.55" aria-hidden="true" />
          <span><strong>{{ feature.title }}</strong><small>{{ feature.subtitle }}</small></span>
        </button>
      </div>
      <p class="home-caption">软件管家 · 清爽，从这里开始</p>
    </section>
  </main>
</template>

<style scoped>
.lemon-home { display: flex; flex: 1; min-height: 0; height: 100%; background: #fff; }
.scan-panel { width: 38.3%; min-width: 340px; flex-shrink: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; position: relative; border-right: 1px solid #f5f5f5; padding: 0 18px 42px; }
.lemon-art { width: 260px; height: 260px; margin-bottom: 20px; }
.lemon-art svg { width: 100%; height: 100%; }
h1 { font-size: 28px; font-weight: 400; color: #292b2e; margin: 0 0 12px; white-space: nowrap; }
.scan-panel p { font-size: 14px; color: #989a9e; margin: 0 0 48px; }
.scan-button { width: 202px; height: 60px; border: 0; border-radius: 3px; color: #fff; font-size: 22px; background: #00c98d; cursor: pointer; transition: background .15s; }
.scan-button:hover { background: #00b880; }
.corner-mark { position: absolute; bottom: 12px; left: 24px; color: #ffb31a; font-size: 26px; }
.tools-panel { flex: 1; min-width: 0; padding: 62px 28px 14px; display: flex; flex-direction: column; justify-content: space-between; }
.features-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); column-gap: 18px; row-gap: 54px; }
.feature { display: flex; align-items: center; gap: 14px; border: 0; border-radius: 5px; background: transparent; color: #5d6168; padding: 12px 8px; cursor: pointer; text-align: left; font-family: inherit; }
.feature:hover { background: #fafbf9; }
.feature svg { flex-shrink: 0; }
.feature span { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.feature strong { font-size: 15px; font-weight: 400; color: #3f4146; white-space: nowrap; }
.feature small { font-size: 11px; color: #999ca2; white-space: nowrap; }
.home-caption { text-align: right; font-size: 11px; color: #b0b2b7; margin: 20px 0 0; }
@media (max-height: 620px) { .lemon-art { width: 220px; height: 220px; margin-bottom: 14px; } .scan-panel p { margin-bottom: 30px; } .tools-panel { padding-top: 30px; } .features-grid { row-gap: 34px; } }
@media (prefers-reduced-motion: reduce) { * { transition: none; } }
</style>
