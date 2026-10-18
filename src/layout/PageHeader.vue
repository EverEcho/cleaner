<script setup lang="ts">
import { computed } from 'vue';
import { useRoute } from 'vue-router';
import { Trash2, AppWindow, ArrowUpCircle, Power, PanelsTopLeft, Files, Copy, HardDrive,Activity } from 'lucide-vue-next';
const props = defineProps<{ title: string; subtitle?: string }>();
const route = useRoute();
const icons: Record<string, typeof Trash2> = { '/monitor':Activity, '/clean': Trash2, '/apps': AppWindow, '/updates': ArrowUpCircle, '/autostart': Power, '/entries': PanelsTopLeft, '/large-files': Files, '/duplicates': Copy, '/storage': HardDrive };
const subtitles: Record<string, string> = { '/clean': '找回被缓存与历史文件占据的空间', '/apps': '管理应用，从安装到告别', '/updates': '让常用软件，保持新鲜', '/autostart': '让每一次启动，轻装上阵', '/entries': '整理桌面与应用菜单中的入口', '/large-files': '找到藏在磁盘里的大家伙', '/duplicates': '一样的内容，一份就够了', '/storage': '看清空间，找到整理的方向' };
const icon = computed(() => icons[route.path] ?? HardDrive);
const description = computed(() => props.subtitle ?? subtitles[route.path] ?? '');
</script>

<template>
  <header class="subpage-header">
    <div class="header-left">
      <span class="page-icon"><component :is="icon" :size="23" :stroke-width="1.5" aria-hidden="true" /></span>
      <div class="page-heading"><h1>{{ title }}</h1><p v-if="description">{{ description }}</p></div>
    </div>
    <div v-if="$slots.actions" class="header-actions"><slot name="actions" /></div>
  </header>
</template>

<style scoped>
.subpage-header { min-height: 100px; flex-shrink: 0; display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 20px 28px; border-bottom: 1px solid var(--line); flex-wrap: wrap; }
.header-left { display: flex; align-items: center; gap: 14px; min-width: 0; }
.page-icon { width: 47px; height: 47px; border-radius: 15px; color: #cab3ff; background: linear-gradient(145deg, #ab7df025, #8258d40d); border: 1px solid #b390ed25; display: grid; place-items: center; flex-shrink: 0; box-shadow: inset 0 1px 0 #c7acff10; }
h1 { font-size: 23px; font-weight: 600; color: var(--text); margin: 0; letter-spacing: -.5px; }
p { font-size: 11px; color: var(--text-muted); margin: 6px 0 0; line-height: 1.6; }
.header-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; justify-content: flex-end; min-width: 0; }
@media (max-width: 1050px) { .subpage-header { padding: 18px 22px; gap: 14px; } .header-actions { justify-content: flex-start; } }
@media (max-width: 700px) { .subpage-header { padding: 18px; } h1 { font-size: 21px; } .page-icon { width: 41px; height: 41px; border-radius: 13px; } }
</style>
